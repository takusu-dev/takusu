import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { PaperProvider, MD3DarkTheme, MD3LightTheme } from 'react-native-paper';
import { useEffect } from 'react';
import * as Linking from 'expo-linking';
import { ServerProvider, useServer } from '@/src/api/ServerProvider';
import { ThemeProvider } from '@/src/theme';
import { emitOAuthCallback } from '@/src/api/oauthCallback';

function ThemedApp() {
  const { darkMode } = useServer();

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
