import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { PaperProvider, MD3DarkTheme, MD3LightTheme } from 'react-native-paper';
import { useEffect } from 'react';
import * as Linking from 'expo-linking';
import * as Notifications from 'expo-notifications';
import { router } from 'expo-router';
import { ServerProvider, useServer } from '@/src/api/ServerProvider';
import { ThemeProvider } from '@/src/theme';
import { emitOAuthCallback } from '@/src/api/oauthCallback';
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

function ThemedApp() {
  const { darkMode, client } = useServer();

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
      if (typeof url === 'string' && url) {
        router.push(url);
      }
    }

    // Check if app was opened from a notification
    const lastResponse = Notifications.getLastNotificationResponse();
    if (lastResponse?.notification) {
      redirect(lastResponse.notification);
    }

    const subscription = Notifications.addNotificationResponseReceivedListener(
      (response) => {
        const actionId = response.actionIdentifier;

        // Handle action button taps (DONE / CANCEL for in-progress tasks)
        if (actionId === ACTION_DONE || actionId === ACTION_CANCEL) {
          const taskId = response.notification.request.content.data?.taskId;
          if (typeof taskId === 'string' && taskId && client) {
            const newStatus = actionId === ACTION_DONE ? 'completed' : 'skipped';
            client.updateTask(taskId, { status: newStatus }).catch(() => {});
            dismissInProgressNotification(taskId).catch(() => {});
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

  useEffect(() => {
    // Listen for OAuth callback deep links: takusu://oauth/callback?code=...
    const subscription = Linking.addEventListener('url', ({ url }) => {
      handleDeepLink(url);
    });

    // Also check for an initial URL (app opened via deep link)
    Linking.getInitialURL().then((url) => {
      if (url) handleDeepLink(url);
    });

    return () => {
      subscription.remove();
    };
  }, []);

  function handleDeepLink(url: string) {
    try {
      const parsed = Linking.parse(url);
      if (parsed.hostname === 'oauth' && parsed.path === 'callback') {
        const code = parsed.queryParams?.code;
        if (typeof code === 'string' && code) {
          emitOAuthCallback(code);
        }
      }
    } catch {
      // ignore malformed URLs
    }
  }

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
      </PaperProvider>
    </ThemeProvider>
  );
}

export default function RootLayout() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <ServerProvider>
        <ThemedApp />
      </ServerProvider>
    </GestureHandlerRootView>
  );
}
