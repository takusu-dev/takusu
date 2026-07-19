// Notification scheduler — cancels all scheduled notifications and reschedules
// based on current tasks, schedule, habits, and notification settings.
//
// Called from HomeView after refresh() and from _layout on app launch.
// In-progress notifications (#5) are posted immediately when a task is started,
// not scheduled here.

import * as Notifications from 'expo-notifications';
import type { TaskRow, ScheduleEntry } from '@/src/api/types';
import { parseSchedule } from '@/src/api/types';
import { type NotificationSettings, minutesToTime } from './settings';
import { CHANNELS } from './channels';
import { CATEGORY_TASK_IN_PROGRESS, CATEGORY_TASK_START } from './categories';

// Android has a ~64 notification limit for scheduled notifications.
// We limit per-task notification batches (pre-start, start-overdue, end-time)
// so the total stays under the platform limit.
const MAX_SCHEDULED_PER_TYPE = 15;

interface ScheduleData {
  tasks: TaskRow[];
  schedule: ScheduleEntry[];
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

// Count incomplete tasks scheduled for today (excludes completed/skipped)
function countTodaysIncompleteTasks(
  tasks: TaskRow[],
  schedule: ScheduleEntry[],
): number {
  const scheduleMap = new Map<string, ScheduleEntry>();
  for (const e of schedule) scheduleMap.set(e.task_id, e);

  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayEnd = new Date();
  todayEnd.setHours(23, 59, 59, 999);

  return tasks.filter((t) => {
    if (t.status === 'completed' || t.status === 'skipped') {
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
  const { tasks, schedule, settings } = data;

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
    const count = countTodaysIncompleteTasks(tasks, schedule);
    const title =
      count === 0
        ? 'おはようございます'
        : `今日は${count}個の未完了タスクがあります`;
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
    const startTime = `${startDate.getHours().toString().padStart(2, '0')}:${startDate
      .getMinutes()
      .toString()
      .padStart(2, '0')}`;

    // Pre-start reminder (#256: attach CATEGORY_TASK_START so the user can
    // start the task early from the reminder notification, not just from the
    // start-overdue one)
    if (settings.preStartReminder) {
      const reminderDate = new Date(
        startDate.getTime() - settings.preStartReminderMinutes * 60 * 1000,
      );
      if (isFuture(reminderDate)) {
        await scheduleAt(
          CHANNELS.taskReminders,
          reminderDate,
          'タスク開始直前',
          `「${task.title}」が${settings.preStartReminderMinutes}分後の${startTime}に開始します`,
          { url: `/task/${task.id}`, taskId: task.id },
          CATEGORY_TASK_START,
        );
      }
    }

    // Start overdue
    if (settings.startOverdue) {
      await scheduleAt(
        CHANNELS.taskReminders,
        startDate,
        'タスク開始時間',
        `「${task.title}」の開始時間です (${startTime})`,
        { url: `/task/${task.id}`, taskId: task.id },
        CATEGORY_TASK_START,
      );
    }
  }

  // ── 3.5 End time notification (#417, #725) — only for in-progress tasks ──
  if (settings.endTime) {
    const endingTasks = tasks
      .filter((t) => t.status === 'in_progress' && scheduleMap.has(t.id))
      .map((t) => ({
        task: t,
        entry: scheduleMap.get(t.id)!,
      }))
      .filter(({ entry }) => {
        const end = new Date(entry.end_at);
        return isTodayOrTomorrow(end) && isFuture(end);
      })
      .sort((a, b) => a.entry.end_at.localeCompare(b.entry.end_at))
      .slice(0, MAX_SCHEDULED_PER_TYPE);

    for (const { task, entry } of endingTasks) {
      const endDate = new Date(entry.end_at);
      const endTime = `${endDate.getHours().toString().padStart(2, '0')}:${endDate
        .getMinutes()
        .toString()
        .padStart(2, '0')}`;
      await scheduleAt(
        CHANNELS.taskReminders,
        endDate,
        'タスク終了時間',
        `「${task.title}」の終了時間です (${endTime})`,
        { url: `/task/${task.id}`, taskId: task.id },
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
}

// ── In-progress notification (#5) — posted immediately, not scheduled ──

export async function postInProgressNotification(task: TaskRow): Promise<void> {
  await Notifications.scheduleNotificationAsync({
    content: {
      title: `実行中: ${task.title}`,
      body: 'タップして詳細を表示',
      data: { url: `/task/${task.id}`, taskId: task.id },
      categoryIdentifier: CATEGORY_TASK_IN_PROGRESS,
      // Keep the in-progress notification visible on tap and prevent swipe dismissal
      // so the user can use the DONE/CANCEL actions while the task is running (#416).
      autoDismiss: false,
      sticky: true,
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

// ── Task result notification (#418) — posted after a notification action completes a task.
export async function postResultNotification(
  taskId: string,
  taskTitle: string,
  status: 'completed' | 'skipped',
): Promise<void> {
  const label = status === 'completed' ? '完了' : 'スキップ';
  await Notifications.scheduleNotificationAsync({
    content: {
      title: `タスクを${label}しました`,
      body: `「${taskTitle}」を${label}しました`,
      data: { url: `/task/${taskId}`, taskId },
    },
    trigger: { channelId: CHANNELS.taskInProgress },
  });
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

// Cancel all pending scheduled notifications for a task.
// When a task is completed or skipped, any scheduled reminders
// (pre-start, start-overdue, end-time) should not fire.
export async function cancelScheduledTaskNotifications(
  taskId: string,
): Promise<void> {
  const scheduled = await Notifications.getAllScheduledNotificationsAsync();
  for (const n of scheduled) {
    if (n.content.data?.taskId === taskId) {
      await Notifications.cancelScheduledNotificationAsync(n.identifier);
    }
  }
}

// Cancel pending start-time notifications for one or more tasks.
// Pre-start reminders and start-overdue reminders are tagged with
// CATEGORY_TASK_START; when a task becomes in_progress, these should not fire.
export async function cancelScheduledStartNotifications(
  taskId: string | string[],
): Promise<void> {
  const ids = new Set(Array.isArray(taskId) ? taskId : [taskId]);
  const scheduled = await Notifications.getAllScheduledNotificationsAsync();
  for (const n of scheduled) {
    const id = n.content.data?.taskId;
    if (
      typeof id === 'string' &&
      ids.has(id) &&
      n.content.categoryIdentifier === CATEGORY_TASK_START
    ) {
      await Notifications.cancelScheduledNotificationAsync(n.identifier);
    }
  }
}

// Wrapper that accepts raw schedule JSON (convenience for HomeView)
export async function rescheduleFromRaw(
  tasks: TaskRow[],
  scheduleJson: string | null,
  settings: NotificationSettings,
): Promise<void> {
  const schedule = scheduleJson ? parseSchedule(scheduleJson) : [];
  await rescheduleNotifications({ tasks, schedule, settings });
}
