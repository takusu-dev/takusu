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
} as const;

export interface PersistedSettings {
  workersUrl: string;
  workersToken: string;
  darkMode: boolean;
  undoSteps: number;
  notifications: NotificationSettings;
}

export async function loadSettings(): Promise<PersistedSettings> {
  const [
    workersUrl,
    workersToken,
    darkModeStr,
    undoStepsStr,
    notifications,
  ] = await Promise.all([
    SecureStore.getItemAsync(KEYS.workersUrl),
    SecureStore.getItemAsync(KEYS.workersToken),
    AsyncStorage.getItem(KEYS.darkMode),
    AsyncStorage.getItem(KEYS.undoSteps),
    loadNotificationSettings(),
  ]);
  const parsedUndoSteps = undoStepsStr ? parseInt(undoStepsStr, 10) : NaN;
  return {
    workersUrl: workersUrl ?? '',
    workersToken: workersToken ?? '',
    darkMode: darkModeStr === 'true',
    undoSteps: Number.isFinite(parsedUndoSteps) && parsedUndoSteps > 0
      ? parsedUndoSteps
      : DEFAULT_MAX_HISTORY,
    notifications,
  };
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
