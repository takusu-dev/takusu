use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use takusu_audio::{
    FunASRClient, FunASRConfig, FunASRMode, RecordConfig, TtsBackend, TtsClient, TtsConfig,
    TtsOptions, TtsRequest, default_hotwords, pick_reference_voice, record,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SttBackend {
    Funasr,
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

        /// STT backend (funasr)
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
    },

    /// Record from microphone and transcribe (Press Enter to stop)
    Listen {
        /// Output WAV file (saved even after transcription)
        #[arg(short, long, default_value = "record.wav")]
        output: PathBuf,

        /// Maximum recording duration in seconds
        #[arg(long, default_value_t = 120.0)]
        max_duration: f64,

        /// STT backend (funasr)
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

        /// Response audio format: wav, mp3, flac, pcm, opus
        #[arg(long, default_value = "wav")]
        format: String,

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
            funasr_url,
            hotwords,
            funasr_mode,
            ..
        } => {
            let samples = read_wav(&audio);
            eprintln!("Loaded {} samples from {}", samples.len(), audio.display());

            let text = transcribe_funasr(
                &samples,
                None,
                &funasr_url,
                hotwords.as_deref(),
                &funasr_mode,
            )
            .await;
            println!("{text}");
        }

        Commands::Listen {
            output,
            max_duration,
            funasr_url,
            hotwords,
            funasr_mode,
            ..
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

            let text = transcribe_funasr(
                &samples,
                None,
                &funasr_url,
                hotwords.as_deref(),
                &funasr_mode,
            )
            .await;
            println!("{text}");
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
            let max_val = 2u32.pow(spec.bits_per_sample as u32 - 1) as f32;
            reader
                .samples::<i16>()
                .map(|s| s.unwrap() as f32 / max_val)
                .collect()
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
