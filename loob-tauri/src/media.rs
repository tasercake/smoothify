use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    header::{
        ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, ALLOW, CACHE_CONTROL, CONTENT_LENGTH,
        CONTENT_RANGE, CONTENT_TYPE, HOST, ORIGIN, VARY,
    },
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    convert::Infallible,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    sync::oneshot,
};
use tokio_util::io::ReaderStream;

type MediaBody = BoxBody<Bytes, std::io::Error>;

#[derive(Clone)]
pub struct MediaRegistry {
    inner: Arc<Mutex<RegistryInner>>,
    authority: Arc<MediaAuthority>,
}

struct MediaAuthority {
    address: SocketAddrV4,
    session_secret: [u8; 32],
    session_path: String,
}

#[derive(Default)]
struct RegistryInner {
    generation: u64,
    paths: HashMap<String, PathBuf>,
}

pub struct MediaServer {
    registry: MediaRegistry,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

impl MediaServer {
    pub fn start() -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("Could not start local audio playback: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("Could not configure local audio playback: {error}"))?;
        let address = match listener.local_addr().map_err(|error| error.to_string())? {
            SocketAddr::V4(address) if address.ip().is_loopback() => address,
            _ => return Err("Local audio playback did not bind to IPv4 loopback.".into()),
        };
        let mut session_secret = [0_u8; 32];
        OsRng.fill_bytes(&mut session_secret);
        let authority = Arc::new(MediaAuthority {
            address,
            session_path: hex(&session_secret),
            session_secret,
        });
        let registry = MediaRegistry {
            inner: Arc::new(Mutex::new(RegistryInner::default())),
            authority,
        };
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let server_registry = registry.clone();
        tauri::async_runtime::spawn(async move {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => listener,
                Err(_) => return,
            };
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((stream, peer)) = accepted else { continue };
                        if !peer.ip().is_loopback() {
                            continue;
                        }
                        let registry = server_registry.clone();
                        tauri::async_runtime::spawn(async move {
                            let service = service_fn(move |request| {
                                let registry = registry.clone();
                                async move {
                                    Ok::<_, Infallible>(registry.http_response(request).await)
                                }
                            });
                            let _ = http1::Builder::new()
                                .serve_connection(TokioIo::new(stream), service)
                                .await;
                        });
                    }
                }
            }
        });
        Ok(Self {
            registry,
            shutdown: Mutex::new(Some(shutdown_tx)),
        })
    }

    pub fn registry(&self) -> MediaRegistry {
        self.registry.clone()
    }

    #[cfg(test)]
    fn local_addr(&self) -> SocketAddrV4 {
        self.registry.authority.address
    }
}

impl Drop for MediaServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self
            .shutdown
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            let _ = shutdown.send(());
        }
    }
}

impl MediaRegistry {
    pub fn invalidate(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.generation = inner.generation.wrapping_add(1).max(1);
        inner.paths.clear();
    }

