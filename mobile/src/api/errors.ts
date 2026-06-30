// Shared error display helpers.
// All async UI actions should surface failures to the user via `showError`
// rather than silently swallowing them. Notification side-effects (which are
// non-critical and should not interrupt the user) use `logError`.

import { Alert } from 'react-native';
import { ApiError } from './client';

/** Format an unknown error into a human-readable string. */
export function formatError(e: unknown): string {
  if (e instanceof ApiError) {
    // Try to parse the response body as JSON for a structured message,
    // otherwise fall back to the raw body.
    try {
      const parsed = JSON.parse(e.body);
      if (typeof parsed === 'string') return parsed;
      if (parsed && typeof parsed.error === 'string') return parsed.error;
      if (parsed && typeof parsed.message === 'string') return parsed.message;
    } catch {
      // not JSON
    }
    if (e.body) return e.body;
    return `HTTP ${e.status}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * Show an alert for an operation failure.
 * `title` defaults to "エラー" but can be overridden for context
 * (e.g. "タスクの削除に失敗").
 */
export function showError(e: unknown, title = 'エラー'): void {
  Alert.alert(title, formatError(e));
}

/**
 * Log a non-critical error without interrupting the user.
 * Used for notification side-effects where a failure should not block the UI.
 */
export function logError(context: string, e: unknown): void {
  console.warn(`${context}:`, formatError(e));
}
