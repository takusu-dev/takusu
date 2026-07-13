import { isRunningInExpoGo } from 'expo';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { PaperProvider, MD3DarkTheme, MD3LightTheme } from 'react-native-paper';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { useEffect, useRef, type RefObject } from 'react';
import * as Notifications from 'expo-notifications';
import { router } from 'expo-router';
import * as Sentry from '@sentry/react-native';
import type { TakusuClient } from '@/src/api/client';
import { ServerProvider, useServer } from '@/src/api/ServerProvider';
import { installGlobalErrorHandler } from '@/src/api/installGlobalErrorHandler';
import { ThemeProvider } from '@/src/theme';
import { UndoRedoToast } from '@/src/components/UndoRedoToast';
import { haptic } from '@/src/components/haptics';
import {
  setupNotificationCategories,
  ensureNotificationPermissions,
  ACTION_DONE,
  ACTION_CANCEL,
  ACTION_START,
  dismissInProgressNotification,
  dismissTaskNotifications,
  cancelScheduledTaskNotifications,
  postInProgressNotification,
  postResultNotification,
} from '@/src/notifications';

// Foreground notification handler — show notifications while app is open
Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldPlaySound: false,
    shouldSetBadge: false,
    shouldShowBanner: true,
    shouldShowList: true,
  }),
});

// Allowlist of valid route prefixes for notification deep links.
// '/' is treated as exact match only — using startsWith('/') would match
// every absolute path and defeat the allowlist purpose.
const VALID_ROUTE_PREFIXES = ['/task/', '/habit/', '/settings'];

function isValidRoute(url: string): boolean {
  return (
    url === '/' || VALID_ROUTE_PREFIXES.some((prefix) => url.startsWith(prefix))
  );
}

if (process.env.EXPO_PUBLIC_SENTRY_DSN) {
  Sentry.init({
    dsn: process.env.EXPO_PUBLIC_SENTRY_DSN,
    environment: __DEV__ ? 'development' : 'production',
    debug: __DEV__,
    tracesSampleRate: 1.0,
    enableNativeFramesTracking: !isRunningInExpoGo(),
    integrations: [
      Sentry.expoRouterIntegration({
        enableTimeToInitialDisplay: !isRunningInExpoGo(),
      }),
    ],
  });
}

// Process a notification action response (START / DONE / CANCEL).
// Returns true if the response was an action button (handled), false otherwise.
// When `client` is null the action is queued via `pendingActions` so it can
// be replayed once the server is ready — this fixes #353 where tapping "開始"
// on a notification silently did nothing because the local server hadn't
// finished starting yet.
function handleActionResponse(
  response: Notifications.NotificationResponse,
  client: TakusuClient | null,
  inProgressNotifEnabled: boolean,
  pendingActions: RefObject<Notifications.NotificationResponse[]>,
): boolean {
  const actionId = response.actionIdentifier;

  // Handle START action (task start reminder → mark in_progress)
  if (actionId === ACTION_START) {
    const taskId = response.notification.request.content.data?.taskId;
    if (typeof taskId !== 'string' || !taskId) return true;
    if (!client) {
      pendingActions.current.push(response);
      return true;
    }
    haptic.medium();
    client
      .updateTask(taskId, { status: 'in_progress' })
      .then(() => {
        // Dismiss the start reminder notification (#257)
        dismissTaskNotifications(taskId).catch((err) =>
          console.warn('Notification action: dismiss failed', err),
        );
        // Post in-progress notification with DONE/CANCEL actions
        if (inProgressNotifEnabled) {
          client
            .getTask(taskId)
            .then((task) =>
              postInProgressNotification(task).catch((err) =>
                console.warn(
                  'Notification action: post in-progress failed',
                  err,
                ),
              ),
            )
            .catch((err) =>
              console.warn('Notification action: getTask failed', err),
            );
        }
      })
      .catch((err) =>
        console.warn('Notification action: updateTask failed', err),
      );
    return true;
  }

  // Handle action button taps (DONE / CANCEL for in-progress tasks)
  if (actionId === ACTION_DONE || actionId === ACTION_CANCEL) {
    const taskId = response.notification.request.content.data?.taskId;
    if (typeof taskId !== 'string' || !taskId) return true;
    if (!client) {
      pendingActions.current.push(response);
      return true;
    }
    const newStatus = actionId === ACTION_DONE ? 'completed' : 'skipped';
    if (actionId === ACTION_DONE) haptic.success();
    else haptic.warning();
    const title = response.notification.request.content.title ?? '';
    const taskTitle = title.replace(/^実行中: /, '') || 'タスク';
    client
      .updateTask(taskId, { status: newStatus })
      .then(() => {
        postResultNotification(taskId, taskTitle, newStatus).catch((err) =>
          console.warn('Notification action: post result failed', err),
        );
        dismissInProgressNotification(taskId).catch((err) =>
          console.warn('Notification action: dismiss failed', err),
        );
        dismissTaskNotifications(taskId).catch((err) =>
          console.warn('Notification action: dismiss task failed', err),
        );
        cancelScheduledTaskNotifications(taskId).catch((err) =>
          console.warn(
            'Notification action: cancel scheduled task failed',
            err,
          ),
        );
      })
      .catch((err) =>
        console.warn('Notification action: updateTask failed', err),
      );
    return true;
  }

  return false;
}

