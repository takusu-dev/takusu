use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
#[cfg(feature = "hush")]
use takusu_audio::hush::Hush;
use takusu_audio::{
    FunASRClient, FunASRConfig, FunASRMode, RecordConfig, TtsBackend, TtsClient, TtsConfig,
    TtsOptions, TtsRequest, default_hotwords, pick_reference_voice, record,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SttBackend {
    Funasr,
    Sherpa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SherpaModel {
    SenseVoice,
    FunasrNano,
}

#[derive(Parser)]
#[command(
    name = "takusu-audio",
    version,
    about = "Audio recording and speech-to-text CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Record audio from the microphone (Press Enter to stop)
    Record {
        /// Output WAV file
        #[arg(short, long, default_value = "record.wav")]
        output: PathBuf,

        /// Maximum recording duration in seconds
        #[arg(long, default_value_t = 300.0)]
        max_duration: f64,
    },

    /// Transcribe a WAV audio file
    Transcribe {
        /// Path to WAV audio file
        audio: PathBuf,

        /// STT backend (funasr, sherpa)
        #[arg(long, default_value = "funasr")]
        backend: SttBackend,

        /// FunASR server URL
        #[arg(long, default_value = "ws://127.0.0.1:10095")]
        funasr_url: String,

        /// Comma-separated hotwords for FunASR
        #[arg(long)]
        hotwords: Option<String>,

        /// FunASR mode: offline or 2pass
        #[arg(long, default_value = "offline")]
        funasr_mode: String,

        /// Path to Sherpa-ONNX model directory (omit to download SenseVoice on first run)
        #[arg(long)]
        sherpa_model_dir: Option<PathBuf>,

        /// Sherpa-ONNX model family (sense-voice or funasr-nano)
        #[arg(long, value_enum, default_value = "sense-voice")]
        sherpa_model: SherpaModel,

        /// SenseVoice language (auto, zh, en, ja, ko)
        #[arg(long, default_value = "auto")]
        sherpa_language: String,

        /// Use Sherpa-ONNX SenseVoice ITN
        #[arg(long, action = clap::ArgAction::Set, default_value = "true")]
        sherpa_use_itn: bool,

        /// Number of threads for Sherpa-ONNX inference
        #[arg(long, default_value_t = 2)]
        sherpa_num_threads: i32,

        /// ONNX provider for Sherpa-ONNX (cpu, cuda, etc.)
        #[arg(long, default_value = "cpu")]
        sherpa_provider: String,
    },

    /// Record from microphone and transcribe (Press Enter to stop)
    Listen {
        /// Output WAV file (saved even after transcription)
        #[arg(short, long, default_value = "record.wav")]
        output: PathBuf,

        /// Maximum recording duration in seconds
        #[arg(long, default_value_t = 120.0)]
        max_duration: f64,

        /// STT backend (funasr, sherpa)
        #[arg(long, default_value = "funasr")]
        backend: SttBackend,

        /// FunASR server URL
        #[arg(long, default_value = "ws://127.0.0.1:10095")]
        funasr_url: String,

        /// Comma-separated hotwords for FunASR
        #[arg(long)]
        hotwords: Option<String>,

        /// FunASR mode: offline or 2pass
        #[arg(long, default_value = "offline")]
        funasr_mode: String,

        /// Path to Sherpa-ONNX model directory (omit to download SenseVoice on first run)
        #[arg(long)]
        sherpa_model_dir: Option<PathBuf>,

        /// Sherpa-ONNX model family (sense-voice or funasr-nano)
        #[arg(long, value_enum, default_value = "sense-voice")]
        sherpa_model: SherpaModel,

        /// SenseVoice language (auto, zh, en, ja, ko)
        #[arg(long, default_value = "auto")]
        sherpa_language: String,

        /// Use Sherpa-ONNX SenseVoice ITN
        #[arg(long, action = clap::ArgAction::Set, default_value = "true")]
        sherpa_use_itn: bool,

        /// Number of threads for Sherpa-ONNX inference
        #[arg(long, default_value_t = 2)]
        sherpa_num_threads: i32,

        /// ONNX provider for Sherpa-ONNX (cpu, cuda, etc.)
        #[arg(long, default_value = "cpu")]
        sherpa_provider: String,
    },

    #[cfg(feature = "hush")]
    /// Enhance a WAV file with the Hush denoiser
    Hush {
        /// Path to Hush ONNX model directory (omit to download on first run)
        #[arg(long)]
        model_dir: Option<PathBuf>,

        /// Input WAV file
        input: PathBuf,

        /// Output WAV file
        #[arg(short, long, default_value = "enhanced.wav")]
        output: PathBuf,

        /// Target RMS for input normalization (0 disables normalization)
        #[arg(long, default_value = "0.1")]
        target_rms: f32,

        /// Do not restore the original loudness after denoising
        #[arg(long, default_value_t = false)]
        no_restore: bool,
    },

    /// Synthesize speech from text (Irodori-TTS)
    Speak {
        /// Text to synthesize
        #[arg(short, long)]
        text: String,

        /// Irodori-TTS server URL
        #[arg(short, long)]
        url: Option<String>,

        /// Output audio file
        #[arg(short, long, default_value = "speech.wav")]
        output: PathBuf,

        /// Directory containing reference audio files
        #[arg(long, default_value = "./refs")]
        refs_dir: PathBuf,

        /// Reference audio file (overrides refs_dir auto-pick)
        #[arg(long)]
        reference: Option<PathBuf>,

        /// Voice ID (default: first refs file stem)
        #[arg(long)]
        voice: Option<String>,

        /// Response audio format: wav, mp3, flac, pcm, opus.
        /// If omitted, inferred from the --output extension.
        #[arg(long)]
        format: Option<String>,

        /// Speaking speed
        #[arg(long)]
        speed: Option<f32>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record {
            output,
            max_duration,
        } => {
            let config = RecordConfig {
                max_duration: Duration::from_secs_f64(max_duration),
            };

            let samples = match record(&config) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Recording error: {e}");
                    std::process::exit(1);
                }
            };

            eprintln!(
                "Recorded {} samples ({:.1}s)",
                samples.len(),
                samples.len() as f64 / 16000.0
            );
            write_wav(&output, &samples, 16000);
            eprintln!("Saved to {}", output.display());
        }

        Commands::Transcribe {
            audio,
            backend,
            funasr_url,
            hotwords,
            funasr_mode,
            sherpa_model_dir,
            sherpa_model,
            sherpa_language,
            sherpa_use_itn,
            sherpa_num_threads,
            sherpa_provider,
        } => {
            let samples = read_wav(&audio);
            eprintln!("Loaded {} samples from {}", samples.len(), audio.display());

            let text = match backend {
                SttBackend::Funasr => {
                    transcribe_funasr(
                        &samples,
                        None,
                        &funasr_url,
                        hotwords.as_deref(),
                        &funasr_mode,
                    )
                    .await
                }
                SttBackend::Sherpa => {
                    transcribe_sherpa(
                        &samples,
                        sherpa_model_dir,
                        sherpa_model,
                        sherpa_language,
                        sherpa_use_itn,
                        sherpa_num_threads,
                        sherpa_provider,
                    )
                    .await
                }
            };
            println!("{text}");
        }

        Commands::Listen {
            output,
            max_duration,
            backend,
            funasr_url,
            hotwords,
            funasr_mode,
            sherpa_model_dir,
            sherpa_model,
            sherpa_language,
            sherpa_use_itn,
            sherpa_num_threads,
            sherpa_provider,
        } => {
            let config = RecordConfig {
                max_duration: Duration::from_secs_f64(max_duration),
            };

            let samples = match record(&config) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Recording error: {e}");
                    std::process::exit(1);
                }
            };

            if samples.is_empty() {
                eprintln!("No audio recorded.");
                std::process::exit(1);
            }

            eprintln!(
                "Recorded {} samples ({:.1}s)",
                samples.len(),
                samples.len() as f64 / 16000.0
            );
            write_wav(&output, &samples, 16000);
            eprintln!("Saved to {}", output.display());

            let text = match backend {
                SttBackend::Funasr => {
                    transcribe_funasr(
                        &samples,
                        None,
                        &funasr_url,
                        hotwords.as_deref(),
                        &funasr_mode,
                    )
                    .await
                }
                SttBackend::Sherpa => {
                    transcribe_sherpa(
                        &samples,
                        sherpa_model_dir,
                        sherpa_model,
                        sherpa_language,
                        sherpa_use_itn,
                        sherpa_num_threads,
                        sherpa_provider,
                    )
                    .await
                }
            };
            println!("{text}");
        }

        #[cfg(feature = "hush")]
        Commands::Hush {
            model_dir,
            input,
            output,
            target_rms,
            no_restore,
        } => {
            let samples = read_wav(&input);
            eprintln!("Loaded {} samples from {}", samples.len(), input.display());

            let model_dir = match model_dir {
                Some(path) => path,
                None => {
                    eprintln!("Downloading Hush model on first run...");
                    let path = tokio::task::spawn_blocking(|| {
                        let cache = takusu_audio::ModelCache::default_dir().map_err(|e| e.to_string())?;
                        cache.ensure("hush").map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap_or_else(|e| {
                        eprintln!("Model download error: {e}");
                        std::process::exit(1);
                    })
                    .unwrap_or_else(|e| {
                        eprintln!("Model cache error: {e}");
                        std::process::exit(1);
                    });
                    eprintln!("Hush model cached at {}", path.display());
                    path
                }
            };

            let mut hush = Hush::from_model_dir(&model_dir).unwrap_or_else(|e| {
                eprintln!("Hush model error: {e}");
                std::process::exit(1);
            });
            let target = if target_rms > 0.0 { Some(target_rms) } else { None };
            hush.set_target_rms(target);
            hush.set_restore_loudness(!no_restore);

            let start = std::time::Instant::now();
            let enhanced = hush.enhance(&samples).unwrap_or_else(|e| {
                eprintln!("Hush enhancement error: {e}");
                std::process::exit(1);
            });
            eprintln!("Done in {:.1}s.", start.elapsed().as_secs_f64());
            write_wav(&output, &enhanced, 16000);
            eprintln!("Saved to {}", output.display());
        }

        Commands::Speak {
            text,
            url,
            output,
            refs_dir,
            reference,
            voice,
            format,
            speed,
        } => {
            let url = url.unwrap_or_else(|| "http://127.0.0.1:8088".to_string());

            // Resolve the response format, ensuring it matches the --output
            // extension so the file isn't silently corrupted (e.g. MP3 bytes
            // written to a .wav file). If --format is omitted, infer it from
            // the extension; if both are given and conflict, error out.
            let format = resolve_audio_format(format.as_deref(), &output);

            let reference_path = reference.or_else(|| {
                pick_reference_voice(&refs_dir)
                    .ok()
                    .flatten()
                    .map(|(path, _)| path)
            });

            if let Some(path) = &reference_path {
                eprintln!("Using reference audio: {}", path.display());
            }

            let resolved_voice = voice.or_else(|| {
                reference_path
                    .as_ref()
                    .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().to_string()))
            });

            let config = TtsConfig {
                backend: TtsBackend::Irodori,
                url,
                api_key: None,
            };
            let client = TtsClient::new(config);

            let request = TtsRequest {
                text,
                voice: resolved_voice,
                reference_audio_path: reference_path,
                options: TtsOptions {
                    response_format: Some(format),
                    speed,
                },
            };

            eprintln!("Synthesizing with Irodori-TTS...");
            let start = std::time::Instant::now();
            let audio = client.synthesize(&request).await.unwrap_or_else(|e| {
                eprintln!("TTS error: {e}");
                std::process::exit(1);
            });
            eprintln!("Done in {:.1}s.", start.elapsed().as_secs_f64());

            std::fs::write(&output, &audio).unwrap_or_else(|e| {
                eprintln!("Failed to write {output}: {e}", output = output.display());
                std::process::exit(1);
            });
            eprintln!("Saved to {}", output.display());
        }
    }
}

