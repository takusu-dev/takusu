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
import { AgentClient, type AgentUpdateSettings } from './agentClient';
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
  pushAgentConfig: () => Promise<void>;
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
  pushAgentConfig: async () => {},
});

export const DEFAULT_PORT = DEFAULT_LOCAL_PORT;

async function buildAgentUpdateSettings(): Promise<AgentUpdateSettings> {
  const settings = await loadSettings();
  const activeLlm = settings.llmProviders.find(
    (p) => p.id === settings.activeLlmProvider,
  );
  const activeTts = settings.ttsProviders.find(
    (p) => p.id === settings.activeTtsProvider,
  );
  const [llmKey, ttsKey] = await Promise.all([
    activeLlm ? loadAgentApiKey('llm', activeLlm.id) : Promise.resolve(''),
    activeTts ? loadAgentApiKey('tts', activeTts.id) : Promise.resolve(''),
  ]);
  const body: AgentUpdateSettings = {};
  if (activeLlm) {
    body.llm = {
      base_url: activeLlm.baseUrl,
      model: activeLlm.selectedModel,
    };
    if (llmKey) {
      body.llm.api_key = llmKey;
    }
  }
  if (activeTts) {
    body.audio = {
      tts: {
        backend: activeTts.provider,
        voice_id: activeTts.voiceId,
        language: activeTts.language,
        sample_rate: activeTts.sampleRate,
        speed: activeTts.speed,
      },
    };
    if (ttsKey) {
      body.audio.tts.api_key = ttsKey;
    }
  }
  return body;
}

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

      const agentConfigJson = JSON.stringify(await buildAgentUpdateSettings());

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
    setState((prev) => {
      const client = prev.client
        ? new TakusuClient(prev.client.baseUrl, token)
        : prev.client;
      return { ...prev, workersToken: token, client };
    });
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

  const pushAgentConfig = useCallback(async () => {
    if (Platform.OS !== 'android' || !state.ready || !state.workersToken) {
      return;
    }
    const agentClient = new AgentClient(
      `http://127.0.0.1:${DEFAULT_PORT}`,
      state.workersToken,
    );
    await agentClient.updateSettings(await buildAgentUpdateSettings());
  }, [state.ready, state.workersToken]);

  const contextValue = useMemo<ServerContextValue>(
    () => ({
      ...state,
      restartServer,
      setWorkersUrl,
      setWorkersToken,
      setTheme,
      setUndoSteps,
      setNotifications,
      pushAgentConfig,
    }),
    [
      state,
      restartServer,
      setWorkersUrl,
      setWorkersToken,
      setTheme,
      setUndoSteps,
      setNotifications,
      pushAgentConfig,
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
