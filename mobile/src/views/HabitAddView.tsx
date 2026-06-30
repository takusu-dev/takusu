// HabitAddView — create a new habit
// Fields: title, recurrence (RRULE), cost (avg, sigma), abandonability

import { useState } from 'react';
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useRouter } from 'expo-router';
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { showError } from '@/src/api/errors';
import { COLORS, BRAND_COLOR } from '@/src/theme';

export function HabitAddView() {
  const { client } = useServer();
  const router = useRouter();

  const [title, setTitle] = useState('');
  const [recurrence, setRecurrence] = useState('FREQ=DAILY');
  const [startTime, setStartTime] = useState('09:00');
  const [endTime, setEndTime] = useState('10:00');
  const [avgMinutes, setAvgMinutes] = useState('60');
  const [sigmaMinutes, setSigmaMinutes] = useState('0');
  const [abandonability, setAbandonability] = useState(0.5);
  const [saving, setSaving] = useState(false);

  async function create() {
    if (!client || !title || saving) return;
    setSaving(true);
    try {
      const habit = await client.createHabit({
        title,
        recurrence,
        start_time: startTime,
        end_time: endTime,
        avg_minutes: parseInt(avgMinutes, 10) || 60,
        sigma_minutes: parseInt(sigmaMinutes, 10) || 0,
        abandonability,
      });
      undoRedo.push({
        description: `create habit: ${title}`,
        undo: async () => {
          await client.deleteHabit(habit.id);
        },
        redo: async () => {
          await client.createHabit({
            title,
            recurrence,
            start_time: startTime,
            end_time: endTime,
            avg_minutes: parseInt(avgMinutes, 10) || 60,
            sigma_minutes: parseInt(sigmaMinutes, 10) || 0,
            abandonability,
          });
        },
      });
      router.back();
    } catch (e) {
      showError(e, 'ハビットの追加に失敗');
    } finally {
      setSaving(false);
    }
  }

  return (
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <Text style={styles.title}>新規ハビット</Text>
        <View style={{ flex: 1 }} />
        <Pressable
          style={[styles.saveButton, (!title || saving) && styles.saveButtonDisabled]}
          onPress={create}
          disabled={!title || saving}
        >
          <Text style={styles.saveButtonText}>{saving ? '保存中…' : '追加'}</Text>
        </Pressable>
      </View>

      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.field}>
          <Text style={styles.label}>タイトル</Text>
          <TextInput
            style={styles.input}
            value={title}
            onChangeText={setTitle}
            placeholder="ハビット名"
          />
        </View>

        <View style={styles.field}>
          <Text style={styles.label}>周期 (RRULE)</Text>
          <TextInput
            style={styles.input}
            value={recurrence}
            onChangeText={setRecurrence}
            placeholder="FREQ=DAILY"
          />
        </View>

        <View style={styles.row}>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={styles.label}>開始時刻</Text>
            <TextInput
              style={styles.input}
              value={startTime}
              onChangeText={setStartTime}
              placeholder="09:00"
            />
          </View>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={styles.label}>終了時刻</Text>
            <TextInput
              style={styles.input}
              value={endTime}
              onChangeText={setEndTime}
              placeholder="10:00"
            />
          </View>
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
          <Text style={styles.label}>
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
});
