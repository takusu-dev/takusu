// Notification module — re-exports and setup helpers.

export { type NotificationSettings, DEFAULT_NOTIFICATION_SETTINGS } from './settings';
export {
  loadNotificationSettings,
  saveNotificationSettings,
  minutesToTime,
  timeToMinutes,
  formatTime,
} from './settings';
export { CHANNELS } from './channels';
export { setupNotificationChannels } from './channels';
export {
  CATEGORY_TASK_IN_PROGRESS,
  ACTION_DONE,
  ACTION_CANCEL,
  setupNotificationCategories,
} from './categories';
export { ensureNotificationPermissions } from './permissions';
export {
  rescheduleNotifications,
  rescheduleFromRaw,
  postInProgressNotification,
  dismissInProgressNotification,
} from './scheduler';
