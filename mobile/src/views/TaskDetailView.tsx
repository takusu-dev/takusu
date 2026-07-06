// TaskDetailView — view and edit a single task
// Elements from top to bottom:
//   title, status, time -> time (if not pending), parallel task, cost (avg, sigma),
//   abandonability (5-step slider), habit (if generated from habit),
//   description, parallel config, deps graph (related only)

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';
import {
  Button,
  Checkbox,
  IconButton,
  List,
  Menu,
  Modal,
  Portal,
  TextInput as PaperTextInput,
  Divider,
} from 'react-native-paper';
import { Slider } from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { showError, logError } from '@/src/api/errors';
import { parseDepends, parseSchedule } from '@/src/api/types';
import type {
  TaskRow,
  HabitRow,
  ScheduleEntry,
  TaskStatus,
} from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import { haptic } from '@/src/components/haptics';
import { CancelConfirmButton } from '@/src/components/CancelConfirmButton';
import { formatDate } from '@/src/formatDate';
import {
  DependencyGraph,
  type GraphNode,
  type GraphEdge,
} from '@/src/components/graph/DependencyGraph';
import {
  postInProgressNotification,
  dismissInProgressNotification,
  dismissTaskNotifications,
} from '@/src/notifications';

const STATUS_LABELS: Record<TaskStatus, string> = {
  pending: '未スケジュール',
  scheduled: 'スケジュール済',
  in_progress: '進行中',
  completed: '完了',
  skipped: 'スキップ',
};

const STATUS_ICONS: Record<TaskStatus, keyof typeof Ionicons.glyphMap> = {
  pending: 'hourglass-outline',
  scheduled: 'calendar-outline',
  in_progress: 'play-circle-outline',
  completed: 'checkmark-circle-outline',
  skipped: 'play-skip-forward-outline',
};

function formatTime(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
}