/// Resolve the TTS response format from `--format` and the `--output` path.
///
/// If `--format` is given, it is used as-is when the output file has no
/// extension. When the output file *does* have an extension, `--format` must
/// match it (case-insensitive); a mismatch is an error to prevent writing
/// e.g. MP3 bytes into a `.wav` file. If `--format` is omitted, the format is
/// inferred from the `--output` extension, defaulting to `wav` when the file
/// has no extension.
fn resolve_audio_format(format: Option<&str>, output: &std::path::Path) -> String {
    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match format {
        Some(f) => {
            let f = f.to_ascii_lowercase();
            if let Some(ref e) = ext
                && f != *e
            {
                eprintln!(
                    "Error: --format {f} does not match --output extension '{e}' ({}). \
                     Use a matching extension or omit --format so it is inferred.",
                    output.display()
                );
                std::process::exit(1);
            }
            f
        }
        None => ext.unwrap_or_else(|| "wav".to_string()),
    }
}

async fn transcribe_sherpa(
    samples: &[f32],
    sherpa_model_dir: Option<PathBuf>,
    sherpa_model: SherpaModel,
    sherpa_language: String,
    sherpa_use_itn: bool,
    sherpa_num_threads: i32,
    sherpa_provider: String,
) -> String {
    #[cfg(not(feature = "sherpa"))]
    {
        let _ = (
            samples,
            sherpa_model_dir,
            sherpa_model,
            sherpa_language,
            sherpa_use_itn,
            sherpa_num_threads,
            sherpa_provider,
        );
        eprintln!("Sherpa-ONNX backend requires the 'sherpa' feature at compile time");
        std::process::exit(1);
    }

    #[cfg(feature = "sherpa")]
    {
        use takusu_audio::{ModelCache, SherpaOnnxAsr, SherpaOnnxAsrConfig, SherpaOnnxModel, SpeechToText};

        let model = match sherpa_model {
            SherpaModel::SenseVoice => SherpaOnnxModel::SenseVoice,
            SherpaModel::FunasrNano => SherpaOnnxModel::FunasrNano,
        };

        let model_dir = match sherpa_model_dir {
            Some(path) => path,
            None => {
                if matches!(model, SherpaOnnxModel::FunasrNano) {
                    eprintln!("--sherpa-model-dir is required for funasr-nano");
                    std::process::exit(1);
                }
                eprintln!("Downloading Sherpa-ONNX SenseVoice model on first run...");
                let path = tokio::task::spawn_blocking(|| {
                    let cache = ModelCache::default_dir().map_err(|e| e.to_string())?;
                    cache
                        .ensure("sherpa-sense-voice-int8")
                        .map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Model download join error: {e}");
                    std::process::exit(1);
                })
                .unwrap_or_else(|e| {
                    eprintln!("Model download error: {e}");
                    std::process::exit(1);
                });
                eprintln!("Sherpa-ONNX model cached at {}", path.display());
                path
            }
        };

        let config = SherpaOnnxAsrConfig {
            model_dir,
            model,
            tokens: None,
            num_threads: sherpa_num_threads,
            provider: sherpa_provider,
            sample_rate: 16000,
            language: Some(sherpa_language),
            use_itn: sherpa_use_itn,
        };

        eprintln!("Loading Sherpa-ONNX model from {}...", config.model_dir.display());
        let start = std::time::Instant::now();
        let asr = SherpaOnnxAsr::from_config(&config).unwrap_or_else(|e| {
            eprintln!("Sherpa-ONNX model error: {e}");
            std::process::exit(1);
        });
        eprintln!("Model loaded in {:.1}s.", start.elapsed().as_secs_f64());

        eprintln!("Transcribing ({} samples, {:.1}s) with Sherpa-ONNX...", samples.len(), samples.len() as f64 / 16000.0);
        let start = std::time::Instant::now();
        let text = asr.transcribe(samples).await.unwrap_or_else(|e| {
            eprintln!("Sherpa-ONNX transcription error: {e}");
            std::process::exit(1);
        });
        eprintln!("Done in {:.1}s.", start.elapsed().as_secs_f64());
        text
    }
}

