// HabitDetailView — view and edit a habit + recent generated tasks

import { useCallback, useEffect, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
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
import type { HabitRow, TaskRow } from '@/src/api/types';
import { BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { RruleBuilderModal } from '@/src/components/RruleBuilderModal';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import { parseRule, summarizeRule } from '@/src/api/rrule';
import { haptic } from '@/src/components/haptics';

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
      }
    } catch (e) {
      showError(e, 'Habitの取得に失敗');
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
    undoRedo.push({
      description: `delete habit: ${habit.title}`,
      undo: async () => {
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
        });
        // CreateHabit does not accept `active`; restore it via update.
        if (!prev.active) {
          await client.updateHabit(recreated.id, { active: prev.active });
        }
        currentId = recreated.id;
      },
      redo: async () => {
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
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列実行可能
                </Text>
                <Checkbox
                  status={parallelizable ? 'checked' : 'unchecked'}
                  onPress={() => setParallelizable(!parallelizable)}
                  color={BRAND_COLOR}
                />
              </View>
              <View style={styles.toggleItem}>
                <Text style={[styles.toggleLabel, { color: colors.black }]}>
                  並列受け入れ
                </Text>
                <Checkbox
                  status={allowsParallel ? 'checked' : 'unchecked'}
                  onPress={() => setAllowsParallel(!allowsParallel)}
                  color={BRAND_COLOR}
                />
              </View>
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
});
