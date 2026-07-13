import { NativeModule, requireNativeModule } from 'expo';

export interface AudioOptions {
  modelDir: string;
  apiKey: string;
  voiceId: string;
  language: string;
  sampleRate: number;
}

interface TakusuAudioModuleType extends NativeModule {
  configure(options: AudioOptions): Promise<boolean>;
  startRecording(): boolean;
  stopAndTranscribe(): Promise<string>;
  synthesizeAndPlay(text: string): Promise<boolean>;
  stopPlayback(): boolean;
}

const TakusuAudioModule =
  requireNativeModule<TakusuAudioModuleType>('TakusuAudio');

export default TakusuAudioModule;
