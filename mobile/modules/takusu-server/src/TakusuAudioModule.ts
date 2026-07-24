import { NativeModule, requireNativeModule } from 'expo';

export interface AudioOptions {
  provider: string;
  modelDir: string;
  apiKey: string;
  voiceId: string;
  language: string;
  sampleRate: number;
  speed: number;
  mute?: boolean;
}

export interface TtsVoiceInfo {
  name: string;
  locale: string;
  quality: number;
  latency: number;
  requiresNetworkConnection: boolean;
  features: string[];
}

interface TakusuAudioModuleType extends NativeModule {
  configure(options: AudioOptions): Promise<boolean>;
  setMuted(muted: boolean): Promise<boolean>;
  startRecording(): boolean;
  stopAndTranscribe(): Promise<string>;
  synthesizeAndPlay(text: string): Promise<boolean>;
  stopPlayback(): boolean;
  getAvailableVoices(): Promise<TtsVoiceInfo[]>;
}

const TakusuAudioModule =
  requireNativeModule<TakusuAudioModuleType>('TakusuAudio');

export default TakusuAudioModule;
