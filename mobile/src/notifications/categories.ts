// Notification action categories — interactive notification buttons.

import { Platform } from 'react-native';
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
  // On Android, action buttons with opensAppToForeground: false do not trigger
  // the response listener when the app is not running. Opening the app is the
  // only reliable way to process the action in JS. On iOS the listener fires
  // even in the background, so we keep the silent behavior there (#647).
  const opensAppToForeground = Platform.OS === 'android';

  await Notifications.setNotificationCategoryAsync(CATEGORY_TASK_IN_PROGRESS, [
    {
      identifier: ACTION_DONE,
      buttonTitle: '完了',
      options: { isDestructive: false, opensAppToForeground },
    },
    {
      identifier: ACTION_CANCEL,
      buttonTitle: 'キャンセル',
      options: { isDestructive: true, opensAppToForeground },
    },
  ]);
  await Notifications.setNotificationCategoryAsync(CATEGORY_TASK_START, [
    {
      identifier: ACTION_START,
      buttonTitle: '開始',
      options: { isDestructive: false, opensAppToForeground },
    },
  ]);
}
