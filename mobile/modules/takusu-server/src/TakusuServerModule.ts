import { NativeModule, requireNativeModule } from 'expo';

export interface StartOptions {
  port: number;
  workersUrl: string;
  rootToken: string;
}

export interface ServerStatusResult {
  running: boolean;
  port: number;
}

interface TakusuServerModuleType extends NativeModule {
  start(options: StartOptions): Promise<boolean>;
  stop(): Promise<boolean>;
  status(): Promise<ServerStatusResult>;
}

const TakusuServerModule = requireNativeModule<TakusuServerModuleType>(
  'TakusuServer',
);

export default TakusuServerModule;
