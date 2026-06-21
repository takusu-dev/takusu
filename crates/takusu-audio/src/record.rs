use std::io::BufRead;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
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

impl From<cpal::Error> for RecorderError {
    fn from(e: cpal::Error) -> Self {
        Self::Cpal(e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct RecordConfig {
    pub max_duration: Duration,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
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

    let error_fn = |err: cpal::Error| {
        eprintln!("audio stream error: {err}");
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let samples_c = samples.clone();
            let stopped_c = stopped.clone();

            device.build_input_stream(
                stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if stopped_c.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples_c.try_lock() {
                        buf.extend_from_slice(data);
                    }
                },
                error_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let samples_c = samples.clone();
            let stopped_c = stopped.clone();

            device.build_input_stream(
                stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if stopped_c.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples_c.try_lock() {
                        for &s in data {
                            buf.push(s as f32 / 32768.0);
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

    let stopped_t = stopped.clone();
    let _waiter = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        let _ = stdin.lock().read_line(&mut line);
        stopped_t.store(true, Ordering::Relaxed);
    });

    eprintln!("Recording... Press Enter to stop.");

    let start = std::time::Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(100));

        if stopped.load(Ordering::Relaxed) {
            break;
        }
        if start.elapsed() >= config.max_duration {
            stopped.store(true, Ordering::Relaxed);
            break;
        }
    }

    drop(stream);

    let mut raw = samples.lock().unwrap().clone();

    if channels > 1 {
        raw = mix_to_mono(&raw, channels);
    }

    if device_sample_rate != 16000 {
        raw = resample(&raw, device_sample_rate, 16000);
    }

    raw = normalize(&raw, 0.1);

    Ok(raw)
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

fn normalize(input: &[f32], target_rms: f32) -> Vec<f32> {
    if input.is_empty() {
        return input.to_vec();
    }
    let rms = {
        let sum_sq: f32 = input.iter().map(|&x| x * x).sum();
        (sum_sq / input.len() as f32).sqrt()
    };
    if rms < 1e-10 {
        return input.to_vec();
    }
    let scale = target_rms / rms;
    input.iter().map(|&x| x * scale).collect()
}
