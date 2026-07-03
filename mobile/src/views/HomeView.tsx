// Home (Task) view — the main screen
// Top bar: hamburger menu, search button, sync button
// Middle: task cards in chronological order (pending on top, date separators)
// Bottom: add button (center), start&done button (right)
// Pull-down-to-reveal past days

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  BackHandler,
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  View,
  RefreshControl,
  useWindowDimensions,
  type ViewStyle,
} from 'react-native';
import { useRouter } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useServer } from '@/src/api/ServerProvider';
import { TakusuClient } from '@/src/api/client';
import { undoRedo } from '@/src/api/undoRedo';
import { showError, logError } from '@/src/api/errors';
import type { TaskRow, ScheduleEntry } from '@/src/api/types';
import { parseDepends, parseSchedule } from '@/src/api/types';
import { TaskCard } from '@/src/components/TaskCard';
import { NavigationButtons } from '@/src/components/NavigationButtons';
import { ViewChanger, type ViewType } from '@/src/components/ViewChanger';
import { ContextMenu } from '@/src/components/ContextMenu';
import { AddButton } from '@/src/components/AddButton';
import { TaskAddSheet } from '@/src/components/TaskAddSheet';
import { Ionicons } from '@expo/vector-icons';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  withTiming,
} from 'react-native-reanimated';
import { useColors, COLORS, BRAND_COLOR } from '@/src/theme';
import { haptic } from '@/src/components/haptics';
import {
  rescheduleFromRaw,
  postInProgressNotification,
  dismissInProgressNotification,
} from '@/src/notifications';
import type { HabitRow } from '@/src/api/types';

interface TaskItem {
  type: 'task';
  task: TaskRow;
  scheduleStart?: string;
  scheduleEnd?: string;
  isDone: boolean;
  dateKey: string;
  // Parallel receiver task (allows_parallel=true) overlapping in schedule
  parallelTask?: TaskRow;
  parallelScheduleStart?: string;
  parallelScheduleEnd?: string;
}

interface DateSeparator {
  type: 'separator';
  label: string;
}

type ListItem = TaskItem | DateSeparator;

function dateKey(iso: string): string {
  return iso.slice(0, 10);
}

