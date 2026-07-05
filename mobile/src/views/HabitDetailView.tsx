// HabitDetailView — view and edit a habit + recent generated tasks

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  Pressable,
  Alert,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';
import {
  Checkbox,
  IconButton,
  Menu,
  TextInput as PaperTextInput,
} from 'react-native-paper';
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { showError, logError } from '@/src/api/errors';
import { parseDepends } from '@/src/api/types';
import type { HabitRow, TaskRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { RruleBuilderModal } from '@/src/components/RruleBuilderModal';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import { parseRule, summarizeRule } from '@/src/api/rrule';
import { haptic } from '@/src/components/haptics';
import { CancelConfirmButton } from '@/src/components/CancelConfirmButton';

export function HabitDetailView() {
  const { client } = useServer();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const { id } = useLocalSearchParams<{ id: string }>();
  const [habit, setHabit] = useState<HabitRow | null>(null);
  const [tasks, setTasks] = useState<TaskRow[]>([]);

  // edit state
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [recurrence, setRecurrence] = useState('');
  const [showRruleBuilder, setShowRruleBuilder] = useState(false);
  const [startTime, setStartTime] = useState('09:00');
  const [endTime, setEndTime] = useState('10:00');
  const [avgMinutes, setAvgMinutes] = useState('60');
  const [sigmaMinutes, setSigmaMinutes] = useState('0');
  const [abandonability, setAbandonability] = useState(0.5);
  const [parallelizable, setParallelizable] = useState(false);
  const [allowsParallel, setAllowsParallel] = useState(false);
  const [fixed, setFixed] = useState(false);
  const [active, setActive] = useState(true);
  const [saving, setSaving] = useState(false);
  const [menuVisible, setMenuVisible] = useState(false);
  const [pickerField, setPickerField] = useState<'start' | 'end' | null>(null);
  // Ref mirror of `editing` so refresh() can skip overwriting unsaved edits
  // when called from menu actions (toggleActive) while editing.
  const editingRef = useRef(false);
  editingRef.current = editing;

  // "HH:MM" → Date (today at that time)
  function timeStringToDate(s: string): Date {
    const [h, m] = s.split(':').map((n) => parseInt(n, 10) || 0);
    const d = new Date();
    d.setHours(h, m, 0, 0);
    return d;
  }

  // Date → "HH:MM"
  function dateToTimeString(d: Date): string {
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
  }

  const refresh = useCallback(async () => {
    if (!client || !id) return;
    try {
      const h = await client.getHabit(id);
      setHabit(h);
      // Don't clobber the user's in-progress edits.
      if (!editingRef.current) {
        setTitle(h.title);
        setDescription(h.description ?? '');
        setRecurrence(h.recurrence);
        setStartTime(h.start_time);
        setEndTime(h.end_time);
        setAvgMinutes(String(h.avg_minutes));
        setSigmaMinutes(h.sigma_minutes > 0 ? String(h.sigma_minutes) : '');
        setAbandonability(h.abandonability);
        setParallelizable(h.parallelizable);
        setAllowsParallel(h.allows_parallel);
        setActive(h.active);
        setFixed(h.fixed);
      }
    } catch (e) {
      showError(e, 'Habitの取得に失敗');
      return;
    }
    try {
      const allTasks = await client.listTasks({ habit_id: id });
      // Show upcoming tasks in chronological order.
      // Server returns tasks ordered by created_at DESC (generation order),
      // not by date. Sort by start_at ascending so the user sees the earliest
      // upcoming task first. Exclude completed/skipped tasks so past finished
      // habit occurrences don't push upcoming ones out of the top 10.
      const sorted = [...allTasks]
        .filter((t) => t.status !== 'completed' && t.status !== 'skipped')
        .sort((a, b) => (a.start_at ?? '').localeCompare(b.start_at ?? ''))
        .slice(0, 10);
      setTasks(sorted);
    } catch (e) {
      logError('ハビットのタスク取得', e);
      setTasks([]);
    }
  }, [client, id]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function save() {
    if (!client || !habit || saving) return;
    const updates: Record<string, unknown> = {};
    if (title !== habit.title) updates.title = title;
    if (description !== (habit.description ?? ''))
      updates.description = description;
    if (recurrence !== habit.recurrence) updates.recurrence = recurrence;
    if (startTime !== habit.start_time) updates.start_time = startTime;
    if (endTime !== habit.end_time) updates.end_time = endTime;
    if (avgMinutes !== String(habit.avg_minutes)) {
      const v = parseInt(avgMinutes, 10);
      if (!isNaN(v) && v > 0) updates.avg_minutes = v;
    }
    if (
      sigmaMinutes !==
      (habit.sigma_minutes > 0 ? String(habit.sigma_minutes) : '')
    ) {
      const v = parseInt(sigmaMinutes, 10);
      if (!isNaN(v) && v >= 0) updates.sigma_minutes = v;
    }
    if (abandonability !== habit.abandonability)
      updates.abandonability = abandonability;
    if (parallelizable !== habit.parallelizable)
      updates.parallelizable = parallelizable;
    if (allowsParallel !== habit.allows_parallel)
      updates.allows_parallel = allowsParallel;
    if (active !== habit.active) updates.active = active;
    if (fixed !== habit.fixed) updates.fixed = fixed;

    if (Object.keys(updates).length === 0) {
      setEditing(false);
      return;
    }
    const prev = { ...habit };
    setSaving(true);
    try {
      await client.updateHabit(habit.id, updates);
    } catch (e) {
      showError(e, 'ハビットの保存に失敗');
      setSaving(false);
      return;
    }
    undoRedo.push({
      description: `edit habit: ${habit.title}`,
      undo: async () => {
        await client.updateHabit(habit.id, {
          title: prev.title,
          description: prev.description,
          recurrence: prev.recurrence,
          start_time: prev.start_time,
          end_time: prev.end_time,
          avg_minutes: prev.avg_minutes,
          sigma_minutes: prev.sigma_minutes,
          abandonability: prev.abandonability,
          parallelizable: prev.parallelizable,
          allows_parallel: prev.allows_parallel,
          active: prev.active,
          fixed: prev.fixed,
        });
        await refresh();
      },
      redo: async () => {
        await client.updateHabit(habit.id, updates);
        await refresh();
      },
    });
    setSaving(false);
    setEditing(false);
    await refresh();
  }

  async function deleteHabit() {
    setMenuVisible(false);
    if (!client || !habit) return;
    // #240: confirm before deleting, showing how many associated
    // tasks will also be cascade-deleted. Fetch the task list first
    // so the confirmation is accurate and undo can restore them.
    let deletedTasks: TaskRow[];
    try {
      deletedTasks = await client.listTasks({ habit_id: habit.id });
    } catch (e) {
      showError(e, 'ハビットのタスク取得に失敗');
      return;
    }
    const taskCount = deletedTasks.length;
    const message =
      taskCount > 0
        ? `このハビットと関連する${taskCount}件のタスクも削除されます。よろしいですか？`
        : 'このハビットを削除しますか？';
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
    const prev = { ...habit };
    // Track the current habit id: undo recreates the habit with a new id,
    // and redo must delete that new id (not the stale original).
    let currentId = habit.id;
    try {
      await client.deleteHabit(currentId);
    } catch (e) {
      showError(e, 'ハビットの削除に失敗');
      return;
    }
    // Track recreated task ids so redo deletes them, and so a retry
    // after partial failure doesn't create duplicates.
    const currentTaskIds: string[] = [...deletedTasks.map((t) => t.id)];
    const taskCreatedIdx = new Set<number>();
    // Guard habit creation so a retry after partial failure doesn't
    // create a duplicate habit (mirrors HabitView's createdIdx).
    let habitCreated = false;
    undoRedo.push({
      description:
        taskCount > 0
          ? `delete habit + ${taskCount} tasks: ${habit.title}`
          : `delete habit: ${habit.title}`,
      undo: async () => {
        if (!habitCreated) {
          const recreated = await client.createHabit({
            title: prev.title,
            description: prev.description,
            recurrence: prev.recurrence,
            start_time: prev.start_time,
            end_time: prev.end_time,
            avg_minutes: prev.avg_minutes,
            sigma_minutes: prev.sigma_minutes,
            parallelizable: prev.parallelizable,
            allows_parallel: prev.allows_parallel,
            abandonability: prev.abandonability,
            fixed: prev.fixed,
          });
          // CreateHabit does not accept `active`; restore it via update.
          if (!prev.active) {
            await client.updateHabit(recreated.id, { active: prev.active });
          }
          currentId = recreated.id;
          habitCreated = true;
        }
        // Restore the cascade-deleted tasks, pointing them at the
        // recreated habit's new id. Two-pass: first create with no
        // deps (so server doesn't reject references to not-yet-existing
        // tasks), then remap deps to new ids — mirrors HomeView's
        // batch-delete undo pattern.
        const oldToNew = new Map<string, string>();
        for (let i = 0; i < deletedTasks.length; i++) {
          if (taskCreatedIdx.has(i)) {
            oldToNew.set(deletedTasks[i].id, currentTaskIds[i]);
            continue;
          }
          const t = deletedTasks[i];
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
            habit_id: currentId,
            fixed: t.fixed,
          });
          if (t.status !== 'pending') {
            await client.updateTask(recreatedTask.id, { status: t.status });
          }
          currentTaskIds[i] = recreatedTask.id;
          oldToNew.set(t.id, recreatedTask.id);
          taskCreatedIdx.add(i);
        }
        // Second pass: remap depends to new IDs for deps within the
        // deleted set.
        for (let i = 0; i < deletedTasks.length; i++) {
          const t = deletedTasks[i];
          const origDeps = parseDepends(t.depends);
          if (origDeps.length === 0) continue;
          const newId = oldToNew.get(t.id)!;
          const remapped = origDeps.map((d) => oldToNew.get(d) ?? d);
          await client.updateTask(newId, { depends: remapped });
        }
      },
      redo: async () => {
        habitCreated = false;
        taskCreatedIdx.clear();
        await client.deleteHabit(currentId);
      },
    });
    router.back();
  }

  async function toggleActive() {
    setMenuVisible(false);
    if (!client || !habit) return;
    const next = !habit.active;
    const prev = habit.active;
    try {
      await client.updateHabit(habit.id, { active: next });
    } catch (e) {
      showError(e, 'アクティブ状態の変更に失敗');
      return;
    }
    undoRedo.push({
      description: `${next ? '有効化' : '無効化'} habit: ${habit.title}`,
      undo: async () => {
        await client.updateHabit(habit.id, { active: prev });
        await refresh();
      },
      redo: async () => {
        await client.updateHabit(habit.id, { active: next });
        await refresh();
      },
    });
    await refresh();
  }

  if (!habit) {
    return (
      <View style={[styles.container, { backgroundColor: colors.white }]}>
        <Text style={[styles.loading, { color: colors.gray }]}>
          読み込み中...
        </Text>
      </View>
    );
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
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
        {editing && (
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
                setEditing(false);
                refresh();
              }}
            />
          </>
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
          {editing ? (
            <>
              <Menu.Item
                onPress={() => {
                  setMenuVisible(false);
                  save();
                }}
                title="保存"
                leadingIcon="content-save-outline"
              />
              <Menu.Item
                onPress={() => {
                  setMenuVisible(false);
                  editingRef.current = false;
                  setEditing(false);
                  refresh();
                }}
                title="キャンセル"
                leadingIcon="close"
              />
            </>
          ) : (
            <>
              <Menu.Item
                onPress={() => {
                  setMenuVisible(false);
                  setEditing(true);
                }}
                title="編集"
                leadingIcon="pencil-outline"
              />
              <Menu.Item
                onPress={toggleActive}
                title={habit.active ? '無効化' : '有効化'}
                leadingIcon={
                  habit.active ? 'pause-circle-outline' : 'play-circle-outline'
                }
              />
              <Menu.Item
                onPress={deleteHabit}
                title="削除"
                leadingIcon="trash-can-outline"
              />
            </>
          )}
        </Menu>
      </View>

      <ScrollView
        contentContainerStyle={[
          styles.content,
          { paddingBottom: 16 + insets.bottom },
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
          <Text style={[styles.title, { color: colors.black }]}>
            {habit.title}
          </Text>
        )}

        {/* Description */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>説明</Text>
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
            <Text style={[styles.value, { color: colors.black }]}>
              {habit.description || '(なし)'}
            </Text>
          )}
        </View>

        {/* Recurrence */}
        <View style={styles.section}>
          <View style={styles.rruleHeader}>
            <Text style={[styles.label, { color: colors.gray }]}>
              周期 (RRULE)
            </Text>
            {editing && (
              <Pressable
                style={styles.helpButton}
                onPress={() => setShowRruleBuilder(true)}
                hitSlop={8}
              >
                <Ionicons
                  name="help-circle-outline"
                  size={18}
                  color={BRAND_COLOR}
                />
              </Pressable>
            )}
          </View>
          {editing ? (
            <Pressable
              style={[
                styles.dateField,
                {
                  borderColor: colors.separator,
                  backgroundColor: colors.white,
                },
              ]}
              onPress={() => setShowRruleBuilder(true)}
            >
              <Ionicons name="repeat" size={20} color={BRAND_COLOR} />
              <Text
                style={[styles.dateText, { color: colors.black }]}
                numberOfLines={2}
              >
                {summarizeRule(parseRule(recurrence))}
              </Text>
              <Ionicons
                name="chevron-forward"
                size={18}
                color={colors.grayLight}
              />
            </Pressable>
          ) : (
            <Text style={[styles.value, { color: colors.black }]}>
              {summarizeRule(parseRule(habit.recurrence))}
            </Text>
          )}
        </View>

        {/* Time */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>時間</Text>
          {editing ? (
            <View style={styles.row}>
              <Pressable
                style={[
                  styles.timeField,
                  {
                    borderColor: colors.separator,
                    backgroundColor: colors.white,
                  },
                ]}
                onPress={() => {
                  haptic.select();
                  setPickerField('start');
                }}
              >
                <Text style={[styles.timeFieldLabel, { color: colors.gray }]}>
                  開始
                </Text>
                <Text style={[styles.timeFieldValue, { color: colors.black }]}>
                  {startTime}
                </Text>
              </Pressable>
              <Pressable
                style={[
                  styles.timeField,
                  {
                    borderColor: colors.separator,
                    backgroundColor: colors.white,
                  },
                ]}
                onPress={() => {
                  haptic.select();
                  setPickerField('end');
                }}
              >
                <Text style={[styles.timeFieldLabel, { color: colors.gray }]}>
                  終了
                </Text>
                <Text style={[styles.timeFieldValue, { color: colors.black }]}>
                  {endTime}
                </Text>
              </Pressable>
            </View>
          ) : (
            <Text style={[styles.value, { color: colors.black }]}>
              {habit.start_time} → {habit.end_time}
            </Text>
          )}
        </View>

        {/* Cost */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>コスト</Text>
          {editing ? (
            <View style={styles.row}>
              <PaperTextInput
                mode="outlined"
                label="avg (分)"
                value={avgMinutes}
                onChangeText={setAvgMinutes}
                keyboardType="numeric"
                outlineColor={colors.separator}
                activeOutlineColor={BRAND_COLOR}
                style={[styles.costInput, { flex: 1 }]}
                dense
              />
              <View style={[styles.costInput, { flex: 1 }]}>
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
                    {habit.sigma_minutes}m
                  </Text>
                )}
              </View>
            </View>
          ) : (
            <Text style={[styles.value, { color: colors.black }]}>
              avg: {habit.avg_minutes}m, sigma:{' '}
              {habit.sigma_minutes > 0 ? (
                `${habit.sigma_minutes}m`
              ) : (
                <Text style={{ color: colors.grayLight }}>0m</Text>
              )}
            </Text>
          )}
        </View>

        {/* Abandonability */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>
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
            <Text style={[styles.value, { color: colors.black }]}>
              {habit.abandonability.toFixed(2)}
            </Text>
          )}
        </View>

        {/* Parallel config */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>並列設定</Text>
          {editing ? (
            <View style={styles.toggleRow}>
              <Pressable
                style={styles.toggleItem}
                onPress={() => setParallelizable(!parallelizable)}
              >
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列実行可能
                </Text>
                <Checkbox
                  status={parallelizable ? 'checked' : 'unchecked'}
                  onPress={() => setParallelizable(!parallelizable)}
                  color={BRAND_COLOR}
                />
              </Pressable>
              <Pressable
                style={styles.toggleItem}
                onPress={() => setAllowsParallel(!allowsParallel)}
              >
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列受け入れ
                </Text>
                <Checkbox
                  status={allowsParallel ? 'checked' : 'unchecked'}
                  onPress={() => setAllowsParallel(!allowsParallel)}
                  color={BRAND_COLOR}
                />
              </Pressable>
            </View>
          ) : (
            <View style={styles.toggleRow}>
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列実行可能
                </Text>
                <Checkbox
                  status={habit.parallelizable ? 'checked' : 'unchecked'}
                  disabled
                  color={BRAND_COLOR}
                />
              </View>
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列受け入れ
                </Text>
                <Checkbox
                  status={habit.allows_parallel ? 'checked' : 'unchecked'}
                  disabled
                  color={BRAND_COLOR}
                />
              </View>
            </View>
          )}
        </View>

        {/* Active */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>アクティブ</Text>
          {editing ? (
            <Checkbox
              status={active ? 'checked' : 'unchecked'}
              onPress={() => setActive(!active)}
              color={BRAND_COLOR}
            />
          ) : (
            <Checkbox
              status={habit.active ? 'checked' : 'unchecked'}
              disabled
              color={BRAND_COLOR}
            />
          )}
        </View>

        {/* Fixed */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>時間固定</Text>
          {editing ? (
            <>
              <Checkbox
                status={fixed ? 'checked' : 'unchecked'}
                onPress={() => setFixed(!fixed)}
                color={BRAND_COLOR}
              />
              <Text style={[styles.hint, { color: colors.grayLight }]}>
                開始時刻を固定し、スケジューラの移動を許可しない
              </Text>
            </>
          ) : (
            <Checkbox
              status={habit.fixed ? 'checked' : 'unchecked'}
              disabled
              color={BRAND_COLOR}
            />
          )}
        </View>

        {/* Recent generated tasks */}
        <View style={styles.section}>
          <Text style={[styles.label, { color: colors.gray }]}>
            直近のタスク
          </Text>
          {tasks.length === 0 ? (
            <Text style={[styles.value, { color: colors.black }]}>(なし)</Text>
          ) : (
            tasks.map((t) => (
              <Pressable
                key={t.id}
                style={[styles.taskItem, { backgroundColor: colors.surface }]}
                onPress={() => {
                  haptic.light();
                  router.push(`/task/${t.id}`);
                }}
              >
                <Text style={[styles.taskItemTitle, { color: colors.black }]}>
                  {t.title}
                </Text>
                <Text style={[styles.taskItemStatus, { color: colors.gray }]}>
                  {t.status}
                </Text>
              </Pressable>
            ))
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

      <RruleBuilderModal
        visible={showRruleBuilder}
        value={recurrence}
        onConfirm={(json) => {
          setRecurrence(json);
          setShowRruleBuilder(false);
        }}
        onCancel={() => setShowRruleBuilder(false)}
      />

      <DateTimePickerModal
        visible={pickerField !== null}
        mode="time"
        label={pickerField === 'start' ? '開始時刻' : '終了時刻'}
        value={timeStringToDate(pickerField === 'start' ? startTime : endTime)}
        onConfirm={(date) => {
          if (date) {
            const s = dateToTimeString(date);
            if (pickerField === 'start') setStartTime(s);
            else setEndTime(s);
          }
          setPickerField(null);
        }}
        onCancel={() => setPickerField(null)}
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
    paddingHorizontal: 4,
    paddingBottom: 4,
  },
  content: {
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
  section: {
    gap: 4,
  },
  label: {
    fontSize: 13,
    fontWeight: '500',
  },
  hint: {
    fontSize: 11,
    marginTop: 2,
  },
  value: {
    fontSize: 16,
  },
  descriptionInput: {
    minHeight: 80,
  },
  rruleHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  helpButton: {
    padding: 2,
  },
  dateField: {
    flexDirection: 'row',
    alignItems: 'center',
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 12,
    gap: 8,
  },
  dateText: {
    flex: 1,
    fontSize: 16,
  },
  row: {
    flexDirection: 'row',
    gap: 12,
  },
  timeField: {
    flex: 1,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    gap: 2,
  },
  timeFieldLabel: {
    fontSize: 12,
    fontWeight: '500',
  },
  timeFieldValue: {
    fontSize: 16,
  },
  costInput: {},
  costHint: {
    fontSize: 11,
    marginTop: 2,
    marginLeft: 4,
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
  taskItem: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingVertical: 12,
    paddingHorizontal: 12,
    borderRadius: 8,
    marginTop: 4,
  },
  taskItemTitle: {
    fontSize: 14,
    flex: 1,
  },
  taskItemStatus: {
    fontSize: 12,
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
