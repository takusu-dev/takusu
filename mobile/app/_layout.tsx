import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { PaperProvider, MD3DarkTheme, MD3LightTheme } from 'react-native-paper';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { useEffect, useRef } from 'react';
import * as Notifications from 'expo-notifications';
import { router } from 'expo-router';
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
  dismissInProgressNotification,
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
  return url === '/' || VALID_ROUTE_PREFIXES.some((prefix) => url.startsWith(prefix));
}

function ThemedApp() {
  const { darkMode, client } = useServer();
  // Track whether the initial cold-start notification response has been handled
  // to prevent duplicate navigation when client transitions from null to non-null
  const initialResponseHandled = useRef(false);

  // Set up notification channels, categories, permissions, and listeners
  useEffect(() => {
    async function setupNotifications() {
      await ensureNotificationPermissions();
      await setupNotificationCategories();
    }
    setupNotifications();
  }, []);

  // Handle notification taps (body tap → navigate to URL in data)
  useEffect(() => {
    function redirect(notification: Notifications.Notification) {
      const url = notification.request.content.data?.url;
      if (typeof url === 'string' && url && isValidRoute(url)) {
        router.push(url);
      }
    }

    // Check if app was opened from a notification (only once on cold start).
    // Only handle default body-tap actions — DONE/CANCEL action buttons have
    // opensAppToForeground: false so they shouldn't cold-start the app, but
    // getLastNotificationResponse() could return a stale action response.
    if (!initialResponseHandled.current) {
      const lastResponse = Notifications.getLastNotificationResponse();
      if (
        lastResponse?.notification &&
        lastResponse.actionIdentifier === Notifications.DEFAULT_ACTION_IDENTIFIER
      ) {
        redirect(lastResponse.notification);
      }
      initialResponseHandled.current = true;
    }

    const subscription = Notifications.addNotificationResponseReceivedListener(
      (response) => {
        const actionId = response.actionIdentifier;

        // Handle action button taps (DONE / CANCEL for in-progress tasks)
        if (actionId === ACTION_DONE || actionId === ACTION_CANCEL) {
          const taskId = response.notification.request.content.data?.taskId;
          if (typeof taskId === 'string' && taskId && client) {
            const newStatus = actionId === ACTION_DONE ? 'completed' : 'skipped';
            if (actionId === ACTION_DONE) haptic.success();
            else haptic.warning();
            client
              .updateTask(taskId, { status: newStatus })
              .catch((err) => console.warn('Notification action: updateTask failed', err));
            dismissInProgressNotification(taskId)
              .catch((err) => console.warn('Notification action: dismiss failed', err));
          }
          return;
        }

        // Default action (tap on notification body) → navigate
        redirect(response.notification);
      },
    );

    return () => {
      subscription.remove();
    };
  }, [client]);

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
          <Stack.Screen name="task/[id]" />
          <Stack.Screen name="task/add" />
          <Stack.Screen name="habit/[id]" />
          <Stack.Screen name="habit/add" />
          <Stack.Screen name="settings" />
        </Stack>
        <UndoRedoToast />
      </PaperProvider>
    </ThemeProvider>
  );
}

export default function RootLayout() {
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