function ThemedApp() {
  const { darkMode, client, notifications } = useServer();
  // Track whether the initial cold-start notification response has been handled
  // to prevent duplicate navigation when client transitions from null to non-null
  const initialResponseHandled = useRef(false);
  // Queue of notification action responses that arrived before `client` was
  // ready (server still starting on cold launch). Drained once `client` is set.
  const pendingActions = useRef<Notifications.NotificationResponse[]>([]);

  // Set up notification channels, categories, permissions, and listeners
  useEffect(() => {
    async function setupNotifications() {
      await ensureNotificationPermissions();
      await setupNotificationCategories();
    }
    setupNotifications();
  }, []);

  // Drain queued action responses once `client` becomes available (#353).
  useEffect(() => {
    if (!client) return;
    if (pendingActions.current.length === 0) return;
    const queued = pendingActions.current;
    pendingActions.current = [];
    for (const response of queued) {
      handleActionResponse(
        response,
        client,
        notifications.inProgress,
        pendingActions,
      );
    }
  }, [client, notifications.inProgress]);

  // Handle notification taps (body tap → navigate to URL in data)
  useEffect(() => {
    function redirect(notification: Notifications.Notification) {
      const url = notification.request.content.data?.url;
      if (typeof url === 'string' && url && isValidRoute(url)) {
        router.push(url);
      }
    }

    // Check if app was opened from a notification (only once on cold start).
    // Only handle default body-tap actions here — action buttons (START/
    // DONE/CANCEL) have opensAppToForeground: false so they don't cold-start
    // the app, and getLastNotificationResponse() could return a stale action
    // response from a previous session. Action buttons are handled by the
    // live listener below, which queues them until `client` is ready (#353).
    if (!initialResponseHandled.current) {
      const lastResponse = Notifications.getLastNotificationResponse();
      if (
        lastResponse?.notification &&
        lastResponse.actionIdentifier ===
          Notifications.DEFAULT_ACTION_IDENTIFIER
      ) {
        redirect(lastResponse.notification);
      }
      initialResponseHandled.current = true;
    }

    const subscription = Notifications.addNotificationResponseReceivedListener(
      (response) => {
        const handled = handleActionResponse(
          response,
          client,
          notifications.inProgress,
          pendingActions,
        );
        if (handled) return;

        // Default action (tap on notification body) → navigate
        redirect(response.notification);
      },
    );

    return () => {
      subscription.remove();
    };
  }, [client, notifications]);

  return (
    <ThemeProvider dark={darkMode}>
      <PaperProvider theme={darkMode ? MD3DarkTheme : MD3LightTheme}>
        <StatusBar style={darkMode ? 'light' : 'dark'} />
        <Stack
          screenOptions={{
            headerShown: false,
            contentStyle: { backgroundColor: darkMode ? '#1A1A2E' : '#fff' },
          }}
        >
          <Stack.Screen name="index" />
          <Stack.Screen name="agent" />
          <Stack.Screen name="task/[id]" />
          <Stack.Screen name="task/add" />
          <Stack.Screen name="habit/[id]" />
          <Stack.Screen name="habit/add" />
          <Stack.Screen name="settings" />
          <Stack.Screen name="import-ical" />
        </Stack>
        <UndoRedoToast />
      </PaperProvider>
    </ThemeProvider>
  );
}

function RootLayout() {
  // Forward uncaught JS exceptions and promise rejections to the native log
  // ring buffer so they appear in log exports alongside server logs.
  useEffect(() => {
    installGlobalErrorHandler();
  }, []);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <ServerProvider>
          <ThemedApp />
        </ServerProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}

export default Sentry.wrap(RootLayout);
