//! Audio playback for WAV buffers.
//!
//! Parses WAV data, validates the header and sample format, then plays the audio
//! through the default output device using cpal.

use std::io::Cursor;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlayError {
    #[error("no output device")]
    NoOutputDevice,
    #[error("wav parse error: {0}")]
    WavParse(String),
    #[error("unsupported wav format: {0}")]
    UnsupportedFormat(String),
    #[error("cpal error: {0}")]
    Cpal(String),
}

impl From<cpal::Error> for PlayError {
    fn from(e: cpal::Error) -> Self {
        Self::Cpal(e.to_string())
    }
}

impl From<hound::Error> for PlayError {
    fn from(e: hound::Error) -> Self {
        Self::WavParse(e.to_string())
    }
}

/// A clip parsed from a WAV buffer, ready for playback.
#[derive(Debug, Clone)]
pub struct AudioClip {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
}

impl AudioClip {
    /// Parse a WAV buffer and validate the format.
    ///
    /// Only 16-bit integer PCM, mono or stereo WAVs are supported.
    pub fn from_wav_bytes(bytes: &[u8]) -> Result<Self, PlayError> {
        let mut reader = hound::WavReader::new(Cursor::new(bytes))?;
        let spec = reader.spec();

        if spec.sample_format != hound::SampleFormat::Int {
            return Err(PlayError::UnsupportedFormat(format!(
                "sample_format={:?}",
                spec.sample_format
            )));
        }
        if spec.bits_per_sample != 16 {
            return Err(PlayError::UnsupportedFormat(format!(
                "bits_per_sample={}",
                spec.bits_per_sample
            )));
        }
        if spec.channels == 0 || spec.channels > 2 {
            return Err(PlayError::UnsupportedFormat(format!(
                "channels={}",
                spec.channels
            )));
        }

        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| Ok(s? as f32 / 32768.0))
            .collect::<Result<_, hound::Error>>()?;

        Ok(Self {
            samples,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        })
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}

/// Play a parsed audio clip on the default output device.
pub fn play(clip: &AudioClip) -> Result<(), PlayError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(PlayError::NoOutputDevice)?;
    let supported = device.default_output_config()?;
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.into();
    let output_channels = stream_config.channels as usize;
    let output_rate = stream_config.sample_rate;

    let mut samples = clip.samples.clone();
    if clip.sample_rate != output_rate {
        samples = resample_interleaved(
            &samples,
            clip.sample_rate,
            output_rate,
            clip.channels as usize,
        );
    }
    if clip.channels as usize != output_channels {
        samples = convert_channels(&samples, clip.channels as usize, output_channels);
    }

    let buffer = Arc::new(Mutex::new(samples));
    let pos = Arc::new(AtomicUsize::new(0));
    let len = buffer.lock().unwrap().len();

    let error_fn = |err: cpal::Error| eprintln!("audio playback stream error: {err}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            build_output_stream::<f32>(&device, stream_config, buffer, pos.clone(), error_fn)?
        }
        cpal::SampleFormat::I16 => {
            build_output_stream::<i16>(&device, stream_config, buffer, pos.clone(), error_fn)?
        }
        cpal::SampleFormat::U16 => {
            build_output_stream::<u16>(&device, stream_config, buffer, pos.clone(), error_fn)?
        }
        _ => {
            return Err(PlayError::UnsupportedFormat(format!(
                "output sample format {sample_format:?}"
            )));
        }
    };

    stream.play()?;

    while pos.load(Ordering::Relaxed) < len {
        thread::sleep(Duration::from_millis(10));
    }
    // Give the backend a moment to drain the last frames.
    thread::sleep(Duration::from_millis(100));

    drop(stream);
    Ok(())
}

fn build_output_stream<T: SizedSample + FromSample<f32> + Send + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    pos: Arc<AtomicUsize>,
    error_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, PlayError> {
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut p = pos.load(Ordering::Relaxed);
            let buf = buffer.lock().unwrap();
            for sample in data.iter_mut() {
                if p < buf.len() {
                    *sample = T::from_sample(buf[p]);
                    p += 1;
                } else {
                    *sample = T::from_sample(0.0f32);
                }
            }
            pos.store(p, Ordering::Relaxed);
        },
        error_fn,
        None,
    )?;
    Ok(stream)
}

fn resample_interleaved(input: &[f32], from_rate: u32, to_rate: u32, channels: usize) -> Vec<f32> {
    if input.is_empty() || from_rate == to_rate {
        return input.to_vec();
    }

    if channels == 1 {
        return resample_mono(input, from_rate, to_rate);
    }

    let frame_count = input.len() / channels;
    let mut per_channel: Vec<Vec<f32>> = vec![Vec::with_capacity(frame_count); channels];
    for (i, s) in input.iter().enumerate() {
        per_channel[i % channels].push(*s);
    }

    let mut resampled: Vec<Vec<f32>> = Vec::with_capacity(channels);
    for ch in per_channel {
        resampled.push(resample_mono(&ch, from_rate, to_rate));
    }

    let output_len = resampled[0].len();
    let mut output = Vec::with_capacity(output_len * channels);
    for i in 0..output_len {
        for ch in &resampled {
            output.push(ch[i]);
        }
    }
    output
}

fn resample_mono(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
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

fn convert_channels(input: &[f32], from_channels: usize, to_channels: usize) -> Vec<f32> {
    if from_channels == to_channels {
        return input.to_vec();
    }

    if from_channels == 1 && to_channels == 2 {
        let mut output = Vec::with_capacity(input.len() * 2);
        for s in input {
            output.push(*s);
            output.push(*s);
        }
        return output;
    }

    if from_channels == 2 && to_channels == 1 {
        let frame_count = input.len() / 2;
        let mut output = Vec::with_capacity(frame_count);
        for i in 0..frame_count {
            let l = input[i * 2];
            let r = input[i * 2 + 1];
            output.push((l + r) / 2.0);
        }
        return output;
    }

    input.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wav_16bit_mono() {
        let mut buf = Vec::new();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        for s in [0.0, 0.5, -0.5, 0.9] {
            writer.write_sample((s * 32767.0) as i16).unwrap();
        }
        writer.finalize().unwrap();

        let clip = AudioClip::from_wav_bytes(&buf).unwrap();
        assert_eq!(clip.channels(), 1);
        assert_eq!(clip.sample_rate(), 16000);
        assert_eq!(clip.samples().len(), 4);
    }

    #[test]
    fn reject_8bit_wav() {
        let mut buf = Vec::new();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 8,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        writer.write_sample(0i8).unwrap();
        writer.finalize().unwrap();

        assert!(AudioClip::from_wav_bytes(&buf).is_err());
    }

    #[test]
    fn reject_float_wav() {
        let mut buf = Vec::new();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::new(Cursor::new(&mut buf), spec).unwrap();
        writer.write_sample(0.0f32).unwrap();
        writer.finalize().unwrap();

        assert!(AudioClip::from_wav_bytes(&buf).is_err());
    }

    #[test]
    fn resample_mono_doubles_length() {
        let input = vec![0.0, 1.0, 0.0, -1.0];
        let out = resample_mono(&input, 16000, 32000);
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn convert_channels_mono_to_stereo() {
        let input = vec![0.5, -0.5];
        let out = convert_channels(&input, 1, 2);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn convert_channels_stereo_to_mono() {
        let input = vec![0.5, 0.5, -0.5, -0.5];
        let out = convert_channels(&input, 2, 1);
        assert_eq!(out, vec![0.5, -0.5]);
    }
}
