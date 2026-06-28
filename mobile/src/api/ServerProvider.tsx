import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';
import { Platform } from 'react-native';
import { TakusuClient } from './client';
import TakusuServerModule from '../../modules/takusu-server/src/TakusuServerModule';

interface ServerState {
  ready: boolean;
  error: string | null;
  client: TakusuClient | null;
}

const ServerContext = createContext<ServerState>({
  ready: false,
  error: null,
  client: null,
});

const DEFAULT_PORT = 3838;

export function ServerProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ServerState>({
    ready: false,
    error: null,
    client: null,
  });

  useEffect(() => {
    let cancelled = false;

    async function init() {
      if (Platform.OS !== 'android') {
        // On non-Android platforms, use a configurable base URL
        // (for development with an external server)
        const baseUrl = process.env.EXPO_PUBLIC_TAKUSU_URL ?? 'http://localhost:3000';
        const token = process.env.EXPO_PUBLIC_TAKUSU_TOKEN ?? '';
        if (cancelled) return;
        setState({
          ready: true,
          error: null,
          client: new TakusuClient(baseUrl, token),
        });
        return;
      }

      try {
        const workersUrl = process.env.EXPO_PUBLIC_WORKERS_URL ?? '';
        const rootToken = process.env.EXPO_PUBLIC_ROOT_TOKEN ?? '';

        await TakusuServerModule.start({
          port: DEFAULT_PORT,
          workersUrl,
          rootToken,
        });

        if (cancelled) return;
        setState({
          ready: true,
          error: null,
          client: new TakusuClient(`http://127.0.0.1:${DEFAULT_PORT}`, rootToken),
        });
      } catch (e) {
        if (cancelled) return;
        setState({
          ready: false,
          error: e instanceof Error ? e.message : String(e),
          client: null,
        });
      }
    }

    init();

    return () => {
      cancelled = true;
      if (Platform.OS === 'android') {
        TakusuServerModule.stop().catch(() => {});
      }
    };
  }, []);

  return (
    <ServerContext.Provider value={state}>{children}</ServerContext.Provider>
  );
}

export function useServer() {
  return useContext(ServerContext);
}
