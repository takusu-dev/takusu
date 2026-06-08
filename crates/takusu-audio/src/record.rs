use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecorderError {
    #[error("no input device")]
    NoInputDevice,
    #[error("cpal: {0}")]
    Cpal(String),
    #[error("unsupported sample format")]
    UnsupportedFormat,
}

impl From<cpal::DefaultStreamConfigError> for RecorderError {
    fn from(e: cpal::DefaultStreamConfigError) -> Self {
        Self::Cpal(e.to_string())
    }
}

impl From<cpal::BuildStreamError> for RecorderError {
    fn from(e: cpal::BuildStreamError) -> Self {
        Self::Cpal(e.to_string())
    }
}

impl From<cpal::PlayStreamError> for RecorderError {
    fn from(e: cpal::PlayStreamError) -> Self {
        Self::Cpal(e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct RecordConfig {
    pub silence_threshold: f32,
    pub silence_duration: Duration,
    pub max_duration: Duration,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            silence_threshold: 0.02,
            silence_duration: Duration::from_secs(2),
            max_duration: Duration::from_secs(300),
        }
    }
}

pub fn record(config: &RecordConfig) -> Result<Vec<f32>, RecorderError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(RecorderError::NoInputDevice)?;
    let device_config = device.default_input_config()?;
    let device_sample_rate = device_config.sample_rate();
    let channels = device_config.channels() as usize;
    let sample_format = device_config.sample_format();
    let stream_config: cpal::StreamConfig = device_config.into();

    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let stopped = Arc::new(AtomicBool::new(false));
    let speech_started = Arc::new(AtomicBool::new(false));
    let last_speech_idx = Arc::new(AtomicU64::new(0));

    let error_fn = |err: cpal::StreamError| {
        eprintln!("audio stream error: {err}");
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let samples_c = samples.clone();
            let stopped_c = stopped.clone();
            let speech_started_c = speech_started.clone();
            let last_speech_idx_c = last_speech_idx.clone();
            let threshold = config.silence_threshold;

            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stopped_c.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples_c.try_lock() {
                        let start = buf.len();
                        buf.extend_from_slice(data);
                        if rms(data) > threshold {
                            speech_started_c.store(true, Ordering::Relaxed);
                            last_speech_idx_c.store((start + data.len()) as u64, Ordering::Relaxed);
                        }
                    }
                },
                error_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let samples_c = samples.clone();
            let stopped_c = stopped.clone();
            let speech_started_c = speech_started.clone();
            let last_speech_idx_c = last_speech_idx.clone();
            let threshold = config.silence_threshold;

            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if stopped_c.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples_c.try_lock() {
                        let start = buf.len();
                        let buf_slice = buf.spare_capacity_mut();
                        let spare = buf_slice.get_mut(..data.len());
                        if let Some(spare) = spare {
                            for (dst, &src) in spare.iter_mut().zip(data.iter()) {
                                dst.write(src as f32 / 32768.0);
                            }
                            unsafe { buf.set_len(start + data.len()) };
                        } else {
                            for &s in data {
                                buf.push(s as f32 / 32768.0);
                            }
                        }
                        if rms_slice(&buf[start..]) > threshold {
                            speech_started_c.store(true, Ordering::Relaxed);
                            last_speech_idx_c.store((start + data.len()) as u64, Ordering::Relaxed);
                        }
                    }
                },
                error_fn,
                None,
            )?
        }
        _ => return Err(RecorderError::UnsupportedFormat),
    };

    stream.play()?;

    let start = std::time::Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(100));

        if start.elapsed() >= config.max_duration {
            stopped.store(true, Ordering::Relaxed);
            break;
        }

        if speech_started.load(Ordering::Relaxed) {
            let total = samples.lock().unwrap().len() as u64;
            let last = last_speech_idx.load(Ordering::Relaxed);
            let silent = total.saturating_sub(last);
            let silent_secs = silent as f64 / device_sample_rate as f64;
            if silent_secs >= config.silence_duration.as_secs_f64() {
                stopped.store(true, Ordering::Relaxed);
                break;
            }
        }
    }

    drop(stream);

    let mut raw = samples.lock().unwrap().clone();

    let last = last_speech_idx.load(Ordering::Relaxed) as usize;
    if last > 0 && last < raw.len() {
        raw.truncate(last);
    }

    if channels > 1 {
        raw = mix_to_mono(&raw, channels);
    }

    if device_sample_rate != 16000 {
        raw = resample(&raw, device_sample_rate, 16000);
    }

    Ok(raw)
}

fn rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = data.iter().map(|&x| x * x).sum();
    (sum_sq / data.len() as f32).sqrt()
}

fn rms_slice(data: &[f32]) -> f32 {
    rms(data)
}

fn mix_to_mono(input: &[f32], channels: usize) -> Vec<f32> {
    let len = input.len() / channels;
    let mut mono = Vec::with_capacity(len);
    for i in 0..len {
        let mut sum = 0.0f32;
        for c in 0..channels {
            sum += input[i * channels + c];
        }
        mono.push(sum / channels as f32);
    }
    mono
}

fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = to_rate as f64 / from_rate as f64;
    let output_len = ((input.len() as f64) * ratio).ceil() as usize;
    let mut output = Vec::with_capacity(output_len);
    for i in 0..output_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = src - idx as f64;
        let s0 = input.get(idx).copied().unwrap_or(0.0);
        let s1 = input.get(idx + 1).copied().unwrap_or(s0);
        output.push((s0 as f64 + (s1 as f64 - s0 as f64) * frac) as f32);
    }
    output
}
