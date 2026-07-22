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
});
