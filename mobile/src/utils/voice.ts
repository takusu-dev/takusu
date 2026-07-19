import { Platform, PermissionsAndroid } from 'react-native';
import { loadSettings, loadAgentApiKey } from '@/src/api/settingsStore';
import TakusuAudioModule from '../../modules/takusu-server/src/TakusuAudioModule';

let configurePromise: Promise<void> | null = null;
let lastConfigKey = '';
let isRecording = false;
let stopPromise: Promise<string> | null = null;

async function doConfigure(): Promise<void> {
  const settings = await loadSettings();
  const provider = settings.ttsProviders.find(
    (p) => p.id === settings.activeTtsProvider,
  );
  if (!provider) {
    throw new Error('TTS provider is not configured');
  }
  const apiKey = await loadAgentApiKey('tts', provider.id);
  // Intentionally exclude the raw API key from the cache key.
  const configKey = `${provider.id}:${provider.voiceId}:${provider.language}:${provider.sampleRate}`;
  if (configKey === lastConfigKey) return;
  await TakusuAudioModule.configure({
    modelDir: '',
    apiKey,
    voiceId: provider.voiceId,
    language: provider.language,
    sampleRate: provider.sampleRate,
  });
  lastConfigKey = configKey;
}

export async function ensureAudioConfigured(): Promise<void> {
  if (!configurePromise) {
    configurePromise = doConfigure().finally(() => {
      // Allow retry on next call if configuration failed.
      configurePromise = null;
    });
  }
  return configurePromise;
}

export function isRecordingActive(): boolean {
  return isRecording;
}

export async function startRecording(): Promise<void> {
  if (isRecording) {
    throw new Error('既に録音中です');
  }
  isRecording = true;
  try {
    if (Platform.OS === 'android') {
      const permission = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
      );
      if (permission !== PermissionsAndroid.RESULTS.GRANTED) {
        isRecording = false;
        throw new Error('マイク権限が許可されていません');
      }
    }
    // Another stopAndTranscribe may have cancelled this start while permission was pending.
    if (!isRecording) {
      throw new Error('録音がキャンセルされました');
    }
    TakusuAudioModule.startRecording();
  } catch (e) {
    isRecording = false;
    throw e;
  }
}

export async function stopAndTranscribe(): Promise<string> {
  if (stopPromise) return stopPromise;
  if (!isRecording) return '';
  stopPromise = (async () => {
    try {
      await ensureAudioConfigured();
      const transcript = await TakusuAudioModule.stopAndTranscribe();
      return transcript.trim();
    } finally {
      isRecording = false;
      stopPromise = null;
    }
  })();
  return stopPromise;
}

export interface VoiceResult {
  transcript: string;
  sendNow: boolean;
}

class VoiceBridge {
  private current: VoiceResult | null = null;
  private listeners = new Set<(result: VoiceResult | null) => void>();

  setResult(result: VoiceResult): void {
    this.current = result;
    this.listeners.forEach((l) => l(result));
  }

  consume(): VoiceResult | null {
    const r = this.current;
    this.current = null;
    return r;
  }

  subscribe(listener: (result: VoiceResult | null) => void): () => void {
    this.listeners.add(listener);
    if (this.current) listener(this.current);
    return () => this.listeners.delete(listener);
  }
}

export const voiceBridge = new VoiceBridge();
