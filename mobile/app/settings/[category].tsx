import { useLocalSearchParams } from 'expo-router';
import { SettingsDetailView } from '@/src/views/SettingsView';
import type { SettingsCategory } from '@/src/views/SettingsView';

const VALID: SettingsCategory[] = [
  'general',
  'sleep',
  'workload',
  'notifications',
  'agent',
  'worker',
  'google',
  'info',
];

export default function SettingsDetailRoute() {
  const { category } = useLocalSearchParams<{ category: string }>();
  const cat = VALID.includes(category as SettingsCategory)
    ? (category as SettingsCategory)
    : 'general';
  return <SettingsDetailView category={cat} />;
}