export function TaskDetailView() {
  const { client, notifications } = useServer();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const { id } = useLocalSearchParams<{ id: string }>();
  const [task, setTask] = useState<TaskRow | null>(null);
  const [habit, setHabit] = useState<HabitRow | null>(null);
  const [parallelTask, setParallelTask] = useState<TaskRow | null>(null);
  const [allTasks, setAllTasks] = useState<TaskRow[]>([]);
  const [editing, setEditing] = useState(false);
  // Ref mirror of `editing` so refresh() can skip overwriting unsaved edits
  // (matching HabitDetailView's pattern).
  const editingRef = useRef(false);
  editingRef.current = editing;
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [abandonability, setAbandonability] = useState(0.5);
  const [avgMinutes, setAvgMinutes] = useState('');
  const [sigmaMinutes, setSigmaMinutes] = useState('');
  const [startAt, setStartAt] = useState<Date | null>(null);
  const [endAt, setEndAt] = useState<Date | null>(null);
  const [parallelizable, setParallelizable] = useState(false);
  const [allowsParallel, setAllowsParallel] = useState(false);
  const [fixed, setFixed] = useState(false);
  const [deps, setDeps] = useState<string[]>([]);
  const [pickerField, setPickerField] = useState<'start' | 'end' | null>(null);
  const [statusMenuVisible, setStatusMenuVisible] = useState(false);
  const [depModalVisible, setDepModalVisible] = useState(false);
  const [depSearch, setDepSearch] = useState('');
  const [status, setStatus] = useState<TaskStatus>('pending');
  const [menuVisible, setMenuVisible] = useState(false);
  // Double-tap detection ref — must be before the early return to satisfy
  // React's Rules of Hooks (hooks must be called unconditionally).
  const lastTapRef = useRef(0);
  const lastSectionRef = useRef('');

  const refresh = useCallback(async () => {
    if (!client || !id) return;
    let t: TaskRow;
    try {
      t = await client.getTask(id);
    } catch (e) {
      showError(e, 'タスクの取得に失敗');
      return;
    }
    // Don't clobber the user's in-progress edits.
    if (!editingRef.current) {
      setTask(t);
      setTitle(t.title);
      setDescription(t.description ?? '');
      setAbandonability(t.abandonability);
      setAvgMinutes(String(t.avg_minutes));
      setSigmaMinutes(t.sigma_minutes > 0 ? String(t.sigma_minutes) : '');
      setStartAt(t.start_at ? new Date(t.start_at) : null);
      setEndAt(new Date(t.end_at));
      setParallelizable(t.parallelizable);
      setAllowsParallel(t.allows_parallel);
      setFixed(t.fixed);
      setDeps(parseDepends(t.depends));
      setStatus(t.status);
    }
    if (t.habit_id) {
      try {
        setHabit(await client.getHabit(t.habit_id));
      } catch (e) {
        logError('ハビット取得', e);
        setHabit(null);
      }
    }

    // Load all tasks for deps editing and parallel task lookup
    try {
      const [tasks, sched] = await Promise.all([
        client.listTasks(),
        client.getSchedule().catch((e) => {
          logError('スケジュール取得', e);
          return null;
        }),
      ]);
      setAllTasks(tasks);
      const entries: ScheduleEntry[] = sched
        ? parseSchedule(sched.schedule)
        : [];
      const myEntry = entries.find((e) => e.task_id === id);
      if (myEntry) {
        const myStart = new Date(myEntry.start_at).getTime();
        const myEnd = new Date(myEntry.end_at).getTime();
        const isReceiver = t.allows_parallel;
        const isParallelizable = t.parallelizable;
        for (const other of tasks) {
          if (other.id === id) continue;
          if (other.status === 'completed' || other.status === 'skipped')
            continue;
          const isMatch = isReceiver
            ? other.parallelizable
            : isParallelizable
              ? other.allows_parallel
              : false;
          if (!isMatch) continue;
          const otherEntry = entries.find((e) => e.task_id === other.id);
          if (!otherEntry) continue;
          const otherStart = new Date(otherEntry.start_at).getTime();
          const otherEnd = new Date(otherEntry.end_at).getTime();
          if (otherStart < myEnd && otherEnd > myStart) {
            setParallelTask(other);
            break;
          }
        }
      }
    } catch (e) {
      logError('タスク一覧取得', e);
      setParallelTask(null);
    }
  }, [client, id]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  function toISO(d: Date): string {
    return d.toISOString();
  }

  async function save() {
    if (!client || !task) return;
    const updates: Record<string, unknown> = {};
    if (title !== task.title) updates.title = title;
    if (description !== (task.description ?? ''))
      updates.description = description;
    if (abandonability !== task.abandonability)
      updates.abandonability = abandonability;
    if (avgMinutes !== String(task.avg_minutes)) {
      const v = parseInt(avgMinutes, 10);
      if (!isNaN(v) && v > 0) updates.avg_minutes = v;
    }
    if (
      sigmaMinutes !==
      (task.sigma_minutes > 0 ? String(task.sigma_minutes) : '')
    ) {
      const v = parseInt(sigmaMinutes, 10);
      if (!isNaN(v) && v >= 0) updates.sigma_minutes = v;
    }
    const prevStart = task.start_at ? new Date(task.start_at) : null;
    if (startAt?.getTime() !== prevStart?.getTime()) {
      updates.start_at = startAt ? toISO(startAt) : null;
    }
    const prevEnd = new Date(task.end_at);
    if (endAt && endAt.getTime() !== prevEnd.getTime()) {
      updates.end_at = toISO(endAt);
    }
    if (parallelizable !== task.parallelizable)
      updates.parallelizable = parallelizable;
    if (allowsParallel !== task.allows_parallel)
      updates.allows_parallel = allowsParallel;
    if (fixed !== task.fixed) updates.fixed = fixed;
    if (status !== task.status) updates.status = status;
    const prevDeps = parseDepends(task.depends);
    if (JSON.stringify(deps) !== JSON.stringify(prevDeps)) {
      updates.depends = deps;
    }

    if (Object.keys(updates).length === 0) {
      editingRef.current = false;
      setEditing(false);
      return;
    }
    const prev = { ...task };
    try {
      await client.updateTask(task.id, updates);
    } catch (e) {
      showError(e, 'タスクの保存に失敗');
      return;
    }
    undoRedo.push({
      description: `edit task: ${task.title}`,
      undo: async () => {
        await client.updateTask(task.id, {
          title: prev.title,
          description: prev.description,
          abandonability: prev.abandonability,
          avg_minutes: prev.avg_minutes,
          sigma_minutes: prev.sigma_minutes,
          start_at: prev.start_at,
          end_at: prev.end_at,
          parallelizable: prev.parallelizable,
          allows_parallel: prev.allows_parallel,
          fixed: prev.fixed,
          status: prev.status,
          depends: prevDeps,
        });
        await refresh();
      },
      redo: async () => {
        await client.updateTask(task.id, updates);
        await refresh();
      },
    });
    editingRef.current = false;
    setEditing(false);
    await refresh();
  }

  async function changeStatus(newStatus: TaskStatus) {
    if (!client || !task) return;
    const prevStatus = task.status;
    setStatus(newStatus);
    setStatusMenuVisible(false);
    if (newStatus === prevStatus) return;

    // In edit mode, only update local state — persisted on Save
    if (editing) return;

    try {
      await client.updateTask(task.id, { status: newStatus });
    } catch (e) {
      showError(e, 'ステータス変更に失敗');
      setStatus(prevStatus);
      return;
    }

    // Manage in-progress notification
    if (newStatus === 'in_progress') {
      // Dismiss any delivered start reminder notifications (#257)
      dismissTaskNotifications(task.id).catch((e) => logError('通知の消去', e));
      if (notifications.inProgress) {
        postInProgressNotification({ ...task, status: newStatus }).catch((e) =>
          logError('通知の投稿', e),
        );
      }
    } else if (prevStatus === 'in_progress') {
      dismissInProgressNotification(task.id).catch((e) =>
        logError('通知の消去', e),
      );
    }
    // Dismiss any delivered reminder notifications when the task is
    // completed or skipped (#257).
    if (newStatus === 'completed' || newStatus === 'skipped') {
      dismissTaskNotifications(task.id).catch((e) => logError('通知の消去', e));
    }

    undoRedo.push({
      description: `status → ${STATUS_LABELS[newStatus]}: ${task.title}`,
      undo: async () => {
        await client.updateTask(task.id, { status: prevStatus });
        await refresh();
      },
      redo: async () => {
        await client.updateTask(task.id, { status: newStatus });
        await refresh();
      },
    });
    await refresh();
  }

  async function revertToHabit() {
    setMenuVisible(false);
    if (!client || !task || !task.habit_id) return;
    const prev = { ...task };
    try {
      await client.updateTask(task.id, { user_edited: false });
    } catch (e) {
      showError(e, 'habitへの追従設定に失敗');
      return;
    }
    undoRedo.push({
      description: `revert to habit: ${task.title}`,
      undo: async () => {
        await client.updateTask(task.id, {
          title: prev.title,
          description: prev.description,
          avg_minutes: prev.avg_minutes,
          sigma_minutes: prev.sigma_minutes,
          start_at: prev.start_at,
          end_at: prev.end_at,
          parallelizable: prev.parallelizable,
          allows_parallel: prev.allows_parallel,
          abandonability: prev.abandonability,
          user_edited: true,
        });
        await refresh();
      },
      redo: async () => {
        await client.updateTask(task.id, { user_edited: false });
        await refresh();
      },
    });
    await refresh();
  }

  async function deleteTask() {
    setMenuVisible(false);
    if (!client || !task) return;
    let currentId = task.id;
    try {
      await client.deleteTask(currentId);
    } catch (e) {
      showError(e, 'タスクの削除に失敗');
      return;
    }
    undoRedo.push({
      description: `delete task: ${task.title}`,
      undo: async () => {
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
          habit_id: task.habit_id,
          fixed: task.fixed,
        });
        currentId = recreated.id;
        if (task.user_edited) {
          await client.updateTask(currentId, { user_edited: true });
        }
      },
      redo: async () => {
        await client.deleteTask(currentId);
      },
    });
    router.back();
  }

  // Build the connected component dependency graph for the current task.
  // Traverses both forward (deps) and reverse (dependents) transitively,
  // following edges in both directions until no new nodes are discovered.
  // Must be BEFORE the early return to satisfy Rules of Hooks.
  const { detailGraphNodes, detailGraphEdges } = useMemo(() => {
    if (!task) return { detailGraphNodes: [], detailGraphEdges: [] };
    const taskMap = new Map(allTasks.map((t) => [t.id, t]));

    // Build bidirectional adjacency: for each task, store its forward deps
    // and reverse deps (tasks that depend on it).
    const forwardAdj = new Map<string, string[]>();
    const reverseAdj = new Map<string, string[]>();
    for (const t of allTasks) {
      const tDeps = parseDepends(t.depends);
      forwardAdj.set(t.id, tDeps);
      for (const depId of tDeps) {
        const rev = reverseAdj.get(depId) ?? [];
        rev.push(t.id);
        reverseAdj.set(depId, rev);
      }
    }

    // BFS from the current task, following both forward and reverse edges.
    const visited = new Set<string>();
    const queue: string[] = [task.id];
    const edges: GraphEdge[] = [];
    while (queue.length > 0) {
      const nodeId = queue.shift()!;
      if (visited.has(nodeId)) continue;
      visited.add(nodeId);
      // Enqueue forward deps
      for (const depId of forwardAdj.get(nodeId) ?? []) {
        edges.push({ source: depId, target: nodeId });
        if (!visited.has(depId)) queue.push(depId);
      }
      // Enqueue reverse deps
      for (const revId of reverseAdj.get(nodeId) ?? []) {
        if (!visited.has(revId)) queue.push(revId);
      }
    }

    // Build nodes in visitation order
    const nodes: GraphNode[] = [];
    for (const nodeId of visited) {
      const t = taskMap.get(nodeId);
      if (!t) continue;
      const isDone = t.status === 'completed' || t.status === 'skipped';
      nodes.push({
        id: t.id,
        label: t.title,
        color: isDone ? '#aaa' : BRAND_COLOR,
        x: 0,
        y: 0,
        vx: 0,
        vy: 0,
      });
    }

    return { detailGraphNodes: nodes, detailGraphEdges: edges };
  }, [allTasks, task]);

  if (!task) {
    return (
      <View style={[styles.container, { backgroundColor: colors.white }]}>
        <Text style={[styles.loading, { color: colors.gray }]}>
          読み込み中...
        </Text>
      </View>
    );
  }

  const isPending = task.status === 'pending';
  const availableDeps = allTasks.filter(
    (t) => t.id !== task.id && !deps.includes(t.id),
  );

  // Double-tap (or single tap on a section) enters edit mode.
  function enterEdit() {
    if (!editing) {
      haptic.light();
      setEditing(true);
    }
  }
  function handleSectionTap(section: string) {
    const now = Date.now();
    if (now - lastTapRef.current < 300 && lastSectionRef.current === section) {
      enterEdit();
      lastTapRef.current = 0;
      lastSectionRef.current = '';
    } else {
      lastTapRef.current = now;
      lastSectionRef.current = section;
    }
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      {/* Top bar */}
      <View style={[styles.topBar, { paddingTop: 4 + insets.top }]}>
        <IconButton
          icon="chevron-left"
          iconColor={BRAND_COLOR}
          size={28}
          onPress={() => {
            haptic.light();
            router.back();
          }}
        />
        <View style={{ flex: 1 }} />
        {editing ? (
          <>
            <IconButton
              icon="check"
              iconColor={COLORS.white}
              containerColor={BRAND_COLOR}
              size={22}
              onPress={() => {
                haptic.medium();
                save();
              }}
            />
            <CancelConfirmButton
              onConfirm={() => {
                haptic.light();
                editingRef.current = false;
                refresh();
                setEditing(false);
              }}
            />
          </>
        ) : (
          <IconButton
            icon="pencil-outline"
            iconColor={BRAND_COLOR}
            size={22}
            onPress={() => {
              haptic.light();
              setEditing(true);
            }}
          />
        )}
        <Menu
          visible={menuVisible}
          onDismiss={() => setMenuVisible(false)}
          anchor={
            <IconButton
              icon="dots-vertical"
              iconColor={BRAND_COLOR}
              size={24}
              onPress={() => setMenuVisible(true)}
            />
          }
        >
          {task.habit_id && task.user_edited && (
            <Menu.Item
              onPress={revertToHabit}
              title="habitの設定に戻す"
              leadingIcon="restore"
            />
          )}
          <Menu.Item
            onPress={deleteTask}
            title="削除"
            leadingIcon="trash-can-outline"
          />
        </Menu>
      </View>

      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.contentContainer,
          { paddingBottom: 40 + insets.bottom },
        ]}
      >
        {/* Title */}
        {editing ? (
          <PaperTextInput
            mode="outlined"
            value={title}
            onChangeText={setTitle}
            label="タイトル"
            outlineColor={colors.separator}
            activeOutlineColor={BRAND_COLOR}
            style={styles.titleInput}
            contentStyle={{ fontSize: 20, fontWeight: '600' }}
          />
        ) : (
          <Pressable onPress={() => handleSectionTap('title')}>
            <Text style={[styles.title, { color: colors.black }]}>
              {task.title}
            </Text>
          </Pressable>
        )}

        {/* Status */}
        <View style={styles.section}>
          <Menu
            visible={statusMenuVisible}
            onDismiss={() => setStatusMenuVisible(false)}
            anchor={
              <Pressable
                style={[styles.statusRow, { borderColor: colors.separator }]}
                onPress={() => {
                  haptic.light();
                  setStatusMenuVisible(true);
                }}
              >
                <Ionicons
                  name={STATUS_ICONS[editing ? status : task.status]}
                  size={20}
                  color={BRAND_COLOR}
                />
                <Text style={[styles.statusText, { color: colors.black }]}>
                  {STATUS_LABELS[editing ? status : task.status]}
                </Text>
                <Ionicons name="chevron-down" size={16} color={colors.gray} />
              </Pressable>
            }
          >
            {(Object.keys(STATUS_LABELS) as TaskStatus[])
              .filter((s) => {
                // pending (未スケジュール) のタスクは done/skip のみ変更可能。
                // scheduled/in_progress への手動変更はスケジューラの役割。
                if (task.status === 'pending') {
                  return s === 'completed' || s === 'skipped';
                }
                return true;
              })
              .map((s) => (
                <Menu.Item
                  key={s}
                  onPress={() => {
                    haptic.medium();
                    changeStatus(s);
                  }}
                  title={STATUS_LABELS[s]}
                  leadingIcon={STATUS_ICONS[s] as string}
                />
              ))}
          </Menu>
        </View>

        {/* Time */}
        {!isPending && (
          <View style={styles.section}>
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>
              時間
            </Text>
            {editing ? (
              <View style={styles.timeEditContainer}>
                <Pressable
                  style={[styles.dateField, { borderColor: colors.separator }]}
                  onPress={() => {
                    haptic.select();
                    setPickerField('start');
                  }}
                >
                  <Ionicons
                    name="calendar-outline"
                    size={18}
                    color={BRAND_COLOR}
                  />
                  <Text
                    style={[
                      styles.dateText,
                      { color: startAt ? colors.black : colors.grayLight },
                    ]}
                  >
                    {formatDate(startAt)}
                  </Text>
                  {startAt && (
                    <Pressable
                      onPress={() => {
                        haptic.light();
                        setStartAt(null);
                      }}
                    >
                      <Ionicons
                        name="close-circle"
                        size={16}
                        color={colors.grayLight}
                      />
                    </Pressable>
                  )}
                </Pressable>
                <Pressable
                  style={[styles.dateField, { borderColor: colors.separator }]}
                  onPress={() => {
                    haptic.select();
                    setPickerField('end');
                  }}
                >
                  <Ionicons
                    name="calendar-outline"
                    size={18}
                    color={BRAND_COLOR}
                  />
                  <Text
                    style={[
                      styles.dateText,
                      { color: endAt ? colors.black : colors.grayLight },
                    ]}
                  >
                    {formatDate(endAt)}
                  </Text>
                </Pressable>
              </View>
            ) : (
              <Pressable onPress={() => handleSectionTap('time')}>
                <Text style={[styles.timeText, { color: colors.gray }]}>
                  {formatTime(task.start_at)} → {formatTime(task.end_at)}
                </Text>
              </Pressable>
            )}
          </View>
        )}

        {/* Parallel task */}
        {(task.allows_parallel || task.parallelizable) && (
          <View style={styles.section}>
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>
              並列タスク
            </Text>
            {parallelTask ? (
              <Pressable
                onPress={() => {
                  haptic.light();
                  router.push(`/task/${parallelTask.id}`);
                }}
              >
                <Text style={styles.habitLink}>{parallelTask.title} ›</Text>
              </Pressable>
            ) : (
              <Text style={[styles.sectionValue, { color: colors.black }]}>
                {task.allows_parallel
                  ? '受け皿タスク (重なるタスクなし)'
                  : 'なし'}
              </Text>
            )}
          </View>
        )}

        {/* Cost */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>
            コスト
          </Text>
          {editing ? (
            <View style={styles.costEditContainer}>
              <View style={styles.avgInputContainer}>
                <PaperTextInput
                  mode="outlined"
                  label="avg (分)"
                  value={avgMinutes}
                  onChangeText={setAvgMinutes}
                  keyboardType="numeric"
                  outlineColor={colors.separator}
                  activeOutlineColor={BRAND_COLOR}
                  style={styles.costInput}
                  dense
                />
                <IconButton
                  icon="arrow-expand"
                  size={18}
                  iconColor={BRAND_COLOR}
                  disabled={!startAt || !endAt}
                  onPress={() => {
                    if (!startAt || !endAt) return;
                    haptic.light();
                    const diffMin = Math.max(
                      1,
                      Math.round((endAt.getTime() - startAt.getTime()) / 60000),
                    );
                    setAvgMinutes(String(diffMin));
                  }}
                />
              </View>
              <View style={styles.costInput}>
                <PaperTextInput
                  mode="outlined"
                  label="sigma (分)"
                  value={sigmaMinutes}
                  onChangeText={setSigmaMinutes}
                  keyboardType="numeric"
                  outlineColor={colors.separator}
                  activeOutlineColor={BRAND_COLOR}
                  dense
                />
                {sigmaMinutes === '' && (
                  <Text style={[styles.costHint, { color: colors.grayLight }]}>
                    {task.sigma_minutes}m
                  </Text>
                )}
              </View>
            </View>
          ) : (
            <Pressable onPress={() => handleSectionTap('cost')}>
              <Text style={[styles.sectionValue, { color: colors.black }]}>
                avg: {task.avg_minutes}m, sigma:{' '}
                {task.sigma_minutes > 0 ? (
                  `${task.sigma_minutes}m`
                ) : (
                  <Text style={{ color: colors.grayLight }}>0m</Text>
                )}
              </Text>
            </Pressable>
          )}
        </View>

        {/* Abandonability */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>
            abandonability
          </Text>
          {editing ? (
            <View style={styles.sliderContainer}>
              <Slider
                value={abandonability}
                onValueChange={setAbandonability}
                minimumValue={0}
                maximumValue={1}
                step={0.25}
                minimumTrackTintColor={BRAND_COLOR}
                style={styles.slider}
              />
              <Text style={[styles.sliderValue, { color: BRAND_COLOR }]}>
                {abandonability.toFixed(2)}
              </Text>
            </View>
          ) : (
            <Pressable onPress={() => handleSectionTap('abandonability')}>
              <Text style={[styles.sectionValue, { color: colors.black }]}>
                {task.abandonability.toFixed(2)}
              </Text>
            </Pressable>
          )}
        </View>

        {/* Habit */}
        {habit && (
          <Pressable
            style={styles.section}
            onPress={() => {
              haptic.light();
              router.push(`/habit/${habit.id}`);
            }}
          >
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>
              Habit
            </Text>
            <Text style={styles.habitLink}>{habit.title} ›</Text>
          </Pressable>
        )}

        {/* Description */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>
            説明
          </Text>
          {editing ? (
            <PaperTextInput
              mode="outlined"
              value={description}
              onChangeText={setDescription}
              multiline
              numberOfLines={4}
              outlineColor={colors.separator}
              activeOutlineColor={BRAND_COLOR}
              style={styles.descriptionInput}
            />
          ) : (
            <Pressable onPress={() => handleSectionTap('description')}>
              <Text style={[styles.sectionValue, { color: colors.black }]}>
                {task.description || '(なし)'}
              </Text>
            </Pressable>
          )}
        </View>

        {/* Parallel config */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>
            並列設定
          </Text>
          {editing ? (
            <View style={styles.toggleRow}>
              <Pressable
                style={styles.toggleItem}
                onPress={() => {
                  haptic.select();
                  setParallelizable(!parallelizable);
                }}
              >
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列実行可能
                </Text>
                <Checkbox
                  status={parallelizable ? 'checked' : 'unchecked'}
                  onPress={() => {
                    haptic.select();
                    setParallelizable(!parallelizable);
                  }}
                  color={BRAND_COLOR}
                />
              </Pressable>
              <Pressable
                style={styles.toggleItem}
                onPress={() => {
                  haptic.select();
                  setAllowsParallel(!allowsParallel);
                }}
              >
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列受け入れ
                </Text>
                <Checkbox
                  status={allowsParallel ? 'checked' : 'unchecked'}
                  onPress={() => {
                    haptic.select();
                    setAllowsParallel(!allowsParallel);
                  }}
                  color={BRAND_COLOR}
                />
              </Pressable>
            </View>
          ) : (
            <Pressable
              style={styles.toggleRow}
              onPress={() => handleSectionTap('parallel')}
            >
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列実行可能
                </Text>
                <Checkbox
                  status={task.parallelizable ? 'checked' : 'unchecked'}
                  disabled
                  color={BRAND_COLOR}
                />
              </View>
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列受け入れ
                </Text>
                <Checkbox
                  status={task.allows_parallel ? 'checked' : 'unchecked'}
                  disabled
                  color={BRAND_COLOR}
                />
              </View>
            </Pressable>
          )}
        </View>

        {/* Fixed */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>
            時間固定
          </Text>
          {editing ? (
            <View style={styles.toggleItem}>
              <Checkbox
                status={fixed ? 'checked' : 'unchecked'}
                onPress={() => {
                  haptic.select();
                  setFixed(!fixed);
                }}
                color={BRAND_COLOR}
              />
              <Text style={[styles.hint, { color: colors.grayLight }]}>
                開始時刻を固定し、スケジューラの移動を許可しない
              </Text>
            </View>
          ) : (
            <Pressable
              style={styles.toggleItem}
              onPress={() => handleSectionTap('fixed')}
            >
              <Checkbox
                status={task.fixed ? 'checked' : 'unchecked'}
                disabled
                color={BRAND_COLOR}
              />
            </Pressable>
          )}
        </View>

        {/* Deps — editable list + mini graph */}
        <View style={styles.section}>
          <View style={styles.depHeader}>
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>
              依存 ({deps.length})
            </Text>
            {editing && (
              <Button
                mode="text"
                compact
                onPress={() => {
                  haptic.light();
                  setDepSearch('');
                  setDepModalVisible(true);
                }}
                textColor={BRAND_COLOR}
              >
                + 追加
              </Button>
            )}
          </View>
          {deps.length > 0 ? (
            deps.map((depId) => {
              const depTask = allTasks.find((t) => t.id === depId);
              return (
                <View key={depId} style={styles.depRow}>
                  <Pressable
                    style={{ flex: 1 }}
                    onPress={() => {
                      if (!editing) {
                        haptic.light();
                        router.push(`/task/${depId}`);
                      }
                    }}
                  >
                    <Text style={styles.depLink}>
                      •{' '}
                      {depTask
                        ? `#${depTask.display_id} ${depTask.title}`
                        : depId.slice(0, 8) + '...'}{' '}
                      ›
                    </Text>
                  </Pressable>
                  {editing && (
                    <IconButton
                      icon="close"
                      size={18}
                      iconColor={COLORS.red}
                      onPress={() => {
                        haptic.light();
                        setDeps(deps.filter((d) => d !== depId));
                      }}
                    />
                  )}
                </View>
              );
            })
          ) : (
            <Text style={[styles.sectionValue, { color: colors.black }]}>
              (なし)
            </Text>
          )}

          {/* Dependency graph: connected component around this task */}
          {detailGraphNodes.length > 1 && (
            <View
              style={[styles.miniGraph, { borderTopColor: colors.separator }]}
            >
              <Text style={[styles.miniGraphLabel, { color: colors.gray }]}>
                依存グラフ
              </Text>
              <DependencyGraph
                nodes={detailGraphNodes}
                edges={detailGraphEdges}
                highlightTaskId={task.id}
                height={240}
                onTapNode={(tappedId) => {
                  if (!editing) {
                    haptic.light();
                    router.push(`/task/${tappedId}`);
                  }
                }}
              />
            </View>
          )}
        </View>
      </ScrollView>

      {/* Big save button — visible only in edit mode */}
      {editing && (
        <View
          style={[
            styles.saveBar,
            {
              paddingBottom: 8 + insets.bottom,
              backgroundColor: colors.white,
              borderTopColor: colors.separator,
            },
          ]}
        >
          <Pressable
            style={[styles.saveBarButton, { backgroundColor: BRAND_COLOR }]}
            onPress={() => {
              haptic.medium();
              save();
            }}
          >
            <Ionicons name="checkmark-circle" size={22} color={COLORS.white} />
            <Text style={styles.saveBarText}>保存</Text>
          </Pressable>
        </View>
      )}

      {/* DateTime Picker Modals */}
      <DateTimePickerModal
        visible={pickerField === 'start'}
        value={startAt}
        mode="datetime"
        label="開始日時"
        optional
        onConfirm={(date) => {
          setStartAt(date);
          setPickerField(null);
        }}
        onCancel={() => setPickerField(null)}
      />
      <DateTimePickerModal
        visible={pickerField === 'end'}
        value={endAt}
        mode="datetime"
        label="期限日時"
        minimumDate={startAt ?? undefined}
        shortcuts={[
          {
            label: '1時間後',
            compute: () => new Date(Date.now() + 60 * 60 * 1000),
          },
          {
            label: '今日23:59',
            compute: () => {
              const d = new Date();
              d.setHours(23, 59, 0, 0);
              return d;
            },
          },
          {
            label: '明日',
            compute: () => {
              const d = new Date();
              d.setDate(d.getDate() + 1);
              d.setHours(23, 59, 0, 0);
              return d;
            },
          },
          {
            label: '明後日',
            compute: () => {
              const d = new Date();
              d.setDate(d.getDate() + 2);
              d.setHours(23, 59, 0, 0);
              return d;
            },
          },
          {
            label: '1週間後',
            compute: () => {
              const d = new Date();
              d.setDate(d.getDate() + 7);
              d.setHours(23, 59, 0, 0);
              return d;
            },
          },
        ]}
        onConfirm={(date) => {
          setEndAt(date);
          setPickerField(null);
        }}
        onCancel={() => setPickerField(null)}
      />

      {/* Dep selection modal (Paper) */}
      <Portal>
        <Modal
          visible={depModalVisible}
          onDismiss={() => setDepModalVisible(false)}
          contentContainerStyle={[
            styles.depModal,
            { backgroundColor: colors.white },
          ]}
        >
          <Text style={[styles.depModalTitle, { color: colors.black }]}>
            依存先を選択
          </Text>
          <View
            style={[
              styles.depModalSearch,
              { borderBottomColor: colors.separator },
            ]}
          >
            <Ionicons name="search" size={18} color={colors.gray} />
            <PaperTextInput
              mode="outlined"
              value={depSearch}
              onChangeText={setDepSearch}
              placeholder="タイトルで検索"
              placeholderTextColor={colors.grayLight}
              outlineColor={colors.separator}
              activeOutlineColor={BRAND_COLOR}
              style={styles.depModalSearchInput}
              dense
              autoFocus
            />
            {depSearch.length > 0 && (
              <Pressable
                onPress={() => {
                  haptic.light();
                  setDepSearch('');
                }}
              >
                <Ionicons
                  name="close-circle"
                  size={18}
                  color={colors.grayLight}
                />
              </Pressable>
            )}
          </View>
          <ScrollView style={styles.depModalList}>
            {availableDeps.length === 0 ? (
              <Text style={[styles.depModalEmpty, { color: colors.gray }]}>
                追加可能なタスクがありません
              </Text>
            ) : (
              availableDeps
                .filter((t) =>
                  depSearch.length === 0
                    ? true
                    : t.title.toLowerCase().includes(depSearch.toLowerCase()),
                )
                .map((t) => (
                  <List.Item
                    key={t.id}
                    title={t.title}
                    description={`#${t.display_id}${t.status !== 'pending' ? ' · ' + STATUS_LABELS[t.status] : ''}`}
                    onPress={() => {
                      haptic.medium();
                      setDeps([...deps, t.id]);
                      setDepModalVisible(false);
                    }}
                    left={() => (
                      <List.Icon
                        icon={STATUS_ICONS[t.status] as string}
                        color={BRAND_COLOR}
                      />
                    )}
                  />
                ))
            )}
          </ScrollView>
          <Divider />
          <Button
            mode="text"
            onPress={() => {
              haptic.light();
              setDepModalVisible(false);
            }}
            textColor={BRAND_COLOR}
            style={styles.depModalClose}
          >
            閉じる
          </Button>
        </Modal>
      </Portal>
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
    paddingHorizontal: 4,
    paddingBottom: 4,
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
  },
  title: {
    fontSize: 24,
    fontWeight: '600',
  },
  titleInput: {
    fontSize: 20,
  },
  statusRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
  },
  statusText: {
    flex: 1,
    fontSize: 15,
    fontWeight: '500',
  },
  timeText: {
    fontSize: 14,
  },
  section: {
    gap: 4,
  },
  sectionLabel: {
    fontSize: 13,
    fontWeight: '500',
  },
  hint: {
    fontSize: 11,
    marginTop: 2,
    flex: 1,
  },
  sectionValue: {
    fontSize: 16,
  },
  sliderContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
  },
  slider: {
    flex: 1,
  },
  sliderValue: {
    fontSize: 14,
    fontVariant: ['tabular-nums'],
  },
  habitLink: {
    fontSize: 16,
    color: BRAND_COLOR,
  },
  descriptionInput: {
    minHeight: 80,
  },
  depLink: {
    fontSize: 14,
    color: BRAND_COLOR,
    paddingVertical: 4,
  },
  depRow: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  timeEditContainer: {
    gap: 8,
  },
  dateField: {
    flexDirection: 'row',
    alignItems: 'center',
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    gap: 8,
  },
  dateText: {
    flex: 1,
    fontSize: 15,
  },
  costEditContainer: {
    flexDirection: 'row',
    gap: 12,
  },
  avgInputContainer: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
  },
  costInput: {
    flex: 1,
  },
  costHint: {
    fontSize: 11,
    marginTop: 2,
    marginLeft: 4,
  },
  toggleRow: {
    flexDirection: 'row',
    gap: 24,
  },
  toggleItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  toggleLabel: {
    fontSize: 14,
  },
  miniGraph: {
    marginTop: 12,
    paddingTop: 12,
    borderTopWidth: 1,
    gap: 4,
  },
  miniGraphLabel: {
    fontSize: 12,
    marginBottom: 4,
  },
  depHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  depModal: {
    margin: 24,
    borderRadius: 12,
    padding: 16,
    maxHeight: '70%',
  },
  depModalTitle: {
    fontSize: 18,
    fontWeight: '600',
    marginBottom: 8,
  },
  depModalSearch: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    borderBottomWidth: 1,
    paddingBottom: 8,
    marginBottom: 8,
  },
  depModalSearchInput: {
    flex: 1,
    fontSize: 15,
  },
  depModalList: {
    maxHeight: 400,
  },
  depModalEmpty: {
    textAlign: 'center',
    paddingVertical: 24,
  },
  depModalClose: {
    marginTop: 8,
  },
  saveBar: {
    paddingHorizontal: 16,
    paddingTop: 8,
    borderTopWidth: 1,
    borderTopColor: '#eee',
  },
  saveBarButton: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    paddingVertical: 14,
    borderRadius: 12,
  },
  saveBarText: {
    color: COLORS.white,
    fontSize: 18,
    fontWeight: '700',
  },
});