    pub fn replace(&self, paths: &[PathBuf]) -> Result<Vec<String>, String> {
        let canonical = paths
            .iter()
            .map(|path| {
                let path = path
                    .canonicalize()
                    .map_err(|_| "A playlist audio file is no longer available.".to_string())?;
                if !path.is_file() {
                    return Err("A playlist audio entry is not a regular file.".into());
                }
                Ok(path)
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.generation = inner.generation.wrapping_add(1).max(1);
        let generation = inner.generation;
        inner.paths.clear();
        let mut urls = Vec::with_capacity(canonical.len());
        for (index, path) in canonical.into_iter().enumerate() {
            let token = media_token(&self.authority.session_secret, generation, index, &path);
            inner.paths.insert(token.clone(), path);
            urls.push(format!(
                "http://{}/media/{}/{generation}/{token}",
                self.authority.address, self.authority.session_path
            ));
        }
        Ok(urls)
    }

    fn resolve_request_path(&self, request_path: &str) -> Option<PathBuf> {
        if request_path.contains('%') || request_path.contains("..") || request_path.contains('\\')
        {
            return None;
        }
        let mut segments = request_path.trim_start_matches('/').split('/');
        if segments.next()? != "media" || segments.next()? != self.authority.session_path {
            return None;
        }
        let generation = segments.next()?.parse::<u64>().ok()?;
        let token = segments.next()?;
        if segments.next().is_some()
            || token.len() != 64
            || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        (generation == inner.generation)
            .then(|| inner.paths.get(token).cloned())
            .flatten()
    }

    async fn http_response(&self, request: Request<Incoming>) -> Response<MediaBody> {
        let expected_host = self.authority.address.to_string();
        if request
            .headers()
            .get(HOST)
            .and_then(|value| value.to_str().ok())
            != Some(expected_host.as_str())
        {
            return empty_response(StatusCode::MISDIRECTED_REQUEST, &request);
        }
        if request.method() != Method::GET && request.method() != Method::HEAD {
            return response_builder(StatusCode::METHOD_NOT_ALLOWED, &request)
                .header(ALLOW, "GET, HEAD")
                .body(empty_body())
                .unwrap();
        }
        let Some(path) = self.resolve_request_path(request.uri().path()) else {
            return empty_response(StatusCode::NOT_FOUND, &request);
        };
        file_response(&path, &request)
            .await
            .unwrap_or_else(|_| empty_response(StatusCode::NOT_FOUND, &request))
    }
}

fn media_token(secret: &[u8; 32], generation: u64, index: usize, path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hasher.update(generation.to_le_bytes());
    hasher.update(index.to_le_bytes());
    hasher.update(path.as_os_str().as_encoded_bytes());
    format!("{:x}", hasher.finalize())
}

async fn file_response(
    path: &Path,
    request: &Request<Incoming>,
) -> Result<Response<MediaBody>, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    let length = file
        .metadata()
        .await
        .map_err(|error| error.to_string())?
        .len();
    let content_type = media_content_type(path);
    let range = request
        .headers()
        .get("range")
        .and_then(|value| value.to_str().ok());

    if let Some(value) = range {
        let Some((start, end)) = parse_range(value, length) else {
            return Ok(response_builder(StatusCode::RANGE_NOT_SATISFIABLE, request)
                .header(CONTENT_RANGE, format!("bytes */{length}"))
                .header(ACCEPT_RANGES, "bytes")
                .header(CONTENT_LENGTH, 0)
                .body(empty_body())
                .unwrap());
        };
        let response_length = end - start + 1;
        let body = if request.method() == Method::GET {
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|error| error.to_string())?;
            stream_body(file.take(response_length))
        } else {
            empty_body()
        };
        return Ok(response_builder(StatusCode::PARTIAL_CONTENT, request)
            .header(CONTENT_TYPE, content_type)
            .header(ACCEPT_RANGES, "bytes")
            .header(CONTENT_RANGE, format!("bytes {start}-{end}/{length}"))
            .header(CONTENT_LENGTH, response_length)
            .body(body)
            .unwrap());
    }

    let body = if request.method() == Method::GET {
        stream_body(file.take(length))
    } else {
        empty_body()
    };
    Ok(response_builder(StatusCode::OK, request)
        .header(CONTENT_TYPE, content_type)
        .header(ACCEPT_RANGES, "bytes")
        .header(CONTENT_LENGTH, length)
        .body(body)
        .unwrap())
}

fn stream_body(reader: tokio::io::Take<tokio::fs::File>) -> MediaBody {
    let stream = ReaderStream::new(reader).map_ok(Frame::data);
    BodyExt::boxed(StreamBody::new(stream))
}

fn empty_body() -> MediaBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn parse_range(value: &str, length: u64) -> Option<(u64, u64)> {
    if length == 0 {
        return None;
    }
    let value = value.strip_prefix("bytes=")?;
    if value.contains(',') {
        return None;
    }
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?.min(length);
        return (suffix > 0).then_some((length - suffix, length - 1));
    }
    let start = start.parse::<u64>().ok()?;
    if start >= length {
        return None;
    }
    let end = if end.is_empty() {
        length - 1
    } else {
        end.parse::<u64>().ok()?.min(length - 1)
    };
    (end >= start).then_some((start, end))
}

fn media_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn response_builder(
    status: StatusCode,
    request: &Request<Incoming>,
) -> hyper::http::response::Builder {
    let mut builder = Response::builder()
        .status(status)
        .header(CACHE_CONTROL, "no-store")
        .header("Cross-Origin-Resource-Policy", "cross-origin")
        .header("X-Content-Type-Options", "nosniff");
    if let Some(origin) = request
        .headers()
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .filter(|origin| {
            matches!(
                *origin,
                "tauri://localhost" | "http://tauri.localhost" | "https://tauri.localhost"
            )
        })
    {
        builder = builder
            .header(ACCESS_CONTROL_ALLOW_ORIGIN, origin)
            .header(VARY, "Origin");
    }
    builder
}

