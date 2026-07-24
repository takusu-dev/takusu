// Local takusu server lifecycle helpers.
//
// Used by ServerProvider (foreground) and the notification background task.

import TakusuServerModule, {
  type StartOptions,
} from '@/modules/takusu-server/src/TakusuServerModule';
import { TakusuClient } from '@/src/api/client';

export const DEFAULT_LOCAL_PORT = 3838;

export interface EnsureLocalServerOptions {
  port?: number;
  workersUrl: string;
  rootToken: string;
  agentConfigJson?: string;
}

function isAlreadyRunningError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return (
    message.includes('already running') ||
    message.includes('ERR_ALREADY_RUNNING')
  );
}

// Return the port the local server is currently running on, falling back
// to the default port if the module reports that it is not running.
export function getLocalServerPort(): number {
  try {
    const status = TakusuServerModule.status();
    if (status.running && status.port > 0) {
      return status.port;
    }
  } catch {
    // module may not be available in tests
  }
  return DEFAULT_LOCAL_PORT;
}

// Return a client for the local server, starting it if necessary.
// Throws if the server cannot be started.
export function ensureLocalServer(
  options: EnsureLocalServerOptions,
): TakusuClient {
  const {
    port = DEFAULT_LOCAL_PORT,
    workersUrl,
    rootToken,
    agentConfigJson,
  } = options;

  const status = TakusuServerModule.status();
  if (status.running) {
    return new TakusuClient(`http://127.0.0.1:${status.port}`, rootToken);
  }

  const startOptions: StartOptions = {
    port,
    workersUrl,
    rootToken,
  };
  if (agentConfigJson !== undefined) {
    startOptions.agentConfigJson = agentConfigJson;
  }

  try {
    TakusuServerModule.start(startOptions);
  } catch (err) {
    if (!isAlreadyRunningError(err)) {
      throw err;
    }
  }

  const after = TakusuServerModule.status();
  if (!after.running) {
    throw new Error('Local server did not start');
  }

  return new TakusuClient(`http://127.0.0.1:${after.port}`, rootToken);
}