function dateLabel(key: string): string {
  const d = new Date(key + 'T00:00:00');
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const diff = Math.floor(
    (d.getTime() - today.getTime()) / (1000 * 60 * 60 * 24),
  );
  if (diff === 0) return '今日';
  if (diff === 1) return '明日';
  if (diff === -1) return '昨日';
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

export function HomeView() {
  const { client, notifications } = useServer();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const { height: screenHeight } = useWindowDimensions();

  // ── Task-add bottom-sheet preview state ──
  // sheetY drives the sheet's translateY (screenHeight = hidden, 0 = open).
  // sheetMounted controls whether the sheet is rendered at all.
  // sheetOpen controls whether the sheet content is interactive.
  // unmountTimer holds the pending setTimeout id that unmounts the sheet
  // after the close animation; it is cleared whenever a new drag starts so
  // a quick second drag can't have the sheet yanked out from under it.
  const sheetY = useSharedValue(screenHeight);
  const [sheetMounted, setSheetMounted] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const unmountTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function scheduleUnmount(delay = 220) {
    if (unmountTimer.current) clearTimeout(unmountTimer.current);
    unmountTimer.current = setTimeout(() => setSheetMounted(false), delay);
  }

  function cancelUnmount() {
    if (unmountTimer.current) {
      clearTimeout(unmountTimer.current);
      unmountTimer.current = null;
    }
  }

  const [tasks, setTasks] = useState<TaskRow[]>([]);
  const [schedule, setSchedule] = useState<ScheduleEntry[]>([]);
  const [habits, setHabits] = useState<HabitRow[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [refreshing, setRefreshing] = useState(false);
  // In-progress status label shown in the top-bar center while a
  // scheduling / Google Calendar sync operation is running.
  const [statusLabel, setStatusLabel] = useState<string | null>(null);
  const [view, setView] = useState<ViewType>('task');
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [showPast, setShowPast] = useState(false);
  const listRef = useRef<FlatList<ListItem>>(null);
  const scrollOffsetRef = useRef(0);

  // Animated chevron rotation for past-day toggle
  const chevronRotate = useSharedValue(0);
  const chevronStyle = useAnimatedStyle(() => ({
    transform: [{ rotate: `${chevronRotate.value}deg` }],
  }));
  function togglePast() {
    haptic.select();
    setShowPast((v) => {
      const next = !v;
      chevronRotate.value = withTiming(next ? 180 : 0, { duration: 250 });
      return next;
    });
  }

  const refresh = useCallback(async () => {
    if (!client) return;
    setRefreshing(true);
    try {
      const [taskList, sched, habitList] = await Promise.all([
        client.listTasks(),
        client.getSchedule().catch((e) => {
          logError('スケジュール取得', e);
          return null;
        }),
        client.listHabits().catch((e) => {
          logError('Habit取得', e);
          return [] as HabitRow[];
        }),
      ]);
      setTasks(taskList);
      setSchedule(sched ? parseSchedule(sched.schedule) : []);
      setHabits(habitList);
    } catch (e) {
      showError(e, 'タスク一覧の取得に失敗');
    } finally {
      setRefreshing(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Reschedule notifications when tasks, schedule, habits, or notification
  // settings change. This is separate from refresh() to avoid triggering a
  // full server refetch when only notification settings are toggled.
  useEffect(() => {
    if (tasks.length === 0 && habits.length === 0) return;
    rescheduleFromRaw(
      tasks,
      schedule.length > 0 ? JSON.stringify(schedule) : null,
      habits,
      notifications,
    ).catch((e) => logError('通知の再スケジュール', e));
  }, [tasks, schedule, habits, notifications]);

  // Close the task-add sheet on Android hardware back button.  Without this
  // the back button would navigate away from HomeView (or exit the app)
  // instead of dismissing the sheet overlay.
  // closeAddSheetRef always points at the latest closeAddSheet (which reads
  // the current screenHeight), so a rotation while the sheet is open does
  // not leave the handler animating to a stale height.
  const closeAddSheetRef = useRef<() => void>(() => {});
  closeAddSheetRef.current = closeAddSheet;
  useEffect(() => {
    if (!sheetOpen) return;
    const subscription = BackHandler.addEventListener(
      'hardwareBackPress',
      () => {
        closeAddSheetRef.current();
        return true; // prevent default navigation
      },
    );
    return () => subscription.remove();
  }, [sheetOpen]);

  const scheduleMap = useMemo(() => {
    const m = new Map<string, ScheduleEntry>();
    for (const e of schedule) m.set(e.task_id, e);
    return m;
  }, [schedule]);

  // Find parallel receiver task (allows_parallel=true) that overlaps in schedule time
  // for a given parallelizable=true task
  const findParallelTask = useCallback(
    (task: TaskRow): {
      parallelTask: TaskRow;
      parallelScheduleStart?: string;
      parallelScheduleEnd?: string;
    } | null => {
      if (!task.parallelizable) return null;
      const taskEntry = scheduleMap.get(task.id);
      if (!taskEntry) return null;
      const taskStart = new Date(taskEntry.start_at).getTime();
      const taskEnd = new Date(taskEntry.end_at).getTime();

      for (const other of tasks) {
        if (other.id === task.id) continue;
        if (!other.allows_parallel) continue;
        if (other.status === 'completed' || other.status === 'skipped') continue;
        const otherEntry = scheduleMap.get(other.id);
        if (!otherEntry) continue;
        const otherStart = new Date(otherEntry.start_at).getTime();
        const otherEnd = new Date(otherEntry.end_at).getTime();
        // Check for time overlap
        if (otherStart < taskEnd && otherEnd > taskStart) {
          return {
            parallelTask: other,
            parallelScheduleStart: otherEntry.start_at,
            parallelScheduleEnd: otherEntry.end_at,
          };
        }
      }
      return null;
    },
    [tasks, scheduleMap],
  );

  const items: ListItem[] = useMemo(() => {
    const filtered = searchQuery
      ? tasks.filter((t) =>
          t.title.toLowerCase().includes(searchQuery.toLowerCase()),
        )
      : tasks;

    const pending = filtered.filter((t) => t.status === 'pending');
    const scheduled = filtered
      .filter((t) => t.status !== 'pending')
      .sort((a, b) => {
        const sa = scheduleMap.get(a.id)?.start_at ?? a.end_at;
        const sb = scheduleMap.get(b.id)?.start_at ?? b.end_at;
        return sa.localeCompare(sb);
      });

    // Past completed/skipped tasks — always compute count, only include in list when showPast
    const now = Date.now();
    const pastAll = scheduled.filter((t) => {
      const entry = scheduleMap.get(t.id);
      const end = entry?.end_at ?? t.end_at;
      return new Date(end).getTime() < now;
    });
    const past = showPast ? pastAll : [];

    // Upcoming = always exclude past tasks, regardless of showPast
    const upcoming = scheduled.filter((t) => {
      const entry = scheduleMap.get(t.id);
      const end = entry?.end_at ?? t.end_at;
      return new Date(end).getTime() >= now;
    });

    const result: ListItem[] = [];

    // Past section (when revealed)
    if (past.length > 0) {
      result.push({ type: 'separator', label: '過去' });
      let lastDate = '';
      for (const t of past) {
        const entry = scheduleMap.get(t.id);
        const key = dateKey(entry?.start_at ?? t.end_at);
        if (key !== lastDate) {
          result.push({ type: 'separator', label: dateLabel(key) });
          lastDate = key;
        }
        result.push({
          type: 'task',
          task: t,
          scheduleStart: entry?.start_at,
          scheduleEnd: entry?.end_at,
          isDone: t.status === 'completed' || t.status === 'skipped',
          dateKey: key,
        });
      }
    }

    if (pending.length > 0) {
      result.push({ type: 'separator', label: 'pending' });
      for (const t of pending) {
        result.push({
          type: 'task',
          task: t,
          isDone: t.status === 'completed' || t.status === 'skipped',
          dateKey: 'pending',
        });
      }
    }

    let lastDate = '';
    for (const t of upcoming) {
      const entry = scheduleMap.get(t.id);
      const key = dateKey(entry?.start_at ?? t.end_at);
      if (key !== lastDate) {
        result.push({ type: 'separator', label: dateLabel(key) });
        lastDate = key;
      }
      const parallel = findParallelTask(t);
      result.push({
        type: 'task',
        task: t,
        scheduleStart: entry?.start_at,
        scheduleEnd: entry?.end_at,
        isDone: t.status === 'completed' || t.status === 'skipped',
        dateKey: key,
        parallelTask: parallel?.parallelTask,
        parallelScheduleStart: parallel?.parallelScheduleStart,
        parallelScheduleEnd: parallel?.parallelScheduleEnd,
      });
    }

    return result;
  }, [tasks, scheduleMap, searchQuery, findParallelTask, showPast]);

  // Count of past tasks (for badge in header, always computed)
  const pastCount = useMemo(() => {
    const now = Date.now();
    return tasks.filter((t) => {
      if (t.status === 'pending') return false;
      const entry = scheduleMap.get(t.id);
      const end = entry?.end_at ?? t.end_at;
      return new Date(end).getTime() < now;
    }).length;
  }, [tasks, scheduleMap]);

  // Marked dates for calendar overlay (dates that have scheduled tasks)
  const markedDates = useMemo(() => {
    const set = new Set<string>();
    for (const t of tasks) {
      if (t.status === 'pending') continue;
      const entry = scheduleMap.get(t.id);
      const key = dateKey(entry?.start_at ?? t.end_at);
      set.add(key);
    }
    return set;
  }, [tasks, scheduleMap]);

  // Map dateKey → index in items array (for scroll navigation)
  const dateIndexMap = useMemo(() => {
    const m = new Map<string, number>();
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.type === 'separator' && item.label !== 'pending') {
        // Reconstruct dateKey from the label — but we stored label, not key.
        // Instead, find the first task after this separator to get its dateKey.
        for (let j = i + 1; j < items.length; j++) {
          const next = items[j];
          if (next.type === 'task') {
            m.set(next.dateKey, i);
            break;
          }
        }
      }
    }
    return m;
  }, [items]);

  function scrollToDateKey(key: string) {
    const idx = dateIndexMap.get(key);
    if (idx !== undefined && listRef.current) {
      listRef.current.scrollToIndex({ index: idx, animated: true });
    }
  }

  function scrollByDay(direction: -1 | 1) {
    if (!listRef.current) return;
    const newOffset = Math.max(0, scrollOffsetRef.current + direction * 300);
    listRef.current.scrollToOffset({ offset: newOffset, animated: true });
  }

  function scrollByPage(direction: -1 | 1) {
    if (!listRef.current) return;
    const newOffset = Math.max(0, scrollOffsetRef.current + direction * 600);
    listRef.current.scrollToOffset({ offset: newOffset, animated: true });
  }

  function jumpToDate(date: Date) {
    const key = `${date.getFullYear()}-${(date.getMonth() + 1)
      .toString()
      .padStart(2, '0')}-${date.getDate().toString().padStart(2, '0')}`;
    scrollToDateKey(key);
  }

  function toggleSelection(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function markDone(task: TaskRow) {
    if (!client) return;
    const prevStatus = task.status;
    try {
      await client.updateTask(task.id, { status: 'completed' });
    } catch (e) {
      showError(e, 'タスクの完了に失敗');
      return;
    }
    // Dismiss in-progress notification if it was showing
    if (prevStatus === 'in_progress') {
      dismissInProgressNotification(task.id).catch((e) =>
        logError('通知の消去', e),
      );
    }
    undoRedo.push({
      description: `mark done: ${task.title}`,
      undo: async () => {
        await client.updateTask(task.id, { status: prevStatus });
        await refresh();
      },
      redo: async () => {
        await client.updateTask(task.id, { status: 'completed' });
        await refresh();
      },
    });
    await refresh();
  }

  async function deleteTask(task: TaskRow) {
    if (!client) return;
    try {
      await client.deleteTask(task.id);
    } catch (e) {
      showError(e, 'タスクの削除に失敗');
      return;
    }
    // Track the id assigned by the server when undo recreates the task,
    // so redo deletes the recreated (not the stale original) id.
    let currentId = task.id;
    undoRedo.push({
      description: `delete: ${task.title}`,
      undo: async () => {
        // Re-create with same fields
        const recreated = await client.createTask({
          title: task.title,
          description: task.description,
          start_at: task.start_at,
          end_at: task.end_at,
          avg_minutes: task.avg_minutes,
          sigma_minutes: task.sigma_minutes,
          depends: parseDepends(task.depends),
          parallelizable: task.parallelizable,
          allows_parallel: task.allows_parallel,
          abandonability: task.abandonability,
          ical_uid: task.ical_uid,
          habit_id: task.habit_id,
        });
        // CreateTask does not accept `status`; restore it via update.
        if (task.status !== 'pending') {
          await client.updateTask(recreated.id, { status: task.status });
        }
        currentId = recreated.id;
        await refresh();
      },
      redo: async () => {
        await client.deleteTask(currentId);
        await refresh();
      },
    });
    await refresh();
  }

  // Run an async operation while showing a status label in the top-bar
  // center. The label is cleared when the operation finishes (success or
  // failure).
  async function withStatus<T>(label: string, fn: () => Promise<T>): Promise<T> {
    setStatusLabel(label);
    try {
      return await fn();
    } finally {
      setStatusLabel(null);
    }
  }

  async function rescheduleSelected() {
    if (!client) return;
    const pinned = tasks
      .filter((t) => !selected.has(t.id))
      .map((t) => t.id);
    const until = new Date();
    until.setDate(until.getDate() + 7);
    try {
      await withStatus('reschedule中', () =>
        client.reschedule({
          range_start: new Date().toISOString(),
          range_end: until.toISOString(),
          pinned_task_ids: pinned,
        }),
      );
    } catch (e) {
      showError(e, '再スケジュールに失敗');
      return;
    }
    setSelected(new Set());
    await refresh();
  }

  async function rescheduleOthers() {
    if (!client) return;
    const pinned = Array.from(selected);
    const until = new Date();
    until.setDate(until.getDate() + 7);
    try {
      await withStatus('reschedule中', () =>
        client.reschedule({
          range_start: new Date().toISOString(),
          range_end: until.toISOString(),
          pinned_task_ids: pinned,
        }),
      );
    } catch (e) {
      showError(e, '再スケジュールに失敗');
      return;
    }
    setSelected(new Set());
    await refresh();
  }

  async function deleteSelected() {
    if (!client) return;
    const toDelete = tasks.filter((t) => selected.has(t.id));
    const deleted: TaskRow[] = [];
    let failed = 0;
    for (const task of toDelete) {
      try {
        await client.deleteTask(task.id);
        deleted.push(task);
      } catch (e) {
        failed++;
        logError(`タスク削除 (${task.title})`, e);
      }
    }
    if (failed > 0) {
      showError(`${failed}件の削除に失敗しました`, 'タスクの削除');
    }
    if (deleted.length === 0) return;
    // Track the ids assigned by the server when undo recreates the tasks,
    // so redo deletes the recreated (not the stale original) ids.
    // Push a single grouped undo entry so one undo restores all tasks.
    const currentIds: string[] = [...deleted.map((t) => t.id)];
    // Track which items have been recreated so a retry after partial failure
    // doesn't create duplicates.
    const createdIdx = new Set<number>();
    undoRedo.push({
      description:
        deleted.length === 1
          ? `delete: ${deleted[0].title}`
          : `delete ${deleted.length} tasks`,
      undo: async () => {
        const oldToNew = new Map<string, string>();
        // First pass: create tasks not yet recreated (skip on retry).
        for (let i = 0; i < deleted.length; i++) {
          if (createdIdx.has(i)) {
            // Already recreated on a previous (partial) attempt.
            oldToNew.set(deleted[i].id, currentIds[i]);
            continue;
          }
          const task = deleted[i];
          const recreated = await client.createTask({
            title: task.title,
            description: task.description,
            start_at: task.start_at,
            end_at: task.end_at,
            avg_minutes: task.avg_minutes,
            sigma_minutes: task.sigma_minutes,
            depends: [],
            parallelizable: task.parallelizable,
            allows_parallel: task.allows_parallel,
            abandonability: task.abandonability,
            ical_uid: task.ical_uid,
            habit_id: task.habit_id,
          });
          // CreateTask does not accept `status`; restore it via update.
          if (task.status !== 'pending') {
            await client.updateTask(recreated.id, { status: task.status });
          }
          currentIds[i] = recreated.id;
          oldToNew.set(task.id, recreated.id);
          createdIdx.add(i);
        }
        // Second pass: remap depends to new IDs for deps within the deleted set.
        for (let i = 0; i < deleted.length; i++) {
          const task = deleted[i];
          const origDeps = parseDepends(task.depends);
          if (origDeps.length === 0) continue;
          const newId = oldToNew.get(task.id)!;
          const remapped = origDeps.map((d) => oldToNew.get(d) ?? d);
          await client.updateTask(newId, { depends: remapped });
        }
        await refresh();
      },
      redo: async () => {
        createdIdx.clear();
        for (const id of currentIds) {
          await client.deleteTask(id);
        }
        await refresh();
      },
    });
    setSelected(new Set());
    await refresh();
  }

  function createDependent() {
    const deps = Array.from(selected);
    setSelected(new Set());
    router.push({ pathname: '/task/add', params: { deps: JSON.stringify(deps) } });
  }

  // ── Bottom-sheet preview handlers (AddButton drag → TaskAddSheet) ──
  function handleAddDragStart() {
    // Cancel any pending unmount so a quick second drag can't have the
    // sheet yanked out from under the user mid-gesture.
    cancelUnmount();
    // Reset to current hidden position so a dimension change (e.g. rotation)
    // doesn't leave sheetY at a stale value that would flash the sheet.
    sheetY.value = screenHeight;
    setSheetMounted(true);
    setSheetOpen(false);
  }

  function handleAddDragEnd(committed: boolean) {
    if (committed) {
      cancelUnmount();
      sheetY.value = withTiming(0, { duration: 200 });
      setSheetOpen(true);
    } else {
      sheetY.value = withTiming(screenHeight, { duration: 200 });
      setSheetOpen(false);
      // Unmount after the close animation finishes.
      scheduleUnmount();
    }
  }

  function closeAddSheet() {
    sheetY.value = withTiming(screenHeight, { duration: 200 });
    setSheetOpen(false);
    scheduleUnmount();
    // Refresh the task list so a newly created task appears immediately.
    refresh();
  }

  function renderItem(item: ListItem) {
    if (item.type === 'separator') {
      return (
        <View style={styles.separator}>
          <View style={styles.separatorBar} />
          <Text style={styles.separatorText}>{item.label}</Text>
          <View style={styles.separatorBar} />
        </View>
      );
    }
    const isSelected = selected.has(item.task.id);
    return (
      <TaskCard
        task={item.task}
        scheduleStart={item.scheduleStart}
        scheduleEnd={item.scheduleEnd}
        isDone={item.isDone}
        selected={isSelected}
        parallelTask={item.parallelTask}
        parallelScheduleStart={item.parallelScheduleStart}
        parallelScheduleEnd={item.parallelScheduleEnd}
        onPress={() => {
          if (selected.size > 0) {
            toggleSelection(item.task.id);
          } else {
            router.push(`/task/${item.task.id}`);
          }
        }}
        onLongPress={() => toggleSelection(item.task.id)}
        onDone={() => markDone(item.task)}
        onDelete={() => deleteTask(item.task)}
        onParallelPress={() => {
          if (item.parallelTask) router.push(`/task/${item.parallelTask.id}`);
        }}
        onParallelDone={() => {
          if (item.parallelTask && client) markDone(item.parallelTask);
        }}
        onParallelDelete={() => {
          if (item.parallelTask && client) deleteTask(item.parallelTask);
        }}
      />
    );
  }

  if (view === 'graph') {
    return (
      <GraphWrapper
        client={client}
        onBack={() => setView('task')}
        onTaskPress={(taskId) => router.push(`/task/${taskId}`)}
        viewChanger={<ViewChanger current={view} onChange={setView} />}
      />
    );
  }

  if (view === 'habit') {
    return (
      <HabitWrapper
        client={client}
        viewChanger={<ViewChanger current={view} onChange={setView} />}
      />
    );
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      {/* Top bar */}
      <View style={[styles.topBar, { paddingTop: 8 + insets.top }]}>
        <ContextMenu
          hasSelection={selected.size > 0}
          onSettings={() => router.push('/settings')}
          onUndo={() =>
            undoRedo.undo().then(refresh).catch((e) => showError(e, 'アンドゥに失敗'))
          }
          onRedo={() =>
            undoRedo.redo().then(refresh).catch((e) => showError(e, 'リドゥに失敗'))
          }
          onSelectAll={() =>
            setSelected(
              new Set(
                items
                  .filter((it): it is TaskItem => it.type === 'task')
                  .map((it) => it.task.id),
              ),
            )
          }
          onRescheduleSelected={rescheduleSelected}
          onRescheduleOthers={rescheduleOthers}
          onDeleteSelected={deleteSelected}
          onCreateDependent={createDependent}
          onClearSelection={() => setSelected(new Set())}
        />
        <Pressable
          style={({ pressed }) => [styles.topButton, pressed && styles.topButtonPressed]}
          onPress={() => { haptic.light(); setSearchOpen(!searchOpen); }}
        >
          <Ionicons name="search-outline" size={22} color={BRAND_COLOR} />
        </Pressable>
        {searchOpen && (
          <TextInput
            style={[styles.searchInput, { borderColor: colors.separator, color: colors.black }]}
            value={searchQuery}
            onChangeText={setSearchQuery}
            placeholder="検索..."
            placeholderTextColor={colors.grayLight}
            autoFocus
          />
        )}
        <View style={styles.topBarCenter}>
          {statusLabel && (
            <View style={styles.statusPill}>
              <ActivityIndicator size="small" color={BRAND_COLOR} />
              <Text style={styles.statusText}>{statusLabel}</Text>
            </View>
          )}
        </View>
        <Pressable
          style={({ pressed }) => [styles.topButton, pressed && styles.topButtonPressed]}
          onPress={async () => {
            if (!client) return;
            haptic.medium();
            try {
              await withStatus('スケジュール生成中', () =>
                client.generateSchedule({}),
              );
              // Trigger Google Calendar sync (no-op if not configured)
              await withStatus('GCal同期中', () =>
                client.triggerSync().catch((e) =>
                  logError('Google Calendar同期', e),
                ),
              );
            } catch (e) {
              showError(e, 'スケジュール生成に失敗');
            }
            await refresh();
          }}
        >
          <Ionicons name="refresh" size={22} color={BRAND_COLOR} />
        </Pressable>
      </View>

      {/* Task list */}
      <FlatList
        ref={listRef}
        data={items}
        keyExtractor={(item, i) =>
          item.type === 'separator' ? `sep-${i}` : `task-${item.task.id}`
        }
        renderItem={({ item }) => renderItem(item)}
        ListHeaderComponent={
          pastCount > 0 ? (
            <Pressable
              style={styles.pastToggle}
              onPress={togglePast}
            >
              <Reanimated.View style={chevronStyle}>
                <Ionicons
                  name="chevron-down"
                  size={16}
                  color={BRAND_COLOR}
                />
              </Reanimated.View>
              <Text style={styles.pastToggleText}>
                {showPast ? '過去を隠す' : '過去を表示'}
              </Text>
              <View style={[styles.pastBadge, { backgroundColor: BRAND_COLOR }]}>
                <Text style={styles.pastBadgeText}>{pastCount}</Text>
              </View>
            </Pressable>
          ) : null
        }
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={refresh}
          />
        }
        onScroll={(e) => {
          scrollOffsetRef.current = e.nativeEvent.contentOffset.y;
        }}
        scrollEventThrottle={16}
        onScrollToIndexFailed={({ index, averageItemLength }) => {
          // Fallback: scroll to approximate offset
          listRef.current?.scrollToOffset({
            offset: index * averageItemLength,
            animated: true,
          });
        }}
        contentContainerStyle={[styles.listContent, { paddingBottom: 100 + insets.bottom }]}
      />

      {/* Bottom bar */}
      <View style={[styles.bottomBar, { paddingBottom: 16 + insets.bottom }]}>
        <AddButton
          onSlideUp={() => {}}
          sheetY={sheetY}
          screenHeight={screenHeight}
          onDragStart={handleAddDragStart}
          onDragEnd={handleAddDragEnd}
        />
        <Pressable
          style={[styles.startDoneButton, { bottom: 16 + insets.bottom }]}
          onPress={async () => {
            // Start next pending/scheduled task — mark as in_progress
            const next = tasks.find(
              (t) => t.status === 'scheduled' || t.status === 'pending',
            );
            if (next) {
              haptic.medium();
              if (client && next.status !== 'in_progress') {
                try {
                  await client.updateTask(next.id, { status: 'in_progress' });
                  // Post in-progress notification with DONE/CANCEL actions
                  if (notifications.inProgress) {
                    postInProgressNotification(next).catch((e) =>
                      logError('通知の投稿', e),
                    );
                  }
                } catch (e) {
                  showError(e, 'タスクの開始に失敗');
                  return;
                }
              }
              router.push(`/task/${next.id}`);
            }
          }}
        >
          <Ionicons name="play" size={24} color={COLORS.white} />
        </Pressable>
      </View>

      {/* Floating navigation */}
      <NavigationButtons
        onScrollUpByDay={() => scrollByDay(-1)}
        onScrollUpByPage={() => scrollByPage(-1)}
        onScrollDownByDay={() => scrollByDay(1)}
        onScrollDownByPage={() => scrollByPage(1)}
        onJumpToDate={jumpToDate}
        markedDates={markedDates}
      />

      {/* View changer */}
      <ViewChanger current={view} onChange={setView} />

      {/* Task-add bottom-sheet preview (revealed by dragging the add button) */}
      {sheetMounted && (
        <TaskAddSheet
          sheetY={sheetY}
          screenHeight={screenHeight}
          open={sheetOpen}
          onClose={closeAddSheet}
        />
      )}
    </View>
  );
}

// Placeholder wrappers for graph and habit views within home
function GraphWrapper({
  client,
  onBack,
  viewChanger,
  onTaskPress,
}: {
  client: TakusuClient | null;
  onBack: () => void;
  viewChanger: React.ReactNode;
  onTaskPress: (taskId: string) => void;
}) {
  // Lazy load to avoid circular deps
  const { GraphView } = require('@/src/views/GraphView');
  return (
    <View style={{ flex: 1 }}>
      <GraphView client={client} onBack={onBack} onTaskPress={onTaskPress} />
      {viewChanger}
    </View>
  );
}

function HabitWrapper({
  client,
  viewChanger,
}: {
  client: TakusuClient | null;
  viewChanger: React.ReactNode;
}) {
  const { HabitView } = require('@/src/views/HabitView');
  return (
    <View style={{ flex: 1 }}>
      <HabitView client={client} />
      {viewChanger}
    </View>
  );
}

// Need to import TextInput
import { TextInput } from 'react-native';

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  pastToggle: {
    flexDirection: 'row',
    paddingVertical: 10,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
  },
  pastToggleText: {
    fontSize: 13,
    color: BRAND_COLOR,
    fontWeight: '500',
  },
  pastBadge: {
    minWidth: 20,
    height: 20,
    borderRadius: 10,
    paddingHorizontal: 6,
    alignItems: 'center',
    justifyContent: 'center',
  },
  pastBadgeText: {
    fontSize: 11,
    color: COLORS.white,
    fontWeight: '600',
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingBottom: 8,
    gap: 4,
  },
  topButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  topBarCenter: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  statusPill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 14,
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  statusText: {
    fontSize: 12,
    color: BRAND_COLOR,
    fontWeight: '500',
  },
  topButtonPressed: {
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  topButtonText: {
    fontSize: 20,
  },
  searchInput: {
    flex: 1,
    height: 40,
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 12,
    paddingHorizontal: 16,
    paddingVertical: 0,
    fontSize: 16,
  },
  listContent: {
    paddingBottom: 100,
  },
  separator: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 8,
    gap: 8,
  },
  separatorBar: {
    flex: 1,
    height: 1,
    backgroundColor: COLORS.separator,
  },
  separatorText: {
    fontSize: 12,
    color: COLORS.gray,
    fontWeight: '500',
  },
  bottomBar: {
    position: 'absolute',
    bottom: 0,
    left: 0,
    right: 0,
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    paddingVertical: 16,
    paddingHorizontal: 24,
    gap: 16,
  },
  addButton: {
    width: 56,
    height: 56,
    borderRadius: 28,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
    elevation: 4,
  },
  addButtonText: {
    fontSize: 28,
    color: COLORS.white,
    fontWeight: '300',
  },
  startDoneButton: {
    position: 'absolute',
    right: 24,
    bottom: 16,
    width: 48,
    height: 48,
    borderRadius: 24,
    backgroundColor: COLORS.green,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
    elevation: 4,
  },
  startDoneText: {
    fontSize: 20,
    color: COLORS.white,
  },
} as Record<string, ViewStyle>);
