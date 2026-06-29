// Home (Task) view — the main screen
// Top bar: hamburger menu, search button, sync button
// Middle: task cards in chronological order (pending on top, date separators)
// Bottom: add button (center), start&done button (right)
// Pull-down-to-reveal past days

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  View,
  RefreshControl,
  type ViewStyle,
} from 'react-native';
import { useRouter } from 'expo-router';
import { useServer } from '@/src/api/ServerProvider';
import { TakusuClient } from '@/src/api/client';
import { undoRedo } from '@/src/api/undoRedo';
import type { TaskRow, ScheduleEntry } from '@/src/api/types';
import { parseDepends, parseSchedule } from '@/src/api/types';
import { TaskCard } from '@/src/components/TaskCard';
import { NavigationButtons } from '@/src/components/NavigationButtons';
import { ViewChanger, type ViewType } from '@/src/components/ViewChanger';
import { ContextMenu } from '@/src/components/ContextMenu';
import { AddButton } from '@/src/components/AddButton';
import { Ionicons } from '@expo/vector-icons';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  withTiming,
} from 'react-native-reanimated';
import { useColors, COLORS, BRAND_COLOR } from '@/src/theme';

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
  const { client } = useServer();
  const router = useRouter();
  const colors = useColors();
  const [tasks, setTasks] = useState<TaskRow[]>([]);
  const [schedule, setSchedule] = useState<ScheduleEntry[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [refreshing, setRefreshing] = useState(false);
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
      const [taskList, sched] = await Promise.all([
        client.listTasks(),
        client.getSchedule().catch(() => null),
      ]);
      setTasks(taskList);
      setSchedule(sched ? parseSchedule(sched.schedule) : []);
    } finally {
      setRefreshing(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

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
    await client.updateTask(task.id, { status: 'completed' });
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
    await client.deleteTask(task.id);
    undoRedo.push({
      description: `delete: ${task.title}`,
      undo: async () => {
        // Re-create with same fields
        await client.createTask({
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
        });
        await refresh();
      },
      redo: async () => {
        await client.deleteTask(task.id);
        await refresh();
      },
    });
    await refresh();
  }

  async function rescheduleSelected() {
    if (!client) return;
    const pinned = tasks
      .filter((t) => !selected.has(t.id))
      .map((t) => t.id);
    const until = new Date();
    until.setDate(until.getDate() + 7);
    await client.reschedule({
      range_start: new Date().toISOString(),
      range_end: until.toISOString(),
      pinned_task_ids: pinned,
    });
    setSelected(new Set());
    await refresh();
  }

  async function rescheduleOthers() {
    if (!client) return;
    const pinned = Array.from(selected);
    const until = new Date();
    until.setDate(until.getDate() + 7);
    await client.reschedule({
      range_start: new Date().toISOString(),
      range_end: until.toISOString(),
      pinned_task_ids: pinned,
    });
    setSelected(new Set());
    await refresh();
  }

  async function deleteSelected() {
    if (!client) return;
    for (const id of selected) {
      const task = tasks.find((t) => t.id === id);
      if (task) await client.deleteTask(id);
    }
    setSelected(new Set());
    await refresh();
  }

  function createDependent() {
    const deps = Array.from(selected);
    setSelected(new Set());
    router.push({ pathname: '/task/add', params: { deps: JSON.stringify(deps) } });
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
        onBack={() => setView('task')}
        viewChanger={<ViewChanger current={view} onChange={setView} />}
      />
    );
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      {/* Top bar */}
      <View style={styles.topBar}>
        <ContextMenu
          hasSelection={selected.size > 0}
          onSettings={() => router.push('/settings')}
          onUndo={() => undoRedo.undo().then(refresh)}
          onRedo={() => undoRedo.redo().then(refresh)}
          onRescheduleSelected={rescheduleSelected}
          onRescheduleOthers={rescheduleOthers}
          onDeleteSelected={deleteSelected}
          onCreateDependent={createDependent}
          onClearSelection={() => setSelected(new Set())}
        />
        <Pressable
          style={({ pressed }) => [styles.topButton, pressed && styles.topButtonPressed]}
          onPress={() => setSearchOpen(!searchOpen)}
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
        <View style={{ flex: 1 }} />
        <Pressable
          style={({ pressed }) => [styles.topButton, pressed && styles.topButtonPressed]}
          onPress={async () => {
            if (!client) return;
            await client.generateSchedule({
              until: new Date(Date.now() + 7 * 86400000).toISOString(),
            });
            // Trigger Google Calendar sync (no-op if not configured)
            await client.triggerSync().catch(() => {});
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
        contentContainerStyle={styles.listContent}
      />

      {/* Bottom bar */}
      <View style={styles.bottomBar}>
        <AddButton onSlideUp={() => router.push('/task/add')} />
        <Pressable
          style={styles.startDoneButton}
          onPress={async () => {
            // Start next pending/scheduled task — mark as in_progress
            const next = tasks.find(
              (t) => t.status === 'scheduled' || t.status === 'pending',
            );
            if (next) {
              if (client && next.status !== 'in_progress') {
                await client.updateTask(next.id, { status: 'in_progress' });
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
  onBack,
  viewChanger,
}: {
  client: TakusuClient | null;
  onBack: () => void;
  viewChanger: React.ReactNode;
}) {
  const { HabitView } = require('@/src/views/HabitView');
  return (
    <View style={{ flex: 1 }}>
      <HabitView client={client} onBack={onBack} />
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
    paddingTop: 48,
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
  topButtonPressed: {
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  topButtonText: {
    fontSize: 20,
  },
  searchInput: {
    flex: 1,
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 6,
    fontSize: 14,
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
