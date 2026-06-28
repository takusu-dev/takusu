// Web stub — the embedded server is only available on Android
import type { StartOptions, ServerStatusResult } from './TakusuServerModule';

const stub = {
  start: async (_options: StartOptions): Promise<boolean> => {
    throw new Error('TakusuServer is not available on web');
  },
  stop: async (): Promise<boolean> => {
    throw new Error('TakusuServer is not available on web');
  },
  status: async (): Promise<ServerStatusResult> => {
    return { running: false, port: 0 };
  },
};

export default stub;
