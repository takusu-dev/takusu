// Notification scheduler — cancels all scheduled notifications and reschedules
// based on current tasks, schedule, habits, and notification settings.
//
// Called from HomeView after refresh() and from _layout on app launch.
// In-progress notifications (#5) are posted immediately when a task is started,
// not scheduled here.

import * as Notifications from 'expo-notifications';
import type { TaskRow, ScheduleEntry, HabitRow } from '@/src/api/types';
import { parseSchedule } from '@/src/api/types';
import { type NotificationSettings, minutesToTime } from './settings';
import { CHANNELS } from './channels';
import { CATEGORY_TASK_IN_PROGRESS, CATEGORY_TASK_START } from './categories';

// Android has a ~64 notification limit for scheduled notifications.
// We limit pre-start and start-overdue to today + tomorrow only.
const MAX_SCHEDULED_PER_TYPE = 25;

interface ScheduleData {
  tasks: TaskRow[];
  schedule: ScheduleEntry[];
  habits: HabitRow[];
  settings: NotificationSettings;
}

function isToday(date: Date): boolean {
  const now = new Date();
  return (
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate()
  );
}

function isTomorrow(date: Date): boolean {
  const tomorrow = new Date();
  tomorrow.setDate(tomorrow.getDate() + 1);
  return (
    date.getFullYear() === tomorrow.getFullYear() &&
    date.getMonth() === tomorrow.getMonth() &&
    date.getDate() === tomorrow.getDate()
  );
}

function isTodayOrTomorrow(date: Date): boolean {
  return isToday(date) || isTomorrow(date);
}

function isFuture(date: Date): boolean {
  return date.getTime() > Date.now();
}

// Count tasks scheduled for today
function countTodaysTasks(tasks: TaskRow[], schedule: ScheduleEntry[]): number {
  const scheduleMap = new Map<string, ScheduleEntry>();
  for (const e of schedule) scheduleMap.set(e.task_id, e);

  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayEnd = new Date();
  todayEnd.setHours(23, 59, 59, 999);

  return tasks.filter((t) => {
    if (
      t.status === 'pending' ||
      t.status === 'completed' ||
      t.status === 'skipped'
    ) {
      return false;
    }
    const entry = scheduleMap.get(t.id);
    const start = entry
      ? new Date(entry.start_at)
      : t.start_at
        ? new Date(t.start_at)
        : null;
    if (!start) return false;
    return start >= todayStart && start <= todayEnd;
  }).length;
}

// Count tasks completed today (using updated_at as proxy for completed_at)
function countCompletedToday(tasks: TaskRow[]): number {
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayEnd = new Date();
  todayEnd.setHours(23, 59, 59, 999);

  return tasks.filter((t) => {
    if (t.status !== 'completed') return false;
    const updated = new Date(t.updated_at);
    return updated >= todayStart && updated <= todayEnd;
  }).length;
}

// Count pending tasks idle for more than threshold hours
function countIdlePendingTasks(
  tasks: TaskRow[],
  thresholdHours: number,
): number {
  const threshold = Date.now() - thresholdHours * 60 * 60 * 1000;
  return tasks.filter((t) => {
    if (t.status !== 'pending') return false;
    return new Date(t.created_at).getTime() < threshold;
  }).length;
}

// Count active habits that don't have a completed task today
function countIncompleteHabits(tasks: TaskRow[], habits: HabitRow[]): number {
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayEnd = new Date();
  todayEnd.setHours(23, 59, 59, 999);

  const activeHabits = habits.filter((h) => h.active);
  if (activeHabits.length === 0) return 0;

  // A habit is "completed today" if there's a completed task with that habit_id today
  const completedHabitIds = new Set(
    tasks
      .filter((t) => {
        if (t.status !== 'completed' || !t.habit_id) return false;
        const updated = new Date(t.updated_at);
        return updated >= todayStart && updated <= todayEnd;
      })
      .map((t) => t.habit_id),
  );

  return activeHabits.filter((h) => !completedHabitIds.has(h.id)).length;
}

