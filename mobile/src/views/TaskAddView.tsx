// TaskAddView — create a new task with optional dependencies
// Fields: title, start_at (optional), end_at (required), avg_minutes, sigma_minutes, abandonability, description
// Can add dependency targets (select from existing tasks)

import { useState } from 'react';
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';
import { Checkbox } from 'react-native-paper';
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { showError } from '@/src/api/errors';
import type { TaskRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import { haptic } from '@/src/components/haptics';

interface TaskAddViewProps {
  /** Called when the view requests closing (back button / successful save).
   *  When omitted (route usage), falls back to router.back(). */
  onClose?: () => void;
  /** Pre-selected dependency IDs. Takes precedence over the `deps` search param. */
  initialDeps?: string[];
  /** When true the view is embedded inside a sheet that already provides
   *  top safe-area spacing (e.g. TaskAddSheet's grabber handle), so the
   *  topBar skips adding `insets.top` padding. Defaults to false (standalone route). */
  embedded?: boolean;
}

export function TaskAddView({
  onClose,
  initialDeps: propDeps,
  embedded = false,
}: TaskAddViewProps = {}) {
  const { client } = useServer();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const { deps } = useLocalSearchParams<{ deps?: string }>();

  const initialDeps: string[] = propDeps ?? (deps ? JSON.parse(deps) : []);

  function close() {
    if (onClose) onClose();
    else router.back();
  }

  const [title, setTitle] = useState('');
  const [startAt, setStartAt] = useState<Date | null>(null);
  const [endAt, setEndAt] = useState<Date | null>(null);
  const [avgMinutes, setAvgMinutes] = useState('60');
  const [sigmaMinutes, setSigmaMinutes] = useState('');
  const [abandonability, setAbandonability] = useState(0.5);
  const [parallelizable, setParallelizable] = useState(false);
  const [allowsParallel, setAllowsParallel] = useState(false);
  const [description, setDescription] = useState('');
  const [selectedDeps, setSelectedDeps] = useState<string[]>(initialDeps);
  const [allTasks, setAllTasks] = useState<TaskRow[]>([]);
  const [showDepPicker, setShowDepPicker] = useState(false);
  const [depSearch, setDepSearch] = useState('');
  const [pickerField, setPickerField] = useState<'start' | 'end' | null>(null);
  const [saving, setSaving] = useState(false);

  async function loadTasks() {
    if (!client) return;
    setAllTasks(await client.listTasks());
  }

  function formatDate(d: Date | null): string {
    if (!d) return '未設定';
    const dateStr = `${d.getFullYear()}/${(d.getMonth() + 1).toString().padStart(2, '0')}/${d.getDate().toString().padStart(2, '0')}`;
    const timeStr = `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
    return `${dateStr} ${timeStr}`;
  }

  function toISO(d: Date): string {
    return d.toISOString();
  }

  async function create() {
    if (!client || !title || !endAt || saving) return;
    haptic.medium();
    setSaving(true);
    const avg = parseInt(avgMinutes, 10) || 60;
    const sigmaRaw = parseInt(sigmaMinutes, 10);
    // sigma=0/未入力の時は未送信にしてサーバーの auto (avg/5) に任せる
    const sigma = sigmaRaw > 0 ? sigmaRaw : undefined;
    try {
      const task = await client.createTask({
        title,
        description: description || undefined,
        start_at: startAt ? toISO(startAt) : undefined,
        end_at: toISO(endAt),
        avg_minutes: avg,
        sigma_minutes: sigma,
        depends: selectedDeps.length > 0 ? selectedDeps : undefined,
        abandonability,
        parallelizable,
        allows_parallel: allowsParallel,
      });
      undoRedo.push({
        description: `create task: ${title}`,
        undo: async () => {
          await client.deleteTask(task.id);
        },
        redo: async () => {
          await client.createTask({
            title,
            description: description || undefined,
            start_at: startAt ? toISO(startAt) : undefined,
            end_at: toISO(endAt),
            avg_minutes: avg,
            sigma_minutes: sigma,
            depends: selectedDeps.length > 0 ? selectedDeps : undefined,
            abandonability,
            parallelizable,
            allows_parallel: allowsParallel,
          });
        },
      });
      close();
    } catch (e) {
      showError(e, 'タスクの追加に失敗');
    } finally {
      setSaving(false);
    }
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View
        style={[styles.topBar, { paddingTop: 8 + (embedded ? 0 : insets.top) }]}
      >
        <Pressable
          style={styles.backButton}
          onPress={() => {
            haptic.light();
            close();
          }}
        >
          <Ionicons name="chevron-back" size={28} color={BRAND_COLOR} />
        </Pressable>
        <Text style={[styles.title, { color: colors.black }]}>新規タスク</Text>
        <View style={{ flex: 1 }} />
        <Pressable
          style={[
            styles.saveButton,
            (!title || !endAt || saving) && styles.saveButtonDisabled,
          ]}
          onPress={create}
          disabled={!title || !endAt || saving}
        >
          <Text style={styles.saveButtonText}>
            {saving ? '保存中…' : '追加'}
          </Text>
        </Pressable>
      </View>

      <ScrollView
        contentContainerStyle={[
          styles.content,
          { paddingBottom: 40 + insets.bottom },
        ]}
      >
        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>タイトル</Text>
          <TextInput
            style={[
              styles.input,
              { borderColor: colors.separator, color: colors.black },
            ]}
            value={title}
            onChangeText={setTitle}
            placeholder="タスク名"
            placeholderTextColor={colors.grayLight}
          />
        </View>

        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>
            開始日時 (任意)
          </Text>
          <Pressable
            style={[
              styles.dateField,
              { borderColor: colors.separator, backgroundColor: colors.white },
            ]}
            onPress={() => {
              haptic.select();
              setPickerField('start');
            }}
          >
            <Ionicons name="calendar-outline" size={20} color={BRAND_COLOR} />
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
                style={styles.clearIcon}
                onPress={() => {
                  haptic.light();
                  setStartAt(null);
                }}
              >
                <Ionicons
                  name="close-circle"
                  size={18}
                  color={colors.grayLight}
                />
              </Pressable>
            )}
          </Pressable>
        </View>

        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>
            期限日時 (必須)
          </Text>
          <Pressable
            style={[
              styles.dateField,
              { borderColor: colors.separator, backgroundColor: colors.white },
            ]}
            onPress={() => {
              haptic.select();
              setPickerField('end');
            }}
          >
            <Ionicons name="calendar-outline" size={20} color={BRAND_COLOR} />
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

        <View style={styles.row}>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={[styles.label, { color: colors.gray }]}>avg (分)</Text>
            <TextInput
              style={[
                styles.input,
                { borderColor: colors.separator, color: colors.black },
              ]}
              value={avgMinutes}
              onChangeText={setAvgMinutes}
              keyboardType="numeric"
            />
          </View>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={[styles.label, { color: colors.gray }]}>
              sigma (分)
            </Text>
            <TextInput
              style={[
                styles.input,
                { borderColor: colors.separator, color: colors.black },
              ]}
              value={sigmaMinutes}
              onChangeText={setSigmaMinutes}
              keyboardType="numeric"
              placeholderTextColor={colors.grayLight}
            />
            {(!sigmaMinutes || sigmaMinutes === '0') && (
              <Text style={[styles.hint, { color: colors.grayLight }]}>
                {Math.max(1, Math.round((parseInt(avgMinutes, 10) || 60) / 5))}
                m (avg/5)
              </Text>
            )}
          </View>
        </View>

        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>
            abandonability: {abandonability.toFixed(2)}
          </Text>
          <Slider
            value={abandonability}
            onValueChange={setAbandonability}
            minimumValue={0}
            maximumValue={1}
            step={0.25}
            minimumTrackTintColor={BRAND_COLOR}
          />
        </View>

        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>並列設定</Text>
          <View style={styles.toggleRow}>
            <View style={styles.toggleItem}>
              <Text style={[styles.toggleLabel, { color: colors.black }]}>
                parallelizable
              </Text>
              <Checkbox
                status={parallelizable ? 'checked' : 'unchecked'}
                onPress={() => setParallelizable(!parallelizable)}
                color={BRAND_COLOR}
              />
            </View>
            <View style={styles.toggleItem}>
              <Text style={[styles.toggleLabel, { color: colors.black }]}>
                allows_parallel
              </Text>
              <Checkbox
                status={allowsParallel ? 'checked' : 'unchecked'}
                onPress={() => setAllowsParallel(!allowsParallel)}
                color={BRAND_COLOR}
              />
            </View>
          </View>
        </View>

        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>説明</Text>
          <TextInput
            style={[
              styles.input,
              styles.multiline,
              { borderColor: colors.separator, color: colors.black },
            ]}
            value={description}
            onChangeText={setDescription}
            multiline
            placeholder="説明 (任意)"
            placeholderTextColor={colors.grayLight}
          />
        </View>

        {/* Dependencies */}
        <View style={styles.field}>
          <View style={styles.depHeader}>
            <Text style={[styles.label, { color: colors.gray }]}>
              依存先タスク ({selectedDeps.length})
            </Text>
            <Pressable
              style={styles.addDepButton}
              onPress={() => {
                haptic.light();
                loadTasks();
                setDepSearch('');
                setShowDepPicker(true);
              }}
            >
              <Ionicons name="add" size={16} color={COLORS.white} />
              <Text style={styles.addDepButtonText}>追加</Text>
            </Pressable>
          </View>
          {selectedDeps.map((depId) => {
            const depTask = allTasks.find((t) => t.id === depId);
            return (
              <View
                key={depId}
                style={[styles.depItem, { backgroundColor: '#F8F5FC' }]}
              >
                <Text style={[styles.depItemText, { color: colors.black }]}>
                  {depTask
                    ? `#${depTask.display_id} ${depTask.title}`
                    : depId.slice(0, 8)}
                </Text>
                <Pressable
                  onPress={() => {
                    haptic.light();
                    setSelectedDeps(selectedDeps.filter((d) => d !== depId));
                  }}
                >
                  <Ionicons name="close" size={18} color={COLORS.red} />
                </Pressable>
              </View>
            );
          })}
        </View>
      </ScrollView>

      {/* Dep picker overlay */}
      {showDepPicker && (
        <View style={[styles.depPicker, { backgroundColor: colors.white }]}>
          <View
            style={[
              styles.depPickerHeader,
              {
                borderBottomColor: colors.separator,
                paddingTop: 16 + (embedded ? 0 : insets.top),
              },
            ]}
          >
            <Text style={[styles.depPickerTitle, { color: colors.black }]}>
              依存先を選択
            </Text>
            <Pressable
              onPress={() => {
                haptic.light();
                setShowDepPicker(false);
              }}
            >
              <Text style={styles.depPickerClose}>閉じる</Text>
            </Pressable>
          </View>
          <View
            style={[
              styles.depSearchContainer,
              { borderBottomColor: colors.separator },
            ]}
          >
            <Ionicons name="search" size={18} color={colors.gray} />
            <TextInput
              style={[styles.depSearchInput, { color: colors.black }]}
              value={depSearch}
              onChangeText={setDepSearch}
              placeholder="タイトルで検索"
              placeholderTextColor={colors.grayLight}
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
          <ScrollView style={styles.depPickerList}>
            {allTasks
              .filter((t) => !selectedDeps.includes(t.id))
              .filter((t) =>
                depSearch.length === 0
                  ? true
                  : t.title.toLowerCase().includes(depSearch.toLowerCase()),
              )
              .map((t) => (
                <Pressable
                  key={t.id}
                  style={[
                    styles.depPickerItem,
                    { borderBottomColor: colors.separator },
                  ]}
                  onPress={() => {
                    haptic.medium();
                    setSelectedDeps([...selectedDeps, t.id]);
                    setShowDepPicker(false);
                  }}
                >
                  <Text
                    style={[styles.depPickerItemId, { color: colors.gray }]}
                  >
                    #{t.display_id}
                  </Text>
                  <Text
                    style={[styles.depPickerItemText, { color: colors.black }]}
                  >
                    {t.title}
                  </Text>
                </Pressable>
              ))}
          </ScrollView>
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
        onConfirm={(date) => {
          setEndAt(date);
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
    paddingHorizontal: 8,
    paddingBottom: 8,
  },
  backButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
    marginLeft: 8,
  },
  saveButton: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    backgroundColor: BRAND_COLOR,
    borderRadius: 8,
  },
  saveButtonDisabled: {
    backgroundColor: COLORS.grayDark,
  },
  saveButtonText: {
    color: COLORS.white,
    fontSize: 14,
    fontWeight: '600',
  },
  content: {
    padding: 16,
    gap: 16,
  },
  field: {
    gap: 4,
  },
  row: {
    flexDirection: 'row',
    gap: 12,
  },
  label: {
    fontSize: 13,
    fontWeight: '500',
  },
  hint: {
    fontSize: 11,
    marginTop: 2,
  },
  input: {
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    fontSize: 16,
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
  clearIcon: {
    padding: 4,
  },
  multiline: {
    minHeight: 80,
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
  depHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  addDepButton: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 12,
    paddingVertical: 4,
    backgroundColor: BRAND_COLOR,
    borderRadius: 6,
    gap: 4,
  },
  addDepButtonText: {
    color: COLORS.white,
    fontSize: 13,
  },
  depItem: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 8,
    paddingHorizontal: 12,
    borderRadius: 8,
    marginTop: 4,
  },
  depItemText: {
    fontSize: 14,
  },
  depPicker: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    zIndex: 100,
  },
  depPickerHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 16,
    borderBottomWidth: 1,
  },
  depPickerTitle: {
    fontSize: 18,
    fontWeight: '600',
  },
  depPickerClose: {
    fontSize: 14,
    color: BRAND_COLOR,
  },
  depPickerList: {
    flex: 1,
  },
  depPickerItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingVertical: 16,
    paddingHorizontal: 16,
    borderBottomWidth: 1,
  },
  depPickerItemId: {
    fontSize: 13,
    fontWeight: '500',
    fontVariant: ['tabular-nums'],
  },
  depPickerItemText: {
    fontSize: 16,
    flex: 1,
  },
  depSearchContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 10,
    gap: 8,
    borderBottomWidth: 1,
  },
  depSearchInput: {
    flex: 1,
    fontSize: 16,
    paddingVertical: 4,
  },
});
