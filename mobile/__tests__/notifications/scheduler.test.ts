process.env.TZ = 'UTC';

jest.mock('@react-native-async-storage/async-storage', () => ({
  setItem: jest.fn(),
  getItem: jest.fn(),
  removeItem: jest.fn(),
}));

import { rescheduleNotifications } from '@/src/notifications/scheduler';
import type { ScheduleData } from '@/src/notifications/scheduler';
import type { TaskRow, ScheduleEntry } from '@/src/api/types';
import type { NotificationSettings } from '@/src/notifications/settings';

const scheduled: Array<Record<string, unknown>> = [];

jest.mock('expo-notifications', () => ({
  cancelAllScheduledNotificationsAsync: jest.fn().mockResolvedValue(undefined),
  scheduleNotificationAsync: jest.fn(async (request) => {
    scheduled.push(request as Record<string, unknown>);
    return 'notification-id';
  }),
  SchedulableTriggerInputTypes: {
    DATE: 'date',
  },
}));

describe('rescheduleNotifications', () => {
  beforeEach(() => {
    jest.useFakeTimers({ now: new Date('2026-07-23T10:00:00Z').getTime() });
    scheduled.length = 0;
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  const baseTask: Omit<
    TaskRow,
    | 'id'
    | 'title'
    | 'status'
    | 'start_at'
    | 'end_at'
    | 'created_at'
    | 'updated_at'
  > = {
    display_id: 1,
    description: '',
    avg_minutes: 60,
    sigma_minutes: 0,
    depends: '[]',
    parallelizable: false,
    allows_parallel: false,
    abandonability: 0.5,
    user_edited: false,
    fixed: false,
    quantity_done: 0,
    completed_at: undefined,
  };

  const settings: NotificationSettings = {
    enabled: true,
    morningBriefing: false,
    morningBriefingTime: 8 * 60,
    preStartReminder: true,
    preStartReminderMinutes: 5,
    startOverdue: true,
    unscheduledIdle: false,
    unscheduledIdleHours: 24,
    inProgress: true,
    endTime: false,
  };

  it('does not schedule pre-start/start notifications for pending tasks', async () => {
    const startAt = '2026-07-24T09:00:00Z';
    const endAt = '2026-07-24T10:00:00Z';
    const tasks: TaskRow[] = [
      {
        ...baseTask,
        id: 'scheduled-1',
        title: 'Scheduled Task',
        status: 'scheduled',
        start_at: startAt,
        end_at: endAt,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
      {
        ...baseTask,
        id: 'pending-1',
        title: 'Pending Task',
        status: 'pending',
        start_at: startAt,
        end_at: endAt,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
    ];
    const schedule: ScheduleEntry[] = [
      { task_id: 'scheduled-1', start_at: startAt, end_at: endAt },
      { task_id: 'pending-1', start_at: startAt, end_at: endAt },
    ];

    const data: ScheduleData = { tasks, schedule, settings };
    await rescheduleNotifications(data);

    // Only the scheduled task should generate notifications (pre-start + start-overdue = 2).
    // Pending tasks are not in the planner output and should not get these reminders.
    const taskIds = scheduled
      .map((r) => (r.content as { data?: { taskId?: string } }).data?.taskId)
      .filter(Boolean);
    expect(taskIds).toEqual(['scheduled-1', 'scheduled-1']);
  });

  it('morning briefing counts incomplete scheduled tasks for the notification date', async () => {
    // Use a local noon time so the schedule entries fall on the same local day
    // regardless of the test runner's timezone.
    const now = new Date(2026, 6, 23, 10, 0, 0);
    jest.setSystemTime(now.getTime());

    const morningSettings: NotificationSettings = {
      ...settings,
      morningBriefing: true,
      morningBriefingTime: 10 * 60 + 1,
      preStartReminder: false,
      startOverdue: false,
    };
    const todayNoon = new Date(2026, 6, 23, 12, 0, 0).toISOString();
    const todayEnd = new Date(2026, 6, 23, 13, 0, 0).toISOString();
    const tomorrowNoon = new Date(2026, 6, 24, 12, 0, 0).toISOString();
    const tomorrowEnd = new Date(2026, 6, 24, 13, 0, 0).toISOString();

    const tasks: TaskRow[] = [
      {
        ...baseTask,
        id: 'today-done',
        title: 'Today done',
        status: 'completed',
        start_at: todayNoon,
        end_at: todayEnd,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
      {
        ...baseTask,
        id: 'today-todo',
        title: 'Today todo',
        status: 'scheduled',
        start_at: todayNoon,
        end_at: todayEnd,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
      {
        ...baseTask,
        id: 'tomorrow-todo',
        title: 'Tomorrow todo',
        status: 'scheduled',
        start_at: tomorrowNoon,
        end_at: tomorrowEnd,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
      {
        ...baseTask,
        id: 'pending-today',
        title: 'Pending today',
        status: 'pending',
        start_at: todayNoon,
        end_at: todayEnd,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
    ];

    // Include a stale schedule entry for the pending task; it should be ignored.
    const schedule: ScheduleEntry[] = [
      { task_id: 'today-done', start_at: todayNoon, end_at: todayEnd },
      { task_id: 'today-todo', start_at: todayNoon, end_at: todayEnd },
      {
        task_id: 'tomorrow-todo',
        start_at: tomorrowNoon,
        end_at: tomorrowEnd,
      },
      { task_id: 'pending-today', start_at: todayNoon, end_at: todayEnd },
    ];

    const data: ScheduleData = { tasks, schedule, settings: morningSettings };
    await rescheduleNotifications(data);

    const summary = scheduled.find((r) =>
      (r.content as { title?: string }).title?.includes('未完了タスク'),
    );
    expect(summary).toBeDefined();
    expect((summary!.content as { title: string }).title).toBe(
      '今日は1個の未完了タスクがあります',
    );

    // The briefing is set one minute after the current time, so it should be
    // scheduled for today.
    const target = (summary!.trigger as { date: Date }).date;
    expect(target.getFullYear()).toBe(2026);
    expect(target.getMonth()).toBe(6);
    expect(target.getDate()).toBe(23);
    expect(target.getHours()).toBe(10);
    expect(target.getMinutes()).toBe(1);
  });

  it('morning briefing counts tomorrow tasks when the briefing time has passed', async () => {
    const now = new Date(2026, 6, 23, 10, 0, 0);
    jest.setSystemTime(now.getTime());

    const morningSettings: NotificationSettings = {
      ...settings,
      morningBriefing: true,
      morningBriefingTime: 8 * 60,
      preStartReminder: false,
      startOverdue: false,
    };
    const todayNoon = new Date(2026, 6, 23, 12, 0, 0).toISOString();
    const todayEnd = new Date(2026, 6, 23, 13, 0, 0).toISOString();
    const tomorrowNoon = new Date(2026, 6, 24, 12, 0, 0).toISOString();
    const tomorrowEnd = new Date(2026, 6, 24, 13, 0, 0).toISOString();

    const tasks: TaskRow[] = [
      {
        ...baseTask,
        id: 'today-todo',
        title: 'Today todo',
        status: 'scheduled',
        start_at: todayNoon,
        end_at: todayEnd,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
      {
        ...baseTask,
        id: 'tomorrow-todo',
        title: 'Tomorrow todo',
        status: 'scheduled',
        start_at: tomorrowNoon,
        end_at: tomorrowEnd,
        created_at: '2026-07-22T00:00:00Z',
        updated_at: '2026-07-22T00:00:00Z',
      },
    ];

    const schedule: ScheduleEntry[] = [
      { task_id: 'today-todo', start_at: todayNoon, end_at: todayEnd },
      { task_id: 'tomorrow-todo', start_at: tomorrowNoon, end_at: tomorrowEnd },
    ];

    const data: ScheduleData = { tasks, schedule, settings: morningSettings };
    await rescheduleNotifications(data);

    const summary = scheduled.find((r) =>
      (r.content as { title?: string }).title?.includes('未完了タスク'),
    );
    expect(summary).toBeDefined();
    expect((summary!.content as { title: string }).title).toBe(
      '今日は1個の未完了タスクがあります',
    );

    // 08:00 has already passed, so the briefing should be scheduled for
    // tomorrow and count tomorrow's tasks.
    const target = (summary!.trigger as { date: Date }).date;
    expect(target.getFullYear()).toBe(2026);
    expect(target.getMonth()).toBe(6);
    expect(target.getDate()).toBe(24);
    expect(target.getHours()).toBe(8);
    expect(target.getMinutes()).toBe(0);
  });
});
