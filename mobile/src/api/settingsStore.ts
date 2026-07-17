// Persistent settings store.
// Sensitive values (Workers URL, token) use expo-secure-store.
// Non-sensitive values (theme, notification settings) use AsyncStorage.

import * as SecureStore from 'expo-secure-store';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { type AppTheme, APP_THEMES } from '@/src/theme';
import {
  type NotificationSettings,
  DEFAULT_NOTIFICATION_SETTINGS,
  loadNotificationSettings,
  saveNotificationSettings,
} from '@/src/notifications/settings';
import { DEFAULT_MAX_HISTORY } from './undoRedo';

export const AGENT_SESSION_HISTORY_DEFAULT = 4;
export const AGENT_SESSION_HISTORY_MIN = 3;
export const AGENT_SESSION_HISTORY_MAX = 5;

const KEYS = {
  workersUrl: 'takusu.workersUrl',
  workersToken: 'takusu.workersToken',
  theme: 'takusu.theme',
  undoSteps: 'takusu.undoSteps',
  agentSessionHistoryCount: 'takusu.agent.sessionHistoryCount',
  llmProviders: 'takusu.agent.llmProviders',
  activeLlmProvider: 'takusu.agent.activeLlmProvider',
  ttsProviders: 'takusu.agent.ttsProviders',
  activeTtsProvider: 'takusu.agent.activeTtsProvider',
} as const;

export interface LlmProviderSettings {
  id: string;
  name: string;
  baseUrl: string;
  selectedModel: string;
  cachedModels: string[];
  modelsFetchedAt?: string;
  cost?: string;
}

export interface TtsProviderSettings {
  id: string;
  name: string;
  provider: 'cartesia';
  voiceId: string;
  model?: string;
  language: string;
  sampleRate: number;
  speed?: number;
}

export interface PersistedSettings {
  workersUrl: string;
  workersToken: string;
  theme: AppTheme;
  undoSteps: number;
  agentSessionHistoryCount: number;
  notifications: NotificationSettings;
  llmProviders: LlmProviderSettings[];
  activeLlmProvider: string | null;
  ttsProviders: TtsProviderSettings[];
  activeTtsProvider: string | null;
}

function isValidTheme(value: string | null): value is AppTheme {
  return value !== null && APP_THEMES.includes(value as AppTheme);
}

const LEGACY_DARK_MODE_KEY = 'takusu.darkMode';

export async function loadSettings(): Promise<PersistedSettings> {
  const [
    workersUrl,
    workersToken,
    themeStr,
    darkModeStr,
    undoStepsStr,
    agentSessionHistoryCountStr,
    notifications,
    llmProvidersStr,
    activeLlmProvider,
    ttsProvidersStr,
    activeTtsProvider,
  ] = await Promise.all([
    SecureStore.getItemAsync(KEYS.workersUrl),
    SecureStore.getItemAsync(KEYS.workersToken),
    AsyncStorage.getItem(KEYS.theme),
    AsyncStorage.getItem(LEGACY_DARK_MODE_KEY),
    AsyncStorage.getItem(KEYS.undoSteps),
    AsyncStorage.getItem(KEYS.agentSessionHistoryCount),
    loadNotificationSettings(),
    AsyncStorage.getItem(KEYS.llmProviders),
    AsyncStorage.getItem(KEYS.activeLlmProvider),
    AsyncStorage.getItem(KEYS.ttsProviders),
    AsyncStorage.getItem(KEYS.activeTtsProvider),
  ]);
  const parsedUndoSteps = undoStepsStr ? parseInt(undoStepsStr, 10) : NaN;
  const parsedSessionCount = agentSessionHistoryCountStr
    ? parseInt(agentSessionHistoryCountStr, 10)
    : NaN;

  let theme: AppTheme;
  if (isValidTheme(themeStr)) {
    theme = themeStr;
  } else if (darkModeStr !== null) {
    // Migrate legacy darkMode boolean to theme string.
    theme = darkModeStr === 'true' ? 'dark' : 'light';
    saveTheme(theme).catch(() => {
      // ignore migration write failures
    });
  } else {
    theme = 'light';
  }

  return {
    workersUrl: workersUrl ?? '',
    workersToken: workersToken ?? '',
    theme,
    undoSteps:
      Number.isFinite(parsedUndoSteps) && parsedUndoSteps > 0
        ? parsedUndoSteps
        : DEFAULT_MAX_HISTORY,
    agentSessionHistoryCount:
      Number.isFinite(parsedSessionCount) &&
      parsedSessionCount >= AGENT_SESSION_HISTORY_MIN &&
      parsedSessionCount <= AGENT_SESSION_HISTORY_MAX
        ? parsedSessionCount
        : AGENT_SESSION_HISTORY_DEFAULT,
    notifications,
    llmProviders: parseJsonArray<LlmProviderSettings>(llmProvidersStr),
    activeLlmProvider: activeLlmProvider ?? null,
    ttsProviders: parseJsonArray<TtsProviderSettings>(ttsProvidersStr),
    activeTtsProvider: activeTtsProvider ?? null,
  };
}

