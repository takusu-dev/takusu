// TaskDetailView — view and edit a single task
// Elements from top to bottom:
//   title, time -> time (if not pending), parallel task, cost (avg, sigma),
//   abandonability (5-step slider), habit (if generated from habit),
//   description, parallel config, deps graph (related only)

import { useCallback, useEffect, useState } from 'react';
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { parseDepends } from '@/src/api/types';
import type { TaskRow, HabitRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR, ABANDON_STEPS } from '@/src/theme';

function formatTime(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
}

export function TaskDetailView() {
  const { client } = useServer();
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string }>();
  const [task, setTask] = useState<TaskRow | null>(null);
  const [habit, setHabit] = useState<HabitRow | null>(null);
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [abandonability, setAbandonability] = useState(0.5);

  const refresh = useCallback(async () => {
    if (!client || !id) return;
    const t = await client.getTask(id);
    setTask(t);
    setTitle(t.title);
    setDescription(t.description ?? '');
    setAbandonability(t.abandonability);
    if (t.habit_id) {
      try {
        setHabit(await client.getHabit(t.habit_id));
      } catch {
        setHabit(null);
      }
    }
  }, [client, id]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function save() {
    if (!client || !task) return;
    const updates = {
      title: title !== task.title ? title : undefined,
      description: description !== (task.description ?? '') ? description : undefined,
      abandonability: abandonability !== task.abandonability ? abandonability : undefined,
    };
    // Only send non-undefined fields
    const filtered = Object.fromEntries(
      Object.entries(updates).filter(([, v]) => v !== undefined),
    );
    if (Object.keys(filtered).length === 0) {
      setEditing(false);
      return;
    }
    const prev = { ...task };
    await client.updateTask(task.id, filtered);
    undoRedo.push({
      description: `edit task: ${task.title}`,
      undo: async () => {
        await client.updateTask(task.id, {
          title: prev.title,
          description: prev.description,
          abandonability: prev.abandonability,
        });
        await refresh();
      },
      redo: async () => {
        await client.updateTask(task.id, filtered);
        await refresh();
      },
    });
    setEditing(false);
    await refresh();
  }

  if (!task) {
    return (
      <View style={styles.container}>
        <Text style={styles.loading}>読み込み中...</Text>
      </View>
    );
  }

  const deps = parseDepends(task.depends);
  const isPending = task.status === 'pending';

  return (
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <View style={{ flex: 1 }} />
        <Pressable
          style={[styles.editButton, editing && styles.editButtonActive]}
          onPress={() => (editing ? save() : setEditing(true))}
        >
          <Text style={[styles.editButtonText, editing && styles.editButtonTextActive]}>
            {editing ? '保存' : '編集'}
          </Text>
        </Pressable>
      </View>

      <ScrollView style={styles.content} contentContainerStyle={styles.contentContainer}>
        {/* Title */}
        {editing ? (
          <TextInput
            style={styles.titleInput}
            value={title}
            onChangeText={setTitle}
          />
        ) : (
          <Text style={styles.title}>{task.title}</Text>
        )}

        {/* Time */}
        {!isPending && (
          <Text style={styles.timeText}>
            {formatTime(task.start_at)} → {formatTime(task.end_at)}
          </Text>
        )}

        {/* Parallel task */}
        {task.allows_parallel && (
          <View style={styles.section}>
            <Text style={styles.sectionLabel}>並列タスク</Text>
            <Text style={styles.sectionValue}>受け皿タスク (allows_parallel)</Text>
          </View>
        )}

        {/* Cost */}
        <View style={styles.section}>
          <Text style={styles.sectionLabel}>コスト</Text>
          <Text style={styles.sectionValue}>
            avg: {task.avg_minutes}m, sigma: {task.sigma_minutes}m
          </Text>
        </View>

        {/* Abandonability */}
        <View style={styles.section}>
          <Text style={styles.sectionLabel}>abandonability</Text>
          {editing ? (
            <View style={styles.sliderContainer}>
              <Slider
                value={abandonability}
                onValueChange={setAbandonability}
                minimumValue={0}
                maximumValue={1}
                step={0.25}
                minimumTrackTintColor={BRAND_COLOR}
              />
              <Text style={styles.sliderValue}>
                {abandonability.toFixed(2)}
              </Text>
            </View>
          ) : (
            <Text style={styles.sectionValue}>
              {task.abandonability.toFixed(2)}
            </Text>
          )}
        </View>

        {/* Habit */}
        {habit && (
          <Pressable
            style={styles.section}
            onPress={() => router.push(`/habit/${habit.id}`)}
          >
            <Text style={styles.sectionLabel}>ハビット</Text>
            <Text style={styles.habitLink}>{habit.title} ›</Text>
          </Pressable>
        )}

        {/* Description */}
        <View style={styles.section}>
          <Text style={styles.sectionLabel}>説明</Text>
          {editing ? (
            <TextInput
              style={styles.descriptionInput}
              value={description}
              onChangeText={setDescription}
              multiline
            />
          ) : (
            <Text style={styles.sectionValue}>
              {task.description || '(なし)'}
            </Text>
          )}
        </View>

        {/* Parallel config */}
        <View style={styles.section}>
          <Text style={styles.sectionLabel}>並列設定</Text>
          <Text style={styles.sectionValue}>
            parallelizable: {task.parallelizable ? 'はい' : 'いいえ'}
            {'\n'}allows_parallel: {task.allows_parallel ? 'はい' : 'いいえ'}
          </Text>
        </View>

        {/* Deps */}
        {deps.length > 0 && (
          <View style={styles.section}>
            <Text style={styles.sectionLabel}>依存 ({deps.length})</Text>
            {deps.map((depId) => (
              <Pressable
                key={depId}
                onPress={() => router.push(`/task/${depId}`)}
              >
                <Text style={styles.depLink}>• {depId.slice(0, 8)}... ›</Text>
              </Pressable>
            ))}
          </View>
        )}
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
  editButton: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: BRAND_COLOR,
  },
  editButtonActive: {
    backgroundColor: BRAND_COLOR,
  },
  editButtonText: {
    fontSize: 14,
    color: BRAND_COLOR,
  },
  editButtonTextActive: {
    color: COLORS.white,
  },
  content: {
    flex: 1,
  },
  contentContainer: {
    padding: 16,
    gap: 16,
  },
  loading: {
    textAlign: 'center',
    marginTop: 40,
    color: COLORS.gray,
  },
  title: {
    fontSize: 24,
    fontWeight: '600',
    color: COLORS.black,
  },
  titleInput: {
    fontSize: 24,
    fontWeight: '600',
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 8,
    padding: 8,
  },
  timeText: {
    fontSize: 14,
    color: COLORS.gray,
  },
  section: {
    gap: 4,
  },
  sectionLabel: {
    fontSize: 13,
    color: COLORS.gray,
    fontWeight: '500',
  },
  sectionValue: {
    fontSize: 16,
    color: COLORS.black,
  },
  sliderContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
  },
  sliderValue: {
    fontSize: 14,
    color: COLORS.brand,
    fontVariant: ['tabular-nums'],
  },
  habitLink: {
    fontSize: 16,
    color: BRAND_COLOR,
  },
  descriptionInput: {
    fontSize: 16,
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 8,
    padding: 8,
    minHeight: 80,
  },
  depLink: {
    fontSize: 14,
    color: BRAND_COLOR,
    paddingVertical: 4,
  },
});