async fn transcribe_funasr(
    samples: &[f32],
    language: Option<&str>,
    url: &str,
    hotwords: Option<&str>,
    mode: &str,
) -> String {
    let hw = match hotwords {
        Some(h) => h.split(',').map(|s| s.trim().to_string()).collect(),
        None => default_hotwords()
            .get(language.unwrap_or("ja"))
            .cloned()
            .unwrap_or_default(),
    };

    let funasr_mode = match mode {
        "2pass" => FunASRMode::TwoPass,
        _ => FunASRMode::Offline,
    };

    let config = FunASRConfig {
        url: url.to_string(),
        language: language.unwrap_or("ja").to_string(),
        hotwords: hw,
        mode: funasr_mode,
    };

    let client = FunASRClient::new(config);

    eprintln!(
        "Transcribing ({} samples, {:.1}s) with FunASR ({mode})...",
        samples.len(),
        samples.len() as f64 / 16000.0
    );
    let start = std::time::Instant::now();
    let text = client.transcribe(samples).await.unwrap_or_else(|e| {
        eprintln!("FunASR error: {e}");
        std::process::exit(1);
    });
    eprintln!("Done in {:.1}s.", start.elapsed().as_secs_f64());
    text
}

fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|e| {
        eprintln!("Failed to create WAV file: {e}");
        std::process::exit(1);
    });

    let max = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let scale = if max > 1.0 { 32767.0 / max } else { 32767.0 };

    for &s in samples {
        let clamped = (s * scale).clamp(-32768.0, 32767.0);
        writer.write_sample(clamped as i16).unwrap_or_else(|e| {
            eprintln!("Failed to write sample: {e}");
            std::process::exit(1);
        });
    }

    writer.finalize().unwrap_or_else(|e| {
        eprintln!("Failed to finalize WAV: {e}");
        std::process::exit(1);
    });
}

