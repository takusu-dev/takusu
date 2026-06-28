// Home (Task) view — the main screen
// Top bar: hamburger menu, search button, sync button
// Middle: task cards in chronological order (pending on top, date separators)
// Bottom: add button (center), start&done button (right)
// Pull-down-to-reveal past days

import { useCallback, useEffect, useMemo, useState } from 'react';
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
import { COLORS, BRAND_COLOR } from '@/src/theme';

interface TaskItem {
  type: 'task';
  task: TaskRow;
  scheduleStart?: string;
  scheduleEnd?: string;
  isDone: boolean;
  dateKey: string;
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
  const [tasks, setTasks] = useState<TaskRow[]>([]);
  const [schedule, setSchedule] = useState<ScheduleEntry[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [refreshing, setRefreshing] = useState(false);
  const [view, setView] = useState<ViewType>('task');
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

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

    const result: ListItem[] = [];

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
    for (const t of scheduled) {
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

    return result;
  }, [tasks, scheduleMap, searchQuery]);

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
      />
    );
  }

  if (view === 'graph') {
    return (
      <GraphWrapper
        client={client}
        onBack={() => setView('task')}
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
    <View style={styles.container}>
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
          <Text style={styles.topButtonText}>🔍</Text>
        </Pressable>
        {searchOpen && (
          <TextInput
            style={styles.searchInput}
            value={searchQuery}
            onChangeText={setSearchQuery}
            placeholder="検索..."
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
            await refresh();
          }}
        >
          <Text style={styles.topButtonText}>🔄</Text>
        </Pressable>
      </View>

      {/* Task list */}
      <FlatList
        data={items}
        keyExtractor={(item, i) =>
          item.type === 'separator' ? `sep-${i}` : `task-${item.task.id}`
        }
        renderItem={({ item }) => renderItem(item)}
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={refresh} />
        }
        contentContainerStyle={styles.listContent}
      />

      {/* Bottom bar */}
      <View style={styles.bottomBar}>
        <Pressable
          style={styles.addButton}
          onPress={() => router.push('/task/add')}
        >
          <Text style={styles.addButtonText}>+</Text>
        </Pressable>
        <Pressable
          style={styles.startDoneButton}
          onPress={() => {
            // Start next pending/scheduled task
            const next = tasks.find(
              (t) => t.status === 'scheduled' || t.status === 'pending',
            );
            if (next) router.push(`/task/${next.id}`);
          }}
        >
          <Text style={styles.startDoneText}>▶</Text>
        </Pressable>
      </View>

      {/* Floating navigation */}
      <NavigationButtons
        onScrollUpByDay={() => {}}
        onScrollUpByPage={() => {}}
        onScrollDownByDay={() => {}}
        onScrollDownByPage={() => {}}
        onJumpToDate={() => {}}
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
}: {
  client: TakusuClient | null;
  onBack: () => void;
  viewChanger: React.ReactNode;
}) {
  // Lazy load to avoid circular deps
  const { GraphView } = require('@/src/views/GraphView');
  return (
    <View style={{ flex: 1 }}>
      <GraphView client={client} onBack={onBack} />
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
    backgroundColor: COLORS.white,
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
