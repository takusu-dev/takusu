import { NativeModule, requireNativeModule } from 'expo';

export interface StartOptions {
  port: number;
  workersUrl: string;
  rootToken: string;
  agentConfigJson?: string;
}

export interface ServerStatusResult {
  running: boolean;
  port: number;
}

interface TakusuServerModuleType extends NativeModule {
  // The Kotlin module registers these as synchronous `Function`s (not
  // `AsyncFunction`), so they return their values directly rather than
  // Promise-wrapped values. `await` on a non-Promise still works, but
  // `.catch()` / `.then()` on the raw return value does not — callers
  // that need Promise chaining must wrap with `Promise.resolve(...)`.
  start(options: StartOptions): boolean;
  stop(): boolean;
  status(): ServerStatusResult;
  getLogs(): string[];
  clearLogs(): boolean;
  pushLog(line: string): boolean;
  startModelDownload(modelId: string): boolean;
}

const TakusuServerModule =
  requireNativeModule<TakusuServerModuleType>('TakusuServer');

export default TakusuServerModule;
