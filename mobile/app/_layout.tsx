import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { useEffect } from 'react';
import { ServerProvider } from '@/src/api/ServerProvider';

export default function RootLayout() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <ServerProvider>
        <StatusBar style="auto" />
        <Stack
          screenOptions={{
            headerShown: false,
            contentStyle: { backgroundColor: '#fff' },
          }}
        >
          <Stack.Screen name="index" />
          <Stack.Screen name="task/[id]" />
          <Stack.Screen name="task/add" />
          <Stack.Screen name="habit/[id]" />
          <Stack.Screen name="habit/add" />
          <Stack.Screen name="settings" />
        </Stack>
      </ServerProvider>
    </GestureHandlerRootView>
  );
}
