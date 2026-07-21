// Notification action button handler (START / DONE / CANCEL).
// Used both in the foreground UI and in the background notification task.

import * as Notifications from 'expo-notifications';
import * as Sentry from '@sentry/react-native';
import type { TakusuClient } from '@/src/api/client';
import { haptic as defaultHaptic } from '@/src/components/haptics';
import { ACTION_DONE, ACTION_CANCEL, ACTION_START } from './categories';
import {
  postInProgressNotification,
  dismissInProgressNotification,
  dismissTaskNotifications,
  cancelScheduledTaskNotifications,
  cancelScheduledStartNotifications,
  postResultNotification,
} from './scheduler';

export interface ActionHandlerHaptic {
  medium: () => void;
  success: () => void;
  warning: () => void;
}

export const NOOP_HAPTIC: ActionHandlerHaptic = {
  medium: () => {},
  success: () => {},
  warning: () => {},
};

export interface ActionHandlerOptions {
  client: TakusuClient;
  inProgressNotifications: boolean;
  haptic?: ActionHandlerHaptic;
}

function logActionError(
  action: string,
  taskId: string | undefined,
  err: unknown,
): void {
  Sentry.withScope((scope) => {
    scope.setTag('action', action);
    scope.setExtra('taskId', taskId ?? null);
    Sentry.captureException(err);
  });
  console.warn('Notification action failed', { action, taskId, err });
}

// Process a notification action button (START / DONE / CANCEL).
// Returns true if the response was a recognized action button, false otherwise.
export async function handleActionButtonResponse(
  response: Notifications.NotificationResponse,
  options: ActionHandlerOptions,
): Promise<boolean> {
  const { client, inProgressNotifications, haptic = defaultHaptic } = options;
  const actionId = response.actionIdentifier;

  // Handle START action (task start reminder → mark in_progress)
  if (actionId === ACTION_START) {
    const taskId = response.notification.request.content.data?.taskId;
    if (typeof taskId !== 'string' || !taskId) return true;
    haptic.medium();
    try {
      await client.updateTask(taskId, { status: 'in_progress' });
      // Dismiss the start reminder notification (#257)
      await dismissTaskNotifications(taskId);
      // Cancel any pending start-time reminders so an in-progress task
      // does not get a "タスク開始時間" notification later (#648).
      await cancelScheduledStartNotifications(taskId);
      // Post in-progress notification when starting via action (#312)
      if (inProgressNotifications) {
        const task = await client.getTask(taskId);
        await postInProgressNotification(task);
      }
    } catch (err) {
      logActionError(actionId, taskId, err);
    }
    return true;
  }

  // Handle action button taps (DONE / CANCEL for in-progress tasks)
  if (actionId === ACTION_DONE || actionId === ACTION_CANCEL) {
    const taskId = response.notification.request.content.data?.taskId;
    if (typeof taskId !== 'string' || !taskId) return true;
    const newStatus = actionId === ACTION_DONE ? 'completed' : 'skipped';
    if (actionId === ACTION_DONE) haptic.success();
    else haptic.warning();
    const title = response.notification.request.content.title ?? '';
    const taskTitle = title.replace(/^実行中: /, '') || 'タスク';
    try {
      await client.updateTask(taskId, { status: newStatus });
      await Promise.all([
        postResultNotification(taskId, taskTitle, newStatus),
        dismissInProgressNotification(taskId),
        dismissTaskNotifications(taskId),
        cancelScheduledTaskNotifications(taskId),
      ]);
    } catch (err) {
      logActionError(actionId, taskId, err);
    }
    return true;
  }

  return false;
}