function parseJsonArray<T>(value: string | null): T[] {
  if (!value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) ? (parsed as T[]) : [];
  } catch {
    return [];
  }
}

export async function saveAgentProviders(
  llmProviders: LlmProviderSettings[],
  activeLlmProvider: string | null,
  ttsProviders: TtsProviderSettings[],
  activeTtsProvider: string | null,
): Promise<void> {
  await Promise.all([
    AsyncStorage.setItem(KEYS.llmProviders, JSON.stringify(llmProviders)),
    AsyncStorage.setItem(KEYS.activeLlmProvider, activeLlmProvider ?? ''),
    AsyncStorage.setItem(KEYS.ttsProviders, JSON.stringify(ttsProviders)),
    AsyncStorage.setItem(KEYS.activeTtsProvider, activeTtsProvider ?? ''),
  ]);
}

export async function loadAgentApiKey(
  kind: 'llm' | 'tts',
  providerId: string,
): Promise<string> {
  return (
    (await SecureStore.getItemAsync(
      `takusu.agent.${kind}.apiKey.${providerId}`,
    )) ?? ''
  );
}

export async function saveAgentApiKey(
  kind: 'llm' | 'tts',
  providerId: string,
  apiKey: string,
): Promise<void> {
  const key = `takusu.agent.${kind}.apiKey.${providerId}`;
  if (apiKey) await SecureStore.setItemAsync(key, apiKey);
  else await SecureStore.deleteItemAsync(key);
}

export async function deleteAgentApiKey(
  kind: 'llm' | 'tts',
  providerId: string,
): Promise<void> {
  await SecureStore.deleteItemAsync(
    `takusu.agent.${kind}.apiKey.${providerId}`,
  );
}

export async function saveWorkersUrl(url: string): Promise<void> {
  await SecureStore.setItemAsync(KEYS.workersUrl, url);
}

export async function saveWorkersToken(token: string): Promise<void> {
  await SecureStore.setItemAsync(KEYS.workersToken, token);
}

export async function saveTheme(theme: AppTheme): Promise<void> {
  await AsyncStorage.setItem(KEYS.theme, theme);
}

export async function saveUndoSteps(steps: number): Promise<void> {
  await AsyncStorage.setItem(KEYS.undoSteps, String(steps));
}

export async function saveAgentSessionHistoryCount(
  count: number,
): Promise<void> {
  const clamped = Math.max(
    AGENT_SESSION_HISTORY_MIN,
    Math.min(AGENT_SESSION_HISTORY_MAX, count),
  );
  await AsyncStorage.setItem(KEYS.agentSessionHistoryCount, String(clamped));
}

export { saveNotificationSettings, DEFAULT_NOTIFICATION_SETTINGS };
export type { NotificationSettings };