// Schedule a one-time notification for the next occurrence of a daily time.
// If the time has already passed today, schedule for tomorrow.
// This avoids stale content from DAILY triggers — the app reschedules with
// fresh data each time it's opened.
async function scheduleNextOccurrence(
  channelId: string,
  hour: number,
  minute: number,
  title: string,
  body: string,
  data: Record<string, unknown>,
): Promise<void> {
  const now = new Date();
  const today = new Date(now);
  today.setHours(hour, minute, 0, 0);

  // If the time has already passed today, schedule for tomorrow
  const target =
    today.getTime() > now.getTime()
      ? today
      : new Date(today.getTime() + 24 * 60 * 60 * 1000);

  await scheduleAt(channelId, target, title, body, data);
}

// Schedule a one-time notification at a specific date
async function scheduleAt(
  channelId: string,
  date: Date,
  title: string,
  body: string,
  data: Record<string, unknown>,
  categoryIdentifier?: string,
): Promise<void> {
  await Notifications.scheduleNotificationAsync({
    content: {
      title,
      body,
      data,
      categoryIdentifier,
    },
    trigger: {
      type: Notifications.SchedulableTriggerInputTypes.DATE,
      date,
      channelId,
    },
  });
}

export async function rescheduleNotifications(
  data: ScheduleData,
): Promise<void> {
  const { tasks, schedule, habits, settings } = data;

  if (!settings.enabled) {
    await Notifications.cancelAllScheduledNotificationsAsync();
    return;
  }

  // Cancel all previously scheduled notifications, then reschedule
  await Notifications.cancelAllScheduledNotificationsAsync();

  const scheduleMap = new Map<string, ScheduleEntry>();
  for (const e of schedule) scheduleMap.set(e.task_id, e);

  // ── 1. Morning briefing (next occurrence only) ──
  if (settings.morningBriefing) {
    const { hour, minute } = minutesToTime(settings.morningBriefingTime);
    const count = countTodaysTasks(tasks, schedule);
    const title =
      count === 0 ? 'おはようございます' : `今日は${count}個のタスクがあります`;
    const body = count === 0 ? 'タスクを追加しましょう' : 'タップして確認';
    await scheduleNextOccurrence(
      CHANNELS.taskSummary,
      hour,
      minute,
      title,
      body,
      { url: '/' },
    );
  }

  // ── 2. Pre-start reminder + 3. Start overdue (per-task, today/tomorrow only) ──
  const upcomingTasks = tasks
    .filter(
      (t) =>
        (t.status === 'scheduled' || t.status === 'pending') &&
        scheduleMap.has(t.id),
    )
    .map((t) => ({
      task: t,
      entry: scheduleMap.get(t.id)!,
    }))
    .filter(({ entry }) => {
      const start = new Date(entry.start_at);
      return isTodayOrTomorrow(start) && isFuture(start);
    })
    .sort((a, b) => a.entry.start_at.localeCompare(b.entry.start_at))
    .slice(0, MAX_SCHEDULED_PER_TYPE);

  for (const { task, entry } of upcomingTasks) {
    const startDate = new Date(entry.start_at);

    // Pre-start reminder
    if (settings.preStartReminder) {
      const reminderDate = new Date(
        startDate.getTime() - settings.preStartReminderMinutes * 60 * 1000,
      );
      if (isFuture(reminderDate)) {
        await scheduleAt(
          CHANNELS.taskReminders,
          reminderDate,
          'タスク開始直前',
          `「${task.title}」が${settings.preStartReminderMinutes}分後に開始します`,
          { url: `/task/${task.id}`, taskId: task.id },
        );
      }
    }

    // Start overdue
    if (settings.startOverdue) {
      await scheduleAt(
        CHANNELS.taskReminders,
        startDate,
        'タスク開始時間',
        `「${task.title}」の開始時間です`,
        { url: `/task/${task.id}`, taskId: task.id },
        CATEGORY_TASK_START,
      );
    }
  }

  // ── 4. Unscheduled idle (next occurrence at noon) ──
  if (settings.unscheduledIdle) {
    const idleCount = countIdlePendingTasks(
      tasks,
      settings.unscheduledIdleHours,
    );
    if (idleCount > 0) {
      await scheduleNextOccurrence(
        CHANNELS.taskIdle,
        12,
        0,
        '未スケジュールのタスクがあります',
        `${idleCount}個のタスクが${settings.unscheduledIdleHours}時間以上放置されています`,
        { url: '/' },
      );
    }
  }

  // ── 6. Evening summary (next occurrence only) ──
  if (settings.eveningSummary) {
    const { hour, minute } = minutesToTime(settings.eveningSummaryTime);
    const completedCount = countCompletedToday(tasks);
    await scheduleNextOccurrence(
      CHANNELS.taskSummary,
      hour,
      minute,
      '今日のサマリー',
      `今日は${completedCount}個のタスクを完了しました`,
      { url: '/' },
    );
  }

  // ── 7. Habit reminder (next occurrence only) ──
  if (settings.habitReminder) {
    const { hour, minute } = minutesToTime(settings.habitReminderTime);
    const incompleteCount = countIncompleteHabits(tasks, habits);
    if (incompleteCount > 0) {
      await scheduleNextOccurrence(
        CHANNELS.habitReminder,
        hour,
        minute,
        'Habitリマインダー',
        `今日のHabitが${incompleteCount}個未完了です`,
        { url: '/' },
      );
    }
  }
}

