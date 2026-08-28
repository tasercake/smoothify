use crate::{analyze_audio_with_hash, TrackAnalysis, ANALYSIS_PIPELINE_VERSION};
use atomicwrites::{AllowOverwrite, AtomicFile};
use fs2::FileExt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
}

#[derive(Debug, Clone)]
pub struct FeatureCache {
    root: PathBuf,
}

impl FeatureCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load_or_analyze(&self, path: &Path) -> Result<(TrackAnalysis, CacheStatus), String> {
        let hash = crate::hash_file(path)?;
        let fingerprint = crate::analysis_fingerprint();
        self.get_or_compute(&hash, &fingerprint, || analyze_audio_with_hash(path, &hash))
    }

    pub fn load_or_analyze_known_hash(
        &self,
        path: &Path,
        content_hash: &str,
    ) -> Result<(TrackAnalysis, CacheStatus), String> {
        if content_hash.len() != 64
            || !content_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("trusted content hash is not a lowercase SHA-256 value".into());
        }
        let fingerprint = crate::analysis_fingerprint();
        self.get_or_compute(content_hash, &fingerprint, || {
            match analyze_audio_with_hash(path, content_hash) {
                Ok(analysis) => Ok(analysis),
                Err(decode_error) => {
                    // A decode failure is unusual enough to justify strong
                    // validation. Cache I/O errors outside this closure do not
                    // trigger an expensive audio pass.
                    let actual = crate::hash_file(path)?;
                    if actual != content_hash {
                        Err("trusted audio object changed and failed integrity validation".into())
                    } else {
                        Err(decode_error)
                    }
                }
            }
        })
    }

    pub fn get_or_compute<F>(
        &self,
        content_hash: &str,
        analysis_fingerprint: &str,
        compute: F,
    ) -> Result<(TrackAnalysis, CacheStatus), String>
    where
        F: FnOnce() -> Result<TrackAnalysis, String>,
    {
        let feature_dir = self.root.join("features");
        let lock_dir = self.root.join("locks");
        fs::create_dir_all(&feature_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(&lock_dir).map_err(|e| e.to_string())?;
        let fingerprint = analysis_fingerprint.replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let feature_path = feature_dir.join(format!("{content_hash}-{fingerprint}.json"));
        let lock_path = lock_dir.join(format!("feature-{content_hash}-{fingerprint}.lock"));
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        lock.lock_exclusive().map_err(|e| e.to_string())?;

        let result = match read_valid(&feature_path, content_hash, analysis_fingerprint) {
            Some(value) => Ok((value, CacheStatus::Hit)),
            None => {
                let value = compute()?;
                if value.content_sha256 != content_hash
                    || value.pipeline_version != ANALYSIS_PIPELINE_VERSION
                    || value.analysis_fingerprint != analysis_fingerprint
                    || !value.has_valid_chunk_timeline()
                {
                    return Err(
                        "analysis result does not match the cache key, pipeline version, or chunk timeline"
                            .into(),
                    );
                }
                atomic_write_json(&feature_path, &value)?;
                Ok((value, CacheStatus::Miss))
            }
        };
        let _ = lock.unlock();
        result
    }
}

fn read_valid(
    path: &Path,
    expected_hash: &str,
    expected_fingerprint: &str,
) -> Option<TrackAnalysis> {
    let bytes = fs::read(path).ok()?;
    let value: TrackAnalysis = serde_json::from_slice(&bytes).ok()?;
    (value.content_sha256 == expected_hash
        && value.pipeline_version == ANALYSIS_PIPELINE_VERSION
        && value.analysis_fingerprint == expected_fingerprint
        && value.has_valid_chunk_timeline())
    .then_some(value)
}

pub(crate) fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cache path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .map_err(|e| e.to_string())?;
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

use std::fs::File;
