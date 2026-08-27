use crate::{DspSummary, TrackAnalysis};
use rustfft::{num_complex::Complex, FftPlanner};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

pub const ANALYSIS_PIPELINE_VERSION: &str = "dsp-v1-fft2048-hop1024";
const FFT_SIZE: usize = 2048;
const BASE_HOP: usize = 1024;
const MAX_GLOBAL_FRAMES: usize = 4096;

pub fn analysis_fingerprint(intro_seconds: f64, outro_seconds: f64) -> String {
    format!("{ANALYSIS_PIPELINE_VERSION}-intro-{intro_seconds:.3}-outro-{outro_seconds:.3}")
}

pub fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn analyze_audio(
    path: &Path,
    intro_seconds: f64,
    outro_seconds: f64,
) -> Result<TrackAnalysis, String> {
    let content_sha256 = hash_file(path)?;
    let (samples, sample_rate) = decode_mono(path)?;
    if samples.is_empty() {
        return Err("decoded audio contains no samples".into());
    }
    let duration_seconds = samples.len() as f64 / sample_rate as f64;
    let intro_len = ((intro_seconds * sample_rate as f64) as usize).clamp(1, samples.len());
    let outro_len = ((outro_seconds * sample_rate as f64) as usize).clamp(1, samples.len());

    Ok(TrackAnalysis {
        pipeline_version: ANALYSIS_PIPELINE_VERSION.into(),
        analysis_fingerprint: analysis_fingerprint(intro_seconds, outro_seconds),
        content_sha256,
        sample_rate,
        duration_seconds,
        intro: summarize(&samples[..intro_len], sample_rate, BASE_HOP),
        outro: summarize(&samples[samples.len() - outro_len..], sample_rate, BASE_HOP),
        whole: summarize(
            &samples,
            sample_rate,
            (samples.len() / MAX_GLOBAL_FRAMES).max(BASE_HOP),
        ),
    })
}

fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let file = Box::new(File::open(path).map_err(|e| e.to_string())?);
    let mss = MediaSourceStream::new(file, MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|v| v.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "no decodable audio track".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut mono = Vec::new();
    let mut sample_rate = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(err) => return Err(err.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => return Err(err.to_string()),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        sample_rate.get_or_insert(spec.rate);
        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        for frame in sample_buf.samples().chunks(channels) {
            mono.push(frame.iter().copied().sum::<f32>() / channels as f32);
        }
    }
    Ok((
        mono,
        sample_rate.ok_or_else(|| "audio stream has no sample rate".to_string())?,
    ))
}

fn summarize(samples: &[f32], sample_rate: u32, hop: usize) -> DspSummary {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos())
        .collect();
    let mut starts: Vec<usize> = if samples.len() <= FFT_SIZE {
        vec![0]
    } else {
        (0..=samples.len() - FFT_SIZE).step_by(hop.max(1)).collect()
    };
    if samples.len() > FFT_SIZE && starts.last().copied() != Some(samples.len() - FFT_SIZE) {
        starts.push(samples.len() - FFT_SIZE);
    }

    let mut rms_sum = 0.0_f64;
    let mut zcr_sum = 0.0_f64;
    let mut centroid_sum = 0.0_f64;
    let mut rolloff_sum = 0.0_f64;
    let mut flatness_sum = 0.0_f64;
    let mut flux_values = Vec::with_capacity(starts.len());
    let mut chroma = [0.0_f64; 12];
    let mut previous = vec![0.0_f32; FFT_SIZE / 2 + 1];

    for start in starts.iter().copied() {
        let mut input = vec![Complex::new(0.0, 0.0); FFT_SIZE];
        let available = (samples.len() - start).min(FFT_SIZE);
        let frame = &samples[start..start + available];
        let rms =
            (frame.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / available as f64).sqrt();
        rms_sum += rms;
        zcr_sum += frame
            .windows(2)
            .filter(|w| w[0].is_sign_positive() != w[1].is_sign_positive())
            .count() as f64
            / available.saturating_sub(1).max(1) as f64;
        for i in 0..available {
            input[i].re = frame[i] * window[i];
        }
        fft.process(&mut input);
        let magnitudes: Vec<f32> = input[..=FFT_SIZE / 2].iter().map(|v| v.norm()).collect();
        let total = magnitudes.iter().map(|v| *v as f64).sum::<f64>().max(1e-12);
        let nyquist = sample_rate as f64 / 2.0;
        centroid_sum += magnitudes
            .iter()
            .enumerate()
            .map(|(i, v)| i as f64 * *v as f64)
            .sum::<f64>()
            / total
            / (FFT_SIZE / 2) as f64;
        let threshold = total * 0.85;
        let mut cumulative = 0.0;
        let mut rolloff_bin = 0;
        for (i, value) in magnitudes.iter().enumerate() {
            cumulative += *value as f64;
            if cumulative >= threshold {
                rolloff_bin = i;
                break;
            }
        }
        rolloff_sum += rolloff_bin as f64 / (FFT_SIZE / 2) as f64;
        let arithmetic = total / magnitudes.len() as f64;
        let geometric = (magnitudes
            .iter()
            .map(|v| (*v as f64 + 1e-12).ln())
            .sum::<f64>()
            / magnitudes.len() as f64)
            .exp();
        flatness_sum += (geometric / arithmetic.max(1e-12)).min(1.0);
        let flux = magnitudes
            .iter()
            .zip(&previous)
            .map(|(v, p)| (*v - *p).max(0.0) as f64)
            .sum::<f64>()
            / total;
        flux_values.push(flux);
        previous.copy_from_slice(&magnitudes);

        for (bin, magnitude) in magnitudes.iter().enumerate().skip(1) {
            let frequency = bin as f64 * nyquist / (FFT_SIZE / 2) as f64;
            if frequency < 40.0 {
                continue;
            }
            let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
            let pitch_class = midi.round() as i32;
            chroma[pitch_class.rem_euclid(12) as usize] += *magnitude as f64;
        }
    }

    let n = starts.len() as f64;
    let mean_flux = flux_values.iter().sum::<f64>() / n;
    let variance = flux_values
        .iter()
        .map(|v| (v - mean_flux).powi(2))
        .sum::<f64>()
        / n;
    let onset_threshold = mean_flux + variance.sqrt();
    let onsets = flux_values.iter().filter(|v| **v > onset_threshold).count() as f64;
    let analyzed_seconds =
        ((samples.len().saturating_sub(1)) as f64 / sample_rate as f64).max(0.001);
    let chroma_total = chroma.iter().sum::<f64>();
    if chroma_total > 0.0 {
        for value in &mut chroma {
            *value /= chroma_total;
        }
    }
    let mean_rms = rms_sum / n;

    DspSummary {
        rms_db: 20.0 * mean_rms.max(1e-9).log10(),
        spectral_centroid: centroid_sum / n,
        spectral_rolloff: rolloff_sum / n,
        spectral_flatness: flatness_sum / n,
        spectral_flux: mean_flux,
        zero_crossing_rate: zcr_sum / n,
        onset_density: onsets / analyzed_seconds,
        chroma,
    }
}
