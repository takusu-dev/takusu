// Persistent settings store.
// Sensitive values (Workers URL, token) use expo-secure-store.
// Non-sensitive values (darkMode) use @react-native-async-storage/async-storage.

import * as SecureStore from 'expo-secure-store';
import AsyncStorage from '@react-native-async-storage/async-storage';

const KEYS = {
  workersUrl: 'takusu.workersUrl',
  workersToken: 'takusu.workersToken',
  darkMode: 'takusu.darkMode',
} as const;

export interface PersistedSettings {
  workersUrl: string;
  workersToken: string;
  darkMode: boolean;
}

export async function loadSettings(): Promise<PersistedSettings> {
  const [workersUrl, workersToken, darkModeStr] = await Promise.all([
    SecureStore.getItemAsync(KEYS.workersUrl),
    SecureStore.getItemAsync(KEYS.workersToken),
    AsyncStorage.getItem(KEYS.darkMode),
  ]);
  return {
    workersUrl: workersUrl ?? '',
    workersToken: workersToken ?? '',
    darkMode: darkModeStr === 'true',
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