fn read_wav(path: &std::path::Path) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| {
        eprintln!("Failed to open WAV file: {e}");
        std::process::exit(1);
    });

    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            if bits == 0 || bits > 32 {
                eprintln!("Unsupported WAV bit depth: {bits}");
                std::process::exit(1);
            }
            // Use the matching hound sample type per bit depth. hound decodes
            // 8/16-bit integer WAVs as i16 and 24/32-bit as i32, so decoding
            // with the wrong type produces garbage or errors. Compute the
            // normalization divisor via u64 to avoid u32 overflow for >16-bit.
            if bits <= 16 {
                let max_val = (1u32 << (bits - 1)) as f32;
                reader
                    .samples::<i16>()
                    .map(|s| s.unwrap() as f32 / max_val)
                    .collect()
            } else {
                let max_val = (1u64 << (bits - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap() as f32 / max_val)
                    .collect()
            }
        }
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
    };

    if spec.sample_rate != 16000 {
        let ratio = 16000.0 / spec.sample_rate as f64;
        let output_len = ((samples.len() as f64) * ratio).ceil() as usize;
        let mut resampled = Vec::with_capacity(output_len);
        for i in 0..output_len {
            let src = i as f64 / ratio;
            let idx = src.floor() as usize;
            let frac = src - idx as f64;
            let s0 = samples.get(idx).copied().unwrap_or(0.0);
            let s1 = samples.get(idx + 1).copied().unwrap_or(s0);
            resampled.push((s0 as f64 + (s1 as f64 - s0 as f64) * frac) as f32);
        }
        return resampled;
    }

    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &std::path::Path, bits: u16, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: bits,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        let max_val = (1u64 << (bits - 1)) as f32;
        for &s in samples {
            let scaled = (s * max_val) as i32;
            if bits <= 16 {
                writer.write_sample(scaled as i16).unwrap();
            } else {
                writer.write_sample(scaled).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn read_wav_16bit_normalizes_correctly() {
        let dir = std::env::temp_dir();
        let path = dir.join("takusu-read-wav-16.wav");
        // Avoid full-scale 1.0 which overflows i16 on write.
        write_wav(&path, 16, &[0.0, 0.5, -0.5, 0.9]);
        let out = read_wav(&path);
        assert_eq!(out.len(), 4);
        assert!((out[0]).abs() < 1e-4);
        assert!((out[1] - 0.5).abs() < 1e-3);
        assert!((out[2] + 0.5).abs() < 1e-3);
        assert!((out[3] - 0.9).abs() < 1e-3);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn read_wav_32bit_normalizes_correctly() {
        let dir = std::env::temp_dir();
        let path = dir.join("takusu-read-wav-32.wav");
        write_wav(&path, 32, &[0.0, 0.25, -0.25, 0.9]);
        let out = read_wav(&path);
        assert_eq!(out.len(), 4);
        assert!((out[0]).abs() < 1e-5);
        assert!((out[1] - 0.25).abs() < 1e-4);
        assert!((out[2] + 0.25).abs() < 1e-4);
        assert!((out[3] - 0.9).abs() < 1e-4);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn read_wav_8bit_normalizes_correctly() {
        // hound sign-extends (not left-shifts) 8-bit samples into i16, so the
        // 2^(bits-1)=128 divisor is correct. This test documents that.
        let dir = std::env::temp_dir();
        let path = dir.join("takusu-read-wav-8.wav");
        write_wav(&path, 8, &[0.0, 0.5, -0.5, 0.9]);
        let out = read_wav(&path);
        assert_eq!(out.len(), 4);
        assert!((out[0]).abs() < 1e-2);
        assert!((out[1] - 0.5).abs() < 2e-2);
        assert!((out[2] + 0.5).abs() < 2e-2);
        assert!((out[3] - 0.9).abs() < 2e-2);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn read_wav_24bit_normalizes_correctly() {
        // hound sign-extends 24-bit samples into i32, so 2^(bits-1) is correct.
        let dir = std::env::temp_dir();
        let path = dir.join("takusu-read-wav-24.wav");
        write_wav(&path, 24, &[0.0, 0.25, -0.25, 0.9]);
        let out = read_wav(&path);
        assert_eq!(out.len(), 4);
        assert!((out[0]).abs() < 1e-5);
        assert!((out[1] - 0.25).abs() < 1e-4);
        assert!((out[2] + 0.25).abs() < 1e-4);
        assert!((out[3] - 0.9).abs() < 1e-4);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn resolve_format_infers_from_extension() {
        assert_eq!(
            resolve_audio_format(None, std::path::Path::new("a.wav")),
            "wav"
        );
        assert_eq!(
            resolve_audio_format(None, std::path::Path::new("a.MP3")),
            "mp3"
        );
        assert_eq!(
            resolve_audio_format(None, std::path::Path::new("a.flac")),
            "flac"
        );
        assert_eq!(
            resolve_audio_format(None, std::path::Path::new("a.opus")),
            "opus"
        );
    }

    #[test]
    fn resolve_format_accepts_matching_explicit() {
        assert_eq!(
            resolve_audio_format(Some("wav"), std::path::Path::new("a.wav")),
            "wav"
        );
        assert_eq!(
            resolve_audio_format(Some("MP3"), std::path::Path::new("a.mp3")),
            "mp3"
        );
    }

    #[test]
    fn resolve_format_defaults_to_wav_for_no_extension() {
        assert_eq!(
            resolve_audio_format(None, std::path::Path::new("speech")),
            "wav"
        );
    }

    #[test]
    fn resolve_format_explicit_with_no_extension() {
        // --format mp3 --output myfile  →  "mp3" (no mismatch error)
        assert_eq!(
            resolve_audio_format(Some("mp3"), std::path::Path::new("myfile")),
            "mp3"
        );
        assert_eq!(
            resolve_audio_format(Some("flac"), std::path::Path::new("myfile")),
            "flac"
        );
    }
}
