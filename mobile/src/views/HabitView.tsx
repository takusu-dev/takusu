// HabitView — list of habit cards with add button
// Habits are selectable, context menu (left) changes with selection

import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  FlatList,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useRouter } from 'expo-router';
import { IconButton } from 'react-native-paper';
import type { TakusuClient } from '@/src/api/client';
import { showError, logError } from '@/src/api/errors';
import { parseDepends } from '@/src/api/types';
import type { HabitRow, TaskRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { ContextMenu } from '@/src/components/ContextMenu';
import { haptic } from '@/src/components/haptics';
import { undoRedo } from '@/src/api/undoRedo';
import { parseRule, summarizeRule } from '@/src/api/rrule';

interface HabitViewProps {
  client: TakusuClient | null;
}

export function HabitView({ client }: HabitViewProps) {
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const [habits, setHabits] = useState<HabitRow[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    if (!client) return;
    setRefreshing(true);
    try {
      setHabits(await client.listHabits());
    } catch (e) {
      showError(e, 'Habit一覧の取得に失敗');
    } finally {
      setRefreshing(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function deleteSelected() {
    if (!client) return;
    const toDelete = habits.filter((h) => selected.has(h.id));
    if (toDelete.length === 0) return;
    // #240: confirm before batch-deleting, showing how many associated
    // tasks will also be cascade-deleted. Capture the tasks per habit
    // so undo can restore them alongside the recreated habits.
    let tasksPerHabit: TaskRow[][];
    try {
      tasksPerHabit = await Promise.all(
        toDelete.map((h) => client!.listTasks({ habit_id: h.id })),
      );
    } catch (e) {
      showError(e, 'ハビットのタスク取得に失敗');
      return;
    }
    const taskCount = tasksPerHabit.reduce((sum, ts) => sum + ts.length, 0);
    const message =
      taskCount > 0
        ? `${toDelete.length}件のハビットと関連する${taskCount}件のタスクを削除しますか？`
        : `${toDelete.length}件のハビットを削除しますか？`;
    const confirmed = await new Promise<boolean>((resolve) => {
      Alert.alert(
        'ハビットを削除',
        message,
        [
          {
            text: 'キャンセル',
            style: 'cancel',
            onPress: () => resolve(false),
          },
          {
            text: '削除',
            style: 'destructive',
            onPress: () => resolve(true),
          },
        ],
        { cancelable: true, onDismiss: () => resolve(false) },
      );
    });
    if (!confirmed) return;
    const deleted: HabitRow[] = [];
    const deletedTasksPerHabit: TaskRow[][] = [];
    let failed = 0;
    for (let i = 0; i < toDelete.length; i++) {
      const h = toDelete[i];
      try {
        await client.deleteHabit(h.id);
        deleted.push(h);
        deletedTasksPerHabit.push(tasksPerHabit[i] ?? []);
      } catch (e) {
        failed++;
        logError(`ハビット削除 (${h.id})`, e);
      }
    }
    if (failed > 0) {
      showError(`${failed}件の削除に失敗しました`, 'Habitの削除');
    }
    if (deleted.length === 0) return;
    // Track the ids assigned by the server when undo recreates the habits,
    // so redo deletes the recreated (not the stale original) ids.
    // Push a single grouped undo entry so one undo restores all habits.
    const currentIds: string[] = [...deleted.map((h) => h.id)];
    // Track which habits have been recreated so a retry after partial
    // failure doesn't create duplicates.
    const createdIdx = new Set<number>();
    // Flatten all cascade-deleted tasks across habits into a single
    // list so dependency remapping can handle deps that span habits.
    // Each entry remembers which habit it belonged to so we can set
    // habit_id correctly after the habit is recreated.
    const flatTasks: { task: TaskRow; habitIdx: number }[] = [];
    for (let i = 0; i < deletedTasksPerHabit.length; i++) {
      for (const t of deletedTasksPerHabit[i]) {
        flatTasks.push({ task: t, habitIdx: i });
      }
    }
    const currentTaskIds: string[] = flatTasks.map((ft) => ft.task.id);
    const taskCreatedIdx = new Set<number>();
    const totalTasks = flatTasks.length;
    undoRedo.push({
      description:
        deleted.length === 1
          ? totalTasks > 0
            ? `delete habit + ${totalTasks} tasks: ${deleted[0].title}`
            : `delete habit: ${deleted[0].title}`
          : totalTasks > 0
            ? `delete ${deleted.length} habits + ${totalTasks} tasks`
            : `delete ${deleted.length} habits`,
      undo: async () => {
        // First pass: recreate any habits not yet restored.
        for (let i = 0; i < deleted.length; i++) {
          if (createdIdx.has(i)) continue;
          const h = deleted[i];
          const recreated = await client.createHabit({
            title: h.title,
            description: h.description,
            recurrence: h.recurrence,
            start_time: h.start_time,
            end_time: h.end_time,
            avg_minutes: h.avg_minutes,
            sigma_minutes: h.sigma_minutes,
            parallelizable: h.parallelizable,
            allows_parallel: h.allows_parallel,
            abandonability: h.abandonability,
            fixed: h.fixed,
          });
          if (!h.active) {
            await client.updateHabit(recreated.id, { active: h.active });
          }
          currentIds[i] = recreated.id;
          createdIdx.add(i);
        }
        // Second pass: recreate tasks with no deps (two-pass dep remap
        // mirrors HomeView's batch-delete undo). All habits are now
        // recreated so habit_id is always valid.
        const oldToNew = new Map<string, string>();
        for (let i = 0; i < flatTasks.length; i++) {
          if (taskCreatedIdx.has(i)) {
            oldToNew.set(flatTasks[i].task.id, currentTaskIds[i]);
            continue;
          }
          const { task: t, habitIdx } = flatTasks[i];
          const recreatedTask = await client.createTask({
            title: t.title,
            description: t.description,
            start_at: t.start_at,
            end_at: t.end_at,
            avg_minutes: t.avg_minutes,
            sigma_minutes: t.sigma_minutes,
            depends: [],
            parallelizable: t.parallelizable,
            allows_parallel: t.allows_parallel,
            abandonability: t.abandonability,
            ical_uid: t.ical_uid,
            habit_id: currentIds[habitIdx],
            fixed: t.fixed,
          });
          if (t.status !== 'pending') {
            await client.updateTask(recreatedTask.id, { status: t.status });
          }
          currentTaskIds[i] = recreatedTask.id;
          oldToNew.set(t.id, recreatedTask.id);
          taskCreatedIdx.add(i);
        }
        // Third pass: remap depends to new IDs for deps within the
        // deleted set (deps on tasks outside the set are left as-is).
        for (let i = 0; i < flatTasks.length; i++) {
          const t = flatTasks[i].task;
          const origDeps = parseDepends(t.depends);
          if (origDeps.length === 0) continue;
          const newId = oldToNew.get(t.id)!;
          const remapped = origDeps.map((d) => oldToNew.get(d) ?? d);
          await client.updateTask(newId, { depends: remapped });
        }
        await refresh();
      },
      redo: async () => {
        createdIdx.clear();
        taskCreatedIdx.clear();
        for (const id of currentIds) {
          await client.deleteHabit(id);
        }
        await refresh();
      },
    });
    setSelected(new Set());
    await refresh();
  }

  function toggleSelection(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View style={[styles.topBar, { paddingTop: 8 + insets.top }]}>
        <ContextMenu
          hasSelection={selected.size > 0}
          onSettings={() => router.push('/settings')}
          onUndo={() =>
            undoRedo
              .undo()
              .then(refresh)
              .catch((e) => showError(e, 'アンドゥに失敗'))
          }
          onRedo={() =>
            undoRedo
              .redo()
              .then(refresh)
              .catch((e) => showError(e, 'リドゥに失敗'))
          }
          onSelectAll={() => setSelected(new Set(habits.map((h) => h.id)))}
          onClearSelection={() => setSelected(new Set())}
          onDeleteSelected={deleteSelected}
        />
        <View style={styles.topBarCenter}>
          <Text style={[styles.title, { color: colors.black }]}>Habit</Text>
        </View>
        <IconButton
          icon="plus"
          iconColor={COLORS.white}
          size={24}
          containerColor={BRAND_COLOR}
          onPress={() => {
            haptic.light();
            router.push('/habit/add');
          }}
          style={styles.addButton}
        />
      </View>

      <FlatList
        data={habits}
        keyExtractor={(h) => h.id}
        renderItem={({ item: h }) => (
          <Pressable
            style={[
              styles.habitCard,
              {
                backgroundColor: colors.surface,
                borderColor: colors.separator,
              },
              selected.has(h.id) && styles.habitCardSelected,
            ]}
            onPress={() => {
              if (selected.size > 0) {
                haptic.light();
                toggleSelection(h.id);
              } else {
                haptic.light();
                router.push(`/habit/${h.id}`);
              }
            }}
            onLongPress={() => {
              haptic.medium();
              toggleSelection(h.id);
            }}
          >
            <View style={styles.habitHeader}>
              <Text
                style={[
                  styles.habitTitle,
                  {
                    color: h.active ? colors.black : colors.gray,
                    textDecorationLine: h.active ? 'none' : 'line-through',
                  },
                ]}
              >
                {h.title}
              </Text>
            </View>
            <Text style={[styles.habitTime, { color: colors.gray }]}>
              時間: {h.start_time} → {h.end_time}
            </Text>
            <Text style={[styles.habitRecurrence, { color: colors.gray }]}>
              周期: {summarizeRule(parseRule(h.recurrence))}
            </Text>
            <Text style={[styles.habitCost, { color: colors.gray }]}>
              {h.avg_minutes}m ±{h.sigma_minutes}
            </Text>
            <Text style={[styles.habitParallel, { color: colors.gray }]}>
              parallel:{' '}
              {h.parallelizable && h.allows_parallel
                ? 'host+guest'
                : h.parallelizable
                  ? 'guest'
                  : h.allows_parallel
                    ? 'host'
                    : 'none'}
              {h.fixed ? ' · fixed' : ''}
            </Text>
            <Text style={[styles.habitAbandon, { color: colors.gray }]}>
              abandon: {h.abandonability.toFixed(2)}
            </Text>
          </Pressable>
        )}
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={refresh} />
        }
        contentContainerStyle={[
          styles.listContent,
          { paddingBottom: 100 + insets.bottom },
        ]}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingBottom: 8,
    gap: 4,
  },
  topBarCenter: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
  },
  addButton: {
    width: 40,
    height: 40,
    margin: 0,
  },
  listContent: {
    padding: 12,
    gap: 8,
  },
  habitCard: {
    borderRadius: 12,
    padding: 16,
    gap: 4,
    borderWidth: 1,
  },
  habitCardSelected: {
    borderWidth: 2,
    borderColor: BRAND_COLOR,
  },
  habitHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  habitTitle: {
    fontSize: 16,
    fontWeight: '600',
    flex: 1,
  },
  habitRecurrence: {
    fontSize: 13,
  },
  habitTime: {
    fontSize: 13,
  },
  habitCost: {
    fontSize: 13,
  },
  habitParallel: {
    fontSize: 13,
  },
  habitAbandon: {
    fontSize: 13,
  },
});
