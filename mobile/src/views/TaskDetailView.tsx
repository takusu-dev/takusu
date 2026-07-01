// TaskDetailView — view and edit a single task
// Elements from top to bottom:
//   title, status, time -> time (if not pending), parallel task, cost (avg, sigma),
//   abandonability (5-step slider), habit (if generated from habit),
//   description, parallel config, deps graph (related only)

import { useCallback, useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';
import {
  Button,
  IconButton,
  List,
  Menu,
  Modal,
  Portal,
  Switch,
  TextInput as PaperTextInput,
  Divider,
} from 'react-native-paper';
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { showError, logError } from '@/src/api/errors';
import { parseDepends, parseSchedule } from '@/src/api/types';
import type { TaskRow, HabitRow, ScheduleEntry, TaskStatus } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import {
  postInProgressNotification,
  dismissInProgressNotification,
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
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [abandonability, setAbandonability] = useState(0.5);
  const [avgMinutes, setAvgMinutes] = useState('');
  const [sigmaMinutes, setSigmaMinutes] = useState('');
  const [startAt, setStartAt] = useState<Date | null>(null);
  const [endAt, setEndAt] = useState<Date | null>(null);
  const [parallelizable, setParallelizable] = useState(false);
  const [allowsParallel, setAllowsParallel] = useState(false);
  const [deps, setDeps] = useState<string[]>([]);
  const [pickerField, setPickerField] = useState<'start' | 'end' | null>(null);
  const [statusMenuVisible, setStatusMenuVisible] = useState(false);
  const [depModalVisible, setDepModalVisible] = useState(false);
  const [depSearch, setDepSearch] = useState('');
  const [status, setStatus] = useState<TaskStatus>('pending');

  const refresh = useCallback(async () => {
    if (!client || !id) return;
    let t: TaskRow;
    try {
      t = await client.getTask(id);
    } catch (e) {
      showError(e, 'タスクの取得に失敗');
      return;
    }
    setTask(t);
    setTitle(t.title);
    setDescription(t.description ?? '');
    setAbandonability(t.abandonability);
    setAvgMinutes(String(t.avg_minutes));
    setSigmaMinutes(String(t.sigma_minutes));
    setStartAt(t.start_at ? new Date(t.start_at) : null);
    setEndAt(new Date(t.end_at));
    setParallelizable(t.parallelizable);
    setAllowsParallel(t.allows_parallel);
    setDeps(parseDepends(t.depends));
    setStatus(t.status);
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
      const entries: ScheduleEntry[] = sched ? parseSchedule(sched.schedule) : [];
      const myEntry = entries.find((e) => e.task_id === id);
      if (myEntry) {
        const myStart = new Date(myEntry.start_at).getTime();
        const myEnd = new Date(myEntry.end_at).getTime();
        const isReceiver = t.allows_parallel;
        const isParallelizable = t.parallelizable;
        for (const other of tasks) {
          if (other.id === id) continue;
          if (other.status === 'completed' || other.status === 'skipped') continue;
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
    const tzOffset = d.getTimezoneOffset() * 60000;
    return new Date(d.getTime() - tzOffset).toISOString().slice(0, -1);
  }

  function formatDate(d: Date | null): string {
    if (!d) return '未設定';
    const dateStr = `${d.getFullYear()}/${(d.getMonth() + 1).toString().padStart(2, '0')}/${d.getDate().toString().padStart(2, '0')}`;
    const timeStr = `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
    return `${dateStr} ${timeStr}`;
  }

  async function save() {
    if (!client || !task) return;
    const updates: Record<string, unknown> = {};
    if (title !== task.title) updates.title = title;
    if (description !== (task.description ?? '')) updates.description = description;
    if (abandonability !== task.abandonability) updates.abandonability = abandonability;
    if (avgMinutes !== String(task.avg_minutes)) {
      const v = parseInt(avgMinutes, 10);
      if (!isNaN(v) && v > 0) updates.avg_minutes = v;
    }
    if (sigmaMinutes !== String(task.sigma_minutes)) {
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
    if (parallelizable !== task.parallelizable) updates.parallelizable = parallelizable;
    if (allowsParallel !== task.allows_parallel) updates.allows_parallel = allowsParallel;
    if (status !== task.status) updates.status = status;
    const prevDeps = parseDepends(task.depends);
    if (JSON.stringify(deps) !== JSON.stringify(prevDeps)) {
      updates.depends = deps;
    }

    if (Object.keys(updates).length === 0) {
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

  if (!task) {
    return (
      <View style={[styles.container, { backgroundColor: colors.white }]}>
        <Text style={[styles.loading, { color: colors.gray }]}>読み込み中...</Text>
      </View>
    );
  }

  const isPending = task.status === 'pending';
  const reverseDeps = allTasks.filter((t) => parseDepends(t.depends).includes(task.id));
  const availableDeps = allTasks.filter(
    (t) => t.id !== task.id && !deps.includes(t.id),
  );

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      {/* Top bar */}
      <View style={[styles.topBar, { paddingTop: 4 + insets.top }]}>
        <IconButton
          icon="chevron-left"
          iconColor={BRAND_COLOR}
          size={28}
          onPress={() => router.back()}
        />
        <View style={{ flex: 1 }} />
        <Button
          mode={editing ? 'contained' : 'outlined'}
          onPress={() => (editing ? save() : setEditing(true))}
          textColor={editing ? COLORS.white : BRAND_COLOR}
          buttonColor={editing ? BRAND_COLOR : undefined}
          compact
        >
          {editing ? '保存' : '編集'}
        </Button>
      </View>

      <ScrollView
        style={styles.content}
        contentContainerStyle={[styles.contentContainer, { paddingBottom: 40 + insets.bottom }]}
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
          <Text style={[styles.title, { color: colors.black }]}>{task.title}</Text>
        )}

        {/* Status */}
        <View style={styles.section}>
          <Menu
            visible={statusMenuVisible}
            onDismiss={() => setStatusMenuVisible(false)}
            anchor={
              <Pressable
                style={[styles.statusRow, { borderColor: colors.separator }]}
                onPress={() => setStatusMenuVisible(true)}
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
            {(Object.keys(STATUS_LABELS) as TaskStatus[]).map((s) => (
              <Menu.Item
                key={s}
                onPress={() => changeStatus(s)}
                title={STATUS_LABELS[s]}
                leadingIcon={STATUS_ICONS[s] as string}
              />
            ))}
          </Menu>
        </View>

        {/* Time */}
        {!isPending && (
          <View style={styles.section}>
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>時間</Text>
            {editing ? (
              <View style={styles.timeEditContainer}>
                <Pressable
                  style={[styles.dateField, { borderColor: colors.separator }]}
                  onPress={() => setPickerField('start')}
                >
                  <Ionicons name="calendar-outline" size={18} color={BRAND_COLOR} />
                  <Text style={[styles.dateText, { color: startAt ? colors.black : colors.grayLight }]}>
                    {formatDate(startAt)}
                  </Text>
                  {startAt && (
                    <Pressable onPress={() => setStartAt(null)}>
                      <Ionicons name="close-circle" size={16} color={colors.grayLight} />
                    </Pressable>
                  )}
                </Pressable>
                <Pressable
                  style={[styles.dateField, { borderColor: colors.separator }]}
                  onPress={() => setPickerField('end')}
                >
                  <Ionicons name="calendar-outline" size={18} color={BRAND_COLOR} />
                  <Text style={[styles.dateText, { color: endAt ? colors.black : colors.grayLight }]}>
                    {formatDate(endAt)}
                  </Text>
                </Pressable>
              </View>
            ) : (
              <Text style={[styles.timeText, { color: colors.gray }]}>
                {formatTime(task.start_at)} → {formatTime(task.end_at)}
              </Text>
            )}
          </View>
        )}

        {/* Parallel task */}
        {(task.allows_parallel || task.parallelizable) && (
          <View style={styles.section}>
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>並列タスク</Text>
            {parallelTask ? (
              <Pressable onPress={() => router.push(`/task/${parallelTask.id}`)}>
                <Text style={styles.habitLink}>{parallelTask.title} ›</Text>
              </Pressable>
            ) : (
              <Text style={[styles.sectionValue, { color: colors.black }]}>
                {task.allows_parallel ? '受け皿タスク (重なるタスクなし)' : 'なし'}
              </Text>
            )}
          </View>
        )}

        {/* Cost */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>コスト</Text>
          {editing ? (
            <View style={styles.costEditContainer}>
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
              <PaperTextInput
                mode="outlined"
                label="sigma (分)"
                value={sigmaMinutes}
                onChangeText={setSigmaMinutes}
                keyboardType="numeric"
                outlineColor={colors.separator}
                activeOutlineColor={BRAND_COLOR}
                style={styles.costInput}
                dense
              />
            </View>
          ) : (
            <Text style={[styles.sectionValue, { color: colors.black }]}>
              avg: {task.avg_minutes}m, sigma: {task.sigma_minutes}m
            </Text>
          )}
        </View>

        {/* Abandonability */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>abandonability</Text>
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
              <Text style={[styles.sliderValue, { color: BRAND_COLOR }]}>
                {abandonability.toFixed(2)}
              </Text>
            </View>
          ) : (
            <Text style={[styles.sectionValue, { color: colors.black }]}>
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
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>ハビット</Text>
            <Text style={styles.habitLink}>{habit.title} ›</Text>
          </Pressable>
        )}

        {/* Description */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>説明</Text>
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
            <Text style={[styles.sectionValue, { color: colors.black }]}>
              {task.description || '(なし)'}
            </Text>
          )}
        </View>

        {/* Parallel config */}
        <View style={styles.section}>
          <Text style={[styles.sectionLabel, { color: colors.gray }]}>並列設定</Text>
          {editing ? (
            <View style={styles.toggleRow}>
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>parallelizable</Text>
                <Switch
                  value={parallelizable}
                  onValueChange={setParallelizable}
                  color={BRAND_COLOR}
                />
              </View>
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>allows_parallel</Text>
                <Switch
                  value={allowsParallel}
                  onValueChange={setAllowsParallel}
                  color={BRAND_COLOR}
                />
              </View>
            </View>
          ) : (
            <Text style={[styles.sectionValue, { color: colors.black }]}>
              parallelizable: {task.parallelizable ? 'はい' : 'いいえ'}
              {'\n'}allows_parallel: {task.allows_parallel ? 'はい' : 'いいえ'}
            </Text>
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
                    onPress={() => !editing && router.push(`/task/${depId}`)}
                  >
                    <Text style={styles.depLink}>
                      • {depTask ? `#${depTask.display_id} ${depTask.title}` : depId.slice(0, 8) + '...'} ›
                    </Text>
                  </Pressable>
                  {editing && (
                    <IconButton
                      icon="close"
                      size={18}
                      iconColor={COLORS.red}
                      onPress={() => setDeps(deps.filter((d) => d !== depId))}
                    />
                  )}
                </View>
              );
            })
          ) : (
            <Text style={[styles.sectionValue, { color: colors.black }]}>(なし)</Text>
          )}

          {/* Mini deps graph: reverse deps (tasks that depend on this) */}
          {reverseDeps.length > 0 && (
            <View style={[styles.miniGraph, { borderTopColor: colors.separator }]}>
              <Text style={[styles.miniGraphLabel, { color: colors.gray }]}>
                これに依存するタスク:
              </Text>
              {reverseDeps.map((rd) => (
                <Pressable
                  key={rd.id}
                  onPress={() => !editing && router.push(`/task/${rd.id}`)}
                >
                  <Text style={styles.depLink}>← {rd.title} ›</Text>
                </Pressable>
              ))}
            </View>
          )}
        </View>
      </ScrollView>

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
          contentContainerStyle={[styles.depModal, { backgroundColor: colors.white }]}
        >
          <Text style={[styles.depModalTitle, { color: colors.black }]}>依存先を選択</Text>
          <View style={[styles.depModalSearch, { borderBottomColor: colors.separator }]}>
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
              <Pressable onPress={() => setDepSearch('')}>
                <Ionicons name="close-circle" size={18} color={colors.grayLight} />
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
                      setDeps([...deps, t.id]);
                      setDepModalVisible(false);
                    }}
                    left={() => (
                      <List.Icon icon={STATUS_ICONS[t.status] as string} color={BRAND_COLOR} />
                    )}
                  />
                ))
            )}
          </ScrollView>
          <Divider />
          <Button
            mode="text"
            onPress={() => setDepModalVisible(false)}
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
  sectionValue: {
    fontSize: 16,
  },
  sliderContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
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
  costInput: {
    flex: 1,
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
});
