// Notification action categories — interactive notification buttons.

import * as Notifications from 'expo-notifications';

// Category for in-progress task notifications: DONE + CANCEL actions
export const CATEGORY_TASK_IN_PROGRESS = 'taskinprogress';

// Category for task start reminders: START action (#258)
export const CATEGORY_TASK_START = 'taskstart';

// Action identifiers
export const ACTION_DONE = 'action_done';
export const ACTION_CANCEL = 'action_cancel';
export const ACTION_START = 'action_start';

export async function setupNotificationCategories(): Promise<void> {
  await Promise.all([
    Notifications.setNotificationCategoryAsync(CATEGORY_TASK_IN_PROGRESS, [
      {
        identifier: ACTION_DONE,
        buttonTitle: '完了',
        options: { isDestructive: false, opensAppToForeground: false },
      },
      {
        identifier: ACTION_CANCEL,
        buttonTitle: 'キャンセル',
        options: { isDestructive: true, opensAppToForeground: false },
      },
    ]),
    Notifications.setNotificationCategoryAsync(CATEGORY_TASK_START, [
      {
        identifier: ACTION_START,
        buttonTitle: '開始',
        options: { isDestructive: false, opensAppToForeground: false },
      },
    ]),
  ]);
}
