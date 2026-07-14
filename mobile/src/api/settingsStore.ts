// Persistent settings store.
// Sensitive values (Workers URL, token) use expo-secure-store.
// Non-sensitive values (darkMode, notification settings) use AsyncStorage.

import * as SecureStore from 'expo-secure-store';
import AsyncStorage from '@react-native-async-storage/async-storage';
import {
  type NotificationSettings,
  DEFAULT_NOTIFICATION_SETTINGS,
  loadNotificationSettings,
  saveNotificationSettings,
} from '@/src/notifications/settings';
import { DEFAULT_MAX_HISTORY } from './undoRedo';

const KEYS = {
  workersUrl: 'takusu.workersUrl',
  workersToken: 'takusu.workersToken',
  darkMode: 'takusu.darkMode',
  undoSteps: 'takusu.undoSteps',
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
  darkMode: boolean;
  undoSteps: number;
  notifications: NotificationSettings;
  llmProviders: LlmProviderSettings[];
  activeLlmProvider: string | null;
  ttsProviders: TtsProviderSettings[];
  activeTtsProvider: string | null;
}

export async function loadSettings(): Promise<PersistedSettings> {
  const [
    workersUrl,
    workersToken,
    darkModeStr,
    undoStepsStr,
    notifications,
    llmProvidersStr,
    activeLlmProvider,
    ttsProvidersStr,
    activeTtsProvider,
  ] = await Promise.all([
    SecureStore.getItemAsync(KEYS.workersUrl),
    SecureStore.getItemAsync(KEYS.workersToken),
    AsyncStorage.getItem(KEYS.darkMode),
    AsyncStorage.getItem(KEYS.undoSteps),
    loadNotificationSettings(),
    AsyncStorage.getItem(KEYS.llmProviders),
    AsyncStorage.getItem(KEYS.activeLlmProvider),
    AsyncStorage.getItem(KEYS.ttsProviders),
    AsyncStorage.getItem(KEYS.activeTtsProvider),
  ]);
  const parsedUndoSteps = undoStepsStr ? parseInt(undoStepsStr, 10) : NaN;
  return {
    workersUrl: workersUrl ?? '',
    workersToken: workersToken ?? '',
    darkMode: darkModeStr === 'true',
    undoSteps:
      Number.isFinite(parsedUndoSteps) && parsedUndoSteps > 0
        ? parsedUndoSteps
        : DEFAULT_MAX_HISTORY,
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

export async function saveDarkMode(enabled: boolean): Promise<void> {
  await AsyncStorage.setItem(KEYS.darkMode, enabled ? 'true' : 'false');
}

export async function saveUndoSteps(steps: number): Promise<void> {
  await AsyncStorage.setItem(KEYS.undoSteps, String(steps));
}

export { saveNotificationSettings, DEFAULT_NOTIFICATION_SETTINGS };
export type { NotificationSettings };
