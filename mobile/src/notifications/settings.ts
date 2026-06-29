// Notification settings — persisted in AsyncStorage.

import AsyncStorage from '@react-native-async-storage/async-storage';

const KEY = 'takusu.notifications';

export interface NotificationSettings {
  // Master toggle — when false, no notifications are scheduled
  enabled: boolean;
  // 朝ブリーフィング: "今日はN個のタスク" / N=0 → "タスクを追加しましょう"
  morningBriefing: boolean;
  morningBriefingTime: number; // minutes from midnight (480 = 08:00)
  // 開始直前リマインダー: start_at の N 分前に通知
  preStartReminder: boolean;
  preStartReminderMinutes: number; // 10
  // 開始時間到着: start_at を過ぎたのに未着手のタスク
  startOverdue: boolean;
  // 未スケジュール放置: pending が長時間放置されている
  unscheduledIdle: boolean;
  unscheduledIdleHours: number; // 24
  // 実行中通知: in_progress のタスク (done/cancel アクション付き)
  inProgress: boolean;
  // 夕方サマリー: "今日はN個完了しました"
  eveningSummary: boolean;
  eveningSummaryTime: number; // minutes from midnight (1080 = 18:00)
  // ハビット未完了リマインダー
  habitReminder: boolean;
  habitReminderTime: number; // minutes from midnight (1200 = 20:00)
}

export const DEFAULT_NOTIFICATION_SETTINGS: NotificationSettings = {
  enabled: true,
  morningBriefing: true,
  morningBriefingTime: 8 * 60, // 08:00
  preStartReminder: true,
  preStartReminderMinutes: 10,
  startOverdue: true,
  unscheduledIdle: true,
  unscheduledIdleHours: 24,
  inProgress: true,
  eveningSummary: true,
  eveningSummaryTime: 18 * 60, // 18:00
  habitReminder: true,
  habitReminderTime: 20 * 60, // 20:00
};

export async function loadNotificationSettings(): Promise<NotificationSettings> {
  const raw = await AsyncStorage.getItem(KEY);
  if (!raw) return { ...DEFAULT_NOTIFICATION_SETTINGS };
  try {
    const parsed = JSON.parse(raw) as Partial<NotificationSettings>;
    return { ...DEFAULT_NOTIFICATION_SETTINGS, ...parsed };
  } catch {
    return { ...DEFAULT_NOTIFICATION_SETTINGS };
  }
}

export async function saveNotificationSettings(
  settings: NotificationSettings,
): Promise<void> {
  await AsyncStorage.setItem(KEY, JSON.stringify(settings));
}

// Helper: convert minutes-from-midnight to { hour, minute }
export function minutesToTime(min: number): { hour: number; minute: number } {
  return { hour: Math.floor(min / 60), minute: min % 60 };
}

// Helper: convert { hour, minute } to minutes-from-midnight
export function timeToMinutes(hour: number, minute: number): number {
  return hour * 60 + minute;
}

// Helper: format minutes-from-midnight as "HH:MM"
export function formatTime(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return `${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}`;
}