fn empty_response(status: StatusCode, request: &Request<Incoming>) -> Response<MediaBody> {
    response_builder(status, request)
        .header(CONTENT_LENGTH, 0)
        .body(empty_body())
        .unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        fs,
        io::{Read, Write},
        net::TcpStream,
        time::Duration,
    };

    struct TestResponse {
        status: u16,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    fn request(
        address: SocketAddrV4,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
    ) -> TestResponse {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: {host}\r\n").unwrap();
        for (name, value) in headers {
            write!(stream, "{name}: {value}\r\n").unwrap();
        }
        write!(stream, "Connection: close\r\n\r\n").unwrap();
        stream.flush().unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let split = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .unwrap();
        let head = String::from_utf8(bytes[..split].to_vec()).unwrap();
        let mut lines = head.lines();
        let status = lines
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        TestResponse {
            status,
            headers,
            body: bytes[split + 4..].to_vec(),
        }
    }

    fn url_parts(url: &str) -> (String, String) {
        let url = tauri::Url::parse(url).unwrap();
        (
            url.host_str().unwrap().to_string() + ":" + &url.port().unwrap().to_string(),
            url.path().to_string(),
        )
    }

    #[test]
    fn loopback_server_enforces_authority_and_serves_seekable_media() {
        let server = MediaServer::start().unwrap();
        assert_eq!(*server.local_addr().ip(), Ipv4Addr::LOCALHOST);
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("track.wav");
        fs::write(&audio, b"0123456789").unwrap();
        let url = server.registry().replace(&[audio]).unwrap()[0].clone();
        let (host, path) = url_parts(&url);

        let get = request(server.local_addr(), &host, "GET", &path, &[]);
        assert_eq!(get.status, 200);
        assert_eq!(get.body, b"0123456789");
        assert_eq!(get.headers["content-type"], "audio/wav");
        assert_eq!(get.headers["accept-ranges"], "bytes");

        let head = request(server.local_addr(), &host, "HEAD", &path, &[]);
        assert_eq!(head.status, 200);
        assert!(head.body.is_empty());
        assert_eq!(head.headers["content-length"], "10");

        let bounded = request(
            server.local_addr(),
            &host,
            "GET",
            &path,
            &[("Range", "bytes=3-6")],
        );
        assert_eq!(bounded.status, 206);
        assert_eq!(bounded.body, b"3456");
        assert_eq!(bounded.headers["content-range"], "bytes 3-6/10");

        let open = request(
            server.local_addr(),
            &host,
            "GET",
            &path,
            &[("Range", "bytes=7-")],
        );
        assert_eq!(open.body, b"789");
        assert_eq!(open.headers["content-range"], "bytes 7-9/10");

        let suffix = request(
            server.local_addr(),
            &host,
            "GET",
            &path,
            &[("Range", "bytes=-4")],
        );
        assert_eq!(suffix.body, b"6789");
        assert_eq!(suffix.headers["content-range"], "bytes 6-9/10");

        let invalid = request(
            server.local_addr(),
            &host,
            "GET",
            &path,
            &[("Range", "bytes=99-")],
        );
        assert_eq!(invalid.status, 416);
        assert_eq!(invalid.headers["content-range"], "bytes */10");

        assert_eq!(
            request(server.local_addr(), &host, "POST", &path, &[]).status,
            405
        );
        assert_eq!(
            request(server.local_addr(), "attacker.invalid", "GET", &path, &[]).status,
            421
        );
        assert_eq!(
            request(server.local_addr(), &host, "GET", "/media/../secret", &[]).status,
            404
        );

        let mut unknown = path.clone();
        unknown.pop();
        unknown.push(if path.ends_with('a') { 'b' } else { 'a' });
        assert_eq!(
            request(server.local_addr(), &host, "GET", &unknown, &[]).status,
            404
        );

        let replacement = dir.path().join("replacement.wav");
        fs::write(&replacement, b"replacement").unwrap();
        server.registry().replace(&[replacement]).unwrap();
        assert_eq!(
            request(server.local_addr(), &host, "GET", &path, &[]).status,
            404
        );
    }

    #[test]
    fn compact_cached_formats_have_specific_media_types() {
        assert_eq!(media_content_type(Path::new("track.m4a")), "audio/mp4");
        assert_eq!(media_content_type(Path::new("track.aac")), "audio/aac");
        assert_eq!(media_content_type(Path::new("legacy.wav")), "audio/wav");
    }

    #[test]
    fn large_open_range_returns_the_complete_requested_remainder() {
        let server = MediaServer::start().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("large.mp3");
        let length = 1024 * 1024 + 4096;
        fs::write(&audio, vec![7_u8; length]).unwrap();
        let url = server.registry().replace(&[audio]).unwrap()[0].clone();
        let (host, path) = url_parts(&url);
        let response = request(
            server.local_addr(),
            &host,
            "GET",
            &path,
            &[("Range", "bytes=0-")],
        );
        assert_eq!(response.status, 206);
        assert_eq!(response.body.len(), length);
        assert_eq!(
            response.headers["content-range"],
            format!("bytes 0-{}/{}", length - 1, length)
        );
        assert_eq!(response.headers["content-length"], length.to_string());
    }
}
