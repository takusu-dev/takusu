import {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  useMemo,
  type ReactNode,
} from 'react';
import { Appearance, Platform } from 'react-native';
import Constants from 'expo-constants';
import { TakusuClient } from './client';
import { DEFAULT_LOCAL_PORT, ensureLocalServer } from './server';
import TakusuServerModule from '@/modules/takusu-server/src/TakusuServerModule';
import TakusuWidgetModule from '../../modules/takusu-widget/src/TakusuWidgetModule';
import {
  loadSettings,
  loadAgentApiKey,
  saveWorkersUrl,
  saveWorkersToken,
  saveTheme,
  saveUndoSteps,
  saveNotificationSettings,
  type PersistedSettings,
  type NotificationSettings,
} from './settingsStore';
import { APP_THEMES, type AppTheme } from '@/src/theme';
import { undoRedo, DEFAULT_MAX_HISTORY } from './undoRedo';

interface ServerState {
  ready: boolean;
  error: string | null;
  client: TakusuClient | null;
  workersUrl: string;
  workersToken: string;
  theme: AppTheme;
  undoSteps: number;
  notifications: NotificationSettings;
  restarting: boolean;
}

interface ServerContextValue extends ServerState {
  restartServer: (url?: string, token?: string) => Promise<void>;
  setWorkersUrl: (url: string) => Promise<void>;
  setWorkersToken: (token: string) => Promise<void>;
  setTheme: (theme: AppTheme) => Promise<void>;
  setUndoSteps: (steps: number) => Promise<void>;
  setNotifications: (settings: NotificationSettings) => Promise<void>;
}

const ServerContext = createContext<ServerContextValue>({
  ready: false,
  error: null,
  client: null,
  workersUrl: '',
  workersToken: '',
  theme: 'light',
  undoSteps: DEFAULT_MAX_HISTORY,
  notifications: {} as NotificationSettings,
  restarting: false,
  restartServer: async () => {},
  setWorkersUrl: async () => {},
  setWorkersToken: async () => {},
  setTheme: async () => {},
  setUndoSteps: async () => {},
  setNotifications: async () => {},
});

export const DEFAULT_PORT = DEFAULT_LOCAL_PORT;

function systemInitialTheme(): AppTheme {
  return Appearance.getColorScheme() === 'dark' ? 'dark' : 'light';
}

export function ServerProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ServerState>({
    ready: false,
    error: null,
    client: null,
    workersUrl: '',
    workersToken: '',
    theme: systemInitialTheme(),
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

      const agentSettings = await loadSettings();
      const activeLlm = agentSettings.llmProviders.find(
        (provider) => provider.id === agentSettings.activeLlmProvider,
      );
      const activeTts = agentSettings.ttsProviders.find(
        (provider) => provider.id === agentSettings.activeTtsProvider,
      );
      const [llmKey, ttsKey] = await Promise.all([
        activeLlm ? loadAgentApiKey('llm', activeLlm.id) : Promise.resolve(''),
        activeTts ? loadAgentApiKey('tts', activeTts.id) : Promise.resolve(''),
      ]);
      const agentConfigJson = JSON.stringify({
        llm: activeLlm
          ? {
              base_url: activeLlm.baseUrl,
              model: activeLlm.selectedModel,
              api_key: llmKey,
            }
          : undefined,
        audio: activeTts
          ? {
              tts: {
                backend: activeTts.provider,
                api_key: ttsKey,
                voice_id: activeTts.voiceId,
                language: activeTts.language,
                sample_rate: activeTts.sampleRate,
                speed: activeTts.speed,
              },
            }
          : undefined,
      });

      const client = ensureLocalServer({
        workersUrl: finalUrl,
        rootToken: finalToken,
        agentConfigJson,
      });

      // Persist credentials for the home screen widget so the
      // WorkManager worker can start the local server independently.
      try {
        const scheme = Constants.expoConfig?.scheme;
        TakusuWidgetModule.saveConfig({
          workersUrl: finalUrl,
          token: finalToken,
          scheme: Array.isArray(scheme) ? scheme[0] : scheme,
        });
      } catch {
        // widget module not available (e.g. non-Android) — ignore
      }

      return client;
    },
    [],
  );

  const restartServer = useCallback(
    async (url?: string, token?: string) => {
      const newUrl = url ?? state.workersUrl;
      const newToken = token ?? state.workersToken;

      setState((prev) => ({ ...prev, restarting: true, error: null }));

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

  const setWorkersUrl = useCallback(async (url: string) => {
    await saveWorkersUrl(url);
    setState((prev) => ({ ...prev, workersUrl: url }));
  }, []);

  const setWorkersToken = useCallback(async (token: string) => {
    await saveWorkersToken(token);
    setState((prev) => ({ ...prev, workersToken: token }));
  }, []);

  const setTheme = useCallback(async (newTheme: AppTheme) => {
    if (!APP_THEMES.includes(newTheme)) return;
    await saveTheme(newTheme);
    setState((prev) => ({ ...prev, theme: newTheme }));
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
        theme: settings.theme,
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
      setWorkersUrl,
      setWorkersToken,
      setTheme,
      setUndoSteps,
      setNotifications,
    }),
    [
      state,
      restartServer,
      setWorkersUrl,
      setWorkersToken,
      setTheme,
      setUndoSteps,
      setNotifications,
    ],
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
