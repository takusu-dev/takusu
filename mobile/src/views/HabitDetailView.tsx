// HabitDetailView — view habit info + recent generated tasks

import { useCallback, useEffect, useState } from 'react';
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import { useServer } from '@/src/api/ServerProvider';
import { showError, logError } from '@/src/api/errors';
import type { HabitRow, TaskRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR } from '@/src/theme';

export function HabitDetailView() {
  const { client } = useServer();
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string }>();
  const [habit, setHabit] = useState<HabitRow | null>(null);
  const [tasks, setTasks] = useState<TaskRow[]>([]);

  const refresh = useCallback(async () => {
    if (!client || !id) return;
    try {
      setHabit(await client.getHabit(id));
    } catch (e) {
      showError(e, 'ハビットの取得に失敗');
      return;
    }
    try {
      const allTasks = await client.listTasks({ habit_id: id });
      // Show recent tasks (up to 10)
      setTasks(allTasks.slice(0, 10));
    } catch (e) {
      logError('ハビットのタスク取得', e);
      setTasks([]);
    }
  }, [client, id]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  if (!habit) {
    return (
      <View style={styles.container}>
        <Text style={styles.loading}>読み込み中...</Text>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <Text style={styles.title}>{habit.title}</Text>
      </View>

      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.section}>
          <Text style={styles.label}>タイトル</Text>
          <Text style={styles.value}>{habit.title}</Text>
        </View>

        {habit.description && (
          <View style={styles.section}>
            <Text style={styles.label}>説明</Text>
            <Text style={styles.value}>{habit.description}</Text>
          </View>
        )}

        <View style={styles.section}>
          <Text style={styles.label}>周期</Text>
          <Text style={styles.value}>{habit.recurrence}</Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.label}>時間</Text>
          <Text style={styles.value}>
            {habit.start_time} → {habit.end_time}
          </Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.label}>コスト</Text>
          <Text style={styles.value}>
            avg: {habit.avg_minutes}m, sigma: {habit.sigma_minutes}m
          </Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.label}>abandonability</Text>
          <Text style={styles.value}>{habit.abandonability.toFixed(2)}</Text>
        </View>

        <View style={styles.section}>
          <Text style={styles.label}>アクティブ</Text>
          <Text style={styles.value}>{habit.active ? 'はい' : 'いいえ'}</Text>
        </View>

        {/* Recent generated tasks */}
        <View style={styles.section}>
          <Text style={styles.label}>直近のタスク</Text>
          {tasks.length === 0 ? (
            <Text style={styles.value}>(なし)</Text>
          ) : (
            tasks.map((t) => (
              <Pressable
                key={t.id}
                style={styles.taskItem}
                onPress={() => router.push(`/task/${t.id}`)}
              >
                <Text style={styles.taskItemTitle}>{t.title}</Text>
                <Text style={styles.taskItemStatus}>{t.status}</Text>
              </Pressable>
            ))
          )}
        </View>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingTop: 48,
    paddingBottom: 8,
  },
  backButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  backButtonText: {
    fontSize: 28,
    color: BRAND_COLOR,
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
    color: COLORS.black,
    marginLeft: 8,
  },
  content: {
    padding: 16,
    gap: 16,
  },
  loading: {
    textAlign: 'center',
    marginTop: 40,
    color: COLORS.gray,
  },
  section: {
    gap: 4,
  },
  label: {
    fontSize: 13,
    color: COLORS.gray,
    fontWeight: '500',
  },
  value: {
    fontSize: 16,
    color: COLORS.black,
  },
  taskItem: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingVertical: 12,
    paddingHorizontal: 12,
    backgroundColor: '#F8F5FC',
    borderRadius: 8,
    marginTop: 4,
  },
  taskItemTitle: {
    fontSize: 14,
    color: COLORS.black,
  },
  taskItemStatus: {
    fontSize: 12,
    color: COLORS.gray,
  },
});
