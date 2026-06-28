// TaskAddView — create a new task with optional dependencies
// Fields: title, end_at, avg_minutes, sigma_minutes, abandonability, description
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
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import type { TaskRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR } from '@/src/theme';

export function TaskAddView() {
  const { client } = useServer();
  const router = useRouter();
  const { deps } = useLocalSearchParams<{ deps?: string }>();

  const initialDeps: string[] = deps ? JSON.parse(deps) : [];

  const [title, setTitle] = useState('');
  const [endAt, setEndAt] = useState('');
  const [avgMinutes, setAvgMinutes] = useState('60');
  const [sigmaMinutes, setSigmaMinutes] = useState('0');
  const [abandonability, setAbandonability] = useState(0.5);
  const [description, setDescription] = useState('');
  const [selectedDeps, setSelectedDeps] = useState<string[]>(initialDeps);
  const [allTasks, setAllTasks] = useState<TaskRow[]>([]);
  const [showDepPicker, setShowDepPicker] = useState(false);

  async function loadTasks() {
    if (!client) return;
    setAllTasks(await client.listTasks());
  }

  async function create() {
    if (!client || !title || !endAt) return;
    const task = await client.createTask({
      title,
      description: description || undefined,
      end_at: endAt,
      avg_minutes: parseInt(avgMinutes, 10) || 60,
      sigma_minutes: parseInt(sigmaMinutes, 10) || 0,
      depends: selectedDeps.length > 0 ? selectedDeps : undefined,
      abandonability,
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
          end_at: endAt,
          avg_minutes: parseInt(avgMinutes, 10) || 60,
          sigma_minutes: parseInt(sigmaMinutes, 10) || 0,
          depends: selectedDeps.length > 0 ? selectedDeps : undefined,
          abandonability,
        });
      },
    });
    router.back();
  }

  return (
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <Text style={styles.title}>新規タスク</Text>
        <View style={{ flex: 1 }} />
        <Pressable style={styles.saveButton} onPress={create}>
          <Text style={styles.saveButtonText}>追加</Text>
        </Pressable>
      </View>

      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.field}>
          <Text style={styles.label}>タイトル</Text>
          <TextInput
            style={styles.input}
            value={title}
            onChangeText={setTitle}
            placeholder="タスク名"
          />
        </View>

        <View style={styles.field}>
          <Text style={styles.label}>期限 (ISO)</Text>
          <TextInput
            style={styles.input}
            value={endAt}
            onChangeText={setEndAt}
            placeholder="2026-06-30T18:00:00+09:00"
          />
        </View>

        <View style={styles.row}>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={styles.label}>avg (分)</Text>
            <TextInput
              style={styles.input}
              value={avgMinutes}
              onChangeText={setAvgMinutes}
              keyboardType="numeric"
            />
          </View>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={styles.label}>sigma (分)</Text>
            <TextInput
              style={styles.input}
              value={sigmaMinutes}
              onChangeText={setSigmaMinutes}
              keyboardType="numeric"
            />
          </View>
        </View>

        <View style={styles.field}>
          <Text style={styles.label}>abandonability: {abandonability.toFixed(2)}</Text>
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
          <Text style={styles.label}>説明</Text>
          <TextInput
            style={[styles.input, styles.multiline]}
            value={description}
            onChangeText={setDescription}
            multiline
            placeholder="説明 (任意)"
          />
        </View>

        {/* Dependencies */}
        <View style={styles.field}>
          <View style={styles.depHeader}>
            <Text style={styles.label}>依存先タスク ({selectedDeps.length})</Text>
            <Pressable
              style={styles.addDepButton}
              onPress={() => {
                loadTasks();
                setShowDepPicker(true);
              }}
            >
              <Text style={styles.addDepButtonText}>+ 追加</Text>
            </Pressable>
          </View>
          {selectedDeps.map((depId) => {
            const depTask = allTasks.find((t) => t.id === depId);
            return (
              <View key={depId} style={styles.depItem}>
                <Text style={styles.depItemText}>
                  {depTask?.title ?? depId.slice(0, 8)}
                </Text>
                <Pressable
                  onPress={() =>
                    setSelectedDeps(selectedDeps.filter((d) => d !== depId))
                  }
                >
                  <Text style={styles.depRemove}>✕</Text>
                </Pressable>
              </View>
            );
          })}
        </View>

        {/* Dep picker overlay */}
        {showDepPicker && (
          <View style={styles.depPicker}>
            <View style={styles.depPickerHeader}>
              <Text style={styles.depPickerTitle}>依存先を選択</Text>
              <Pressable onPress={() => setShowDepPicker(false)}>
                <Text style={styles.depPickerClose}>閉じる</Text>
              </Pressable>
            </View>
            <ScrollView style={styles.depPickerList}>
              {allTasks
                .filter((t) => !selectedDeps.includes(t.id))
                .map((t) => (
                  <Pressable
                    key={t.id}
                    style={styles.depPickerItem}
                    onPress={() => {
                      setSelectedDeps([...selectedDeps, t.id]);
                      setShowDepPicker(false);
                    }}
                  >
                    <Text style={styles.depPickerItemText}>{t.title}</Text>
                  </Pressable>
                ))}
            </ScrollView>
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
  title: {
    fontSize: 18,
    fontWeight: '600',
    color: COLORS.black,
    marginLeft: 8,
  },
  saveButton: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    backgroundColor: BRAND_COLOR,
    borderRadius: 8,
  },
  saveButtonText: {
    color: COLORS.white,
    fontSize: 14,
    fontWeight: '600',
  },
  content: {
    padding: 16,
    gap: 16,
    paddingBottom: 40,
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
    color: COLORS.gray,
    fontWeight: '500',
  },
  input: {
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    fontSize: 16,
  },
  multiline: {
    minHeight: 80,
  },
  depHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  addDepButton: {
    paddingHorizontal: 12,
    paddingVertical: 4,
    backgroundColor: BRAND_COLOR,
    borderRadius: 6,
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
    backgroundColor: '#F8F5FC',
    borderRadius: 8,
    marginTop: 4,
  },
  depItemText: {
    fontSize: 14,
    color: COLORS.black,
  },
  depRemove: {
    fontSize: 16,
    color: COLORS.red,
  },
  depPicker: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: COLORS.white,
    zIndex: 100,
  },
  depPickerHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 16,
    paddingTop: 60,
    borderBottomWidth: 1,
    borderBottomColor: COLORS.separator,
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
    paddingVertical: 16,
    paddingHorizontal: 16,
    borderBottomWidth: 1,
    borderBottomColor: COLORS.separator,
  },
  depPickerItemText: {
    fontSize: 16,
  },
});