// ── In-progress notification (#5) — posted immediately, not scheduled ──

export async function postInProgressNotification(task: TaskRow): Promise<void> {
  await Notifications.scheduleNotificationAsync({
    content: {
      title: `実行中: ${task.title}`,
      body: 'タップして詳細を表示',
      data: { url: `/task/${task.id}`, taskId: task.id },
      categoryIdentifier: CATEGORY_TASK_IN_PROGRESS,
    },
    // Use channel-aware trigger for immediate delivery on the in-progress channel
    trigger: { channelId: CHANNELS.taskInProgress },
  });
}

export async function dismissInProgressNotification(
  taskId: string,
): Promise<void> {
  // Dismiss all presented notifications for this task
  // expo-notifications doesn't support dismissing by data, so we dismiss all
  // from the in-progress channel. This is acceptable since only one task
  // should be in_progress at a time.
  const presented = await Notifications.getPresentedNotificationsAsync();
  for (const n of presented) {
    if (n.request.content.data?.taskId === taskId) {
      await Notifications.dismissNotificationAsync(n.request.identifier);
    }
  }
}

// Dismiss all delivered notifications for a task (#257).
// When a task is completed, skipped, or started, any already-delivered
// reminder notifications (pre-start, start-overdue) sitting in the
// notification tray should be removed — cancelAllScheduledNotificationsAsync
// only cancels pending (not-yet-fired) notifications, not delivered ones.
export async function dismissTaskNotifications(taskId: string): Promise<void> {
  const presented = await Notifications.getPresentedNotificationsAsync();
  for (const n of presented) {
    if (n.request.content.data?.taskId === taskId) {
      await Notifications.dismissNotificationAsync(n.request.identifier);
    }
  }
}

// Wrapper that accepts raw schedule JSON (convenience for HomeView)
export async function rescheduleFromRaw(
  tasks: TaskRow[],
  scheduleJson: string | null,
  habits: HabitRow[],
  settings: NotificationSettings,
): Promise<void> {
  const schedule = scheduleJson ? parseSchedule(scheduleJson) : [];
  await rescheduleNotifications({ tasks, schedule, habits, settings });
}
