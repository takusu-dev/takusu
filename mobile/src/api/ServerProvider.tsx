import {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  useMemo,
  type ReactNode,
} from 'react';
import { Platform } from 'react-native';
import { TakusuClient } from './client';
import TakusuServerModule from '../../modules/takusu-server/src/TakusuServerModule';
import TakusuWidgetModule from '../../modules/takusu-widget/src/TakusuWidgetModule';
import {
  loadSettings,
  saveWorkersUrl,
  saveWorkersToken,
  saveDarkMode,
  saveUndoSteps,
  saveNotificationSettings,
  type PersistedSettings,
  type NotificationSettings,
} from './settingsStore';
import { undoRedo, DEFAULT_MAX_HISTORY } from './undoRedo';

interface ServerState {
  ready: boolean;
  error: string | null;
  client: TakusuClient | null;
  workersUrl: string;
  workersToken: string;
  darkMode: boolean;
  undoSteps: number;
  notifications: NotificationSettings;
  restarting: boolean;
}

interface ServerContextValue extends ServerState {
  restartServer: (url?: string, token?: string) => Promise<void>;
  setDarkMode: (enabled: boolean) => Promise<void>;
  setUndoSteps: (steps: number) => Promise<void>;
  setNotifications: (settings: NotificationSettings) => Promise<void>;
}

const ServerContext = createContext<ServerContextValue>({
  ready: false,
  error: null,
  client: null,
  workersUrl: '',
  workersToken: '',
  darkMode: false,
  undoSteps: DEFAULT_MAX_HISTORY,
  notifications: {} as NotificationSettings,
  restarting: false,
  restartServer: async () => {},
  setDarkMode: async () => {},
  setUndoSteps: async () => {},
  setNotifications: async () => {},
});

const DEFAULT_PORT = 3838;

export function ServerProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ServerState>({
    ready: false,
    error: null,
    client: null,
    workersUrl: '',
    workersToken: '',
    darkMode: false,
    undoSteps: DEFAULT_MAX_HISTORY,
    notifications: {} as NotificationSettings,
    restarting: false,
  });

  const startServer = useCallback(
    async (url: string, token: string): Promise<TakusuClient | null> => {
      if (Platform.OS !== 'android') {
        const baseUrl =
          process.env.EXPO_PUBLIC_TAKUSU_URL ?? 'http://localhost:3000';
        const tk = process.env.EXPO_PUBLIC_TAKUSU_TOKEN ?? '';
        return new TakusuClient(baseUrl, tk);
      }

      const finalUrl = url || process.env.EXPO_PUBLIC_WORKERS_URL || '';
      const finalToken = token || process.env.EXPO_PUBLIC_ROOT_TOKEN || '';

      if (!finalUrl || !finalToken) {
        throw new Error('Workers URL and token are required');
      }

      await TakusuServerModule.start({
        port: DEFAULT_PORT,
        workersUrl: finalUrl,
        rootToken: finalToken,
      });

      // Persist credentials for the home screen widget so the
      // WorkManager worker can start the local server independently.
      try {
        TakusuWidgetModule.saveConfig({
          workersUrl: finalUrl,
          token: finalToken,
        });
      } catch {
        // widget module not available (e.g. non-Android) — ignore
      }

      return new TakusuClient(`http://127.0.0.1:${DEFAULT_PORT}`, finalToken);
    },
    [],
  );

  const restartServer = useCallback(
    async (url?: string, token?: string) => {
      const newUrl = url ?? state.workersUrl;
      const newToken = token ?? state.workersToken;

      setState((prev) => ({ ...prev, restarting: true }));

      try {
        if (Platform.OS === 'android') {
          try {
            await TakusuServerModule.stop();
          } catch {
            // server may not be running, ignore
          }
        }

        const client = await startServer(newUrl, newToken);

        setState((prev) => ({
          ...prev,
          ready: true,
          error: null,
          client,
          workersUrl: newUrl,
          workersToken: newToken,
          restarting: false,
        }));
      } catch (e) {
        setState((prev) => ({
          ...prev,
          error: e instanceof Error ? e.message : String(e),
          restarting: false,
        }));
      }
    },
    [state.workersUrl, state.workersToken, startServer],
  );

  const setDarkMode = useCallback(async (enabled: boolean) => {
    await saveDarkMode(enabled);
    setState((prev) => ({ ...prev, darkMode: enabled }));
  }, []);

  const setUndoSteps = useCallback(async (steps: number) => {
    if (!Number.isFinite(steps) || steps <= 0) return;
    const n = Math.floor(steps);
    await saveUndoSteps(n);
    undoRedo.setMaxHistory(n);
    setState((prev) => ({ ...prev, undoSteps: n }));
  }, []);

  const setNotifications = useCallback(
    async (settings: NotificationSettings) => {
      await saveNotificationSettings(settings);
      setState((prev) => ({ ...prev, notifications: settings }));
    },
    [],
  );

  useEffect(() => {
    let cancelled = false;

    async function init() {
      const settings: PersistedSettings = await loadSettings();

      if (cancelled) return;

      setState((prev) => ({
        ...prev,
        workersUrl: settings.workersUrl,
        workersToken: settings.workersToken,
        darkMode: settings.darkMode,
        undoSteps: settings.undoSteps,
        notifications: settings.notifications,
      }));

      undoRedo.setMaxHistory(settings.undoSteps);

      try {
        const client = await startServer(
          settings.workersUrl,
          settings.workersToken,
        );
        if (cancelled) return;
        setState((prev) => ({
          ...prev,
          ready: true,
          error: null,
          client,
        }));
      } catch (e) {
        if (cancelled) return;
        setState((prev) => ({
          ...prev,
          ready: false,
          error: e instanceof Error ? e.message : String(e),
          client: null,
        }));
      }
    }

    init();

    return () => {
      cancelled = true;
      if (Platform.OS === 'android') {
        // stop() is a synchronous native Function; a thrown native
        // exception (e.g. "Server not running") propagates synchronously,
        // so use try/catch rather than Promise.resolve().catch().
        try {
          TakusuServerModule.stop();
        } catch {
          // server may not be running
        }
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const contextValue = useMemo<ServerContextValue>(
    () => ({
      ...state,
      restartServer,
      setDarkMode,
      setUndoSteps,
      setNotifications,
    }),
    [state, restartServer, setDarkMode, setUndoSteps, setNotifications],
  );

  return (
    <ServerContext.Provider value={contextValue}>
      {children}
    </ServerContext.Provider>
  );
}

export function useServer() {
  return useContext(ServerContext);
}

export { saveWorkersUrl, saveWorkersToken, saveNotificationSettings };
