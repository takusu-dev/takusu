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
import { Ionicons } from '@expo/vector-icons';
import Slider from '@expo/ui/community/slider';
import { useServer } from '@/src/api/ServerProvider';
import { undoRedo } from '@/src/api/undoRedo';
import { showError } from '@/src/api/errors';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { RruleBuilderModal } from '@/src/components/RruleBuilderModal';
import { haptic } from '@/src/components/haptics';
import { defaultRule, parseRule, serializeRule, summarizeRule } from '@/src/api/rrule';

export function HabitAddView() {
  const { client } = useServer();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();

  const [title, setTitle] = useState('');
  const [recurrence, setRecurrence] = useState(serializeRule(defaultRule()));
  const [showRruleBuilder, setShowRruleBuilder] = useState(false);
  const [startTime, setStartTime] = useState('09:00');
  const [endTime, setEndTime] = useState('10:00');
  const [avgMinutes, setAvgMinutes] = useState('60');
  const [sigmaMinutes, setSigmaMinutes] = useState('0');
  const [abandonability, setAbandonability] = useState(0.5);
  const [saving, setSaving] = useState(false);

  async function create() {
    if (!client || !title || saving) return;
    haptic.medium();
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
      showError(e, 'Habitの追加に失敗');
    } finally {
      setSaving(false);
    }
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View style={[styles.topBar, { paddingTop: 8 + insets.top }]}>
        <Pressable style={styles.backButton} onPress={() => { haptic.light(); router.back(); }}>
          <Ionicons name="chevron-back" size={28} color={BRAND_COLOR} />
        </Pressable>
        <Text style={[styles.title, { color: colors.black }]}>New Habit</Text>
        <View style={{ flex: 1 }} />
        <Pressable
          style={[styles.saveButton, (!title || saving) && styles.saveButtonDisabled]}
          onPress={create}
          disabled={!title || saving}
        >
          <Text style={styles.saveButtonText}>{saving ? '保存中…' : '追加'}</Text>
        </Pressable>
      </View>

      <ScrollView
        contentContainerStyle={[styles.content, { paddingBottom: 40 + insets.bottom }]}
      >
        <View style={styles.field}>
          <Text style={[styles.label, { color: colors.gray }]}>タイトル</Text>
          <TextInput
            style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
            value={title}
            onChangeText={setTitle}
            placeholder="Habit name"
            placeholderTextColor={colors.grayLight}
          />
        </View>

        <View style={styles.field}>
          <View style={styles.rruleHeader}>
            <Text style={[styles.label, { color: colors.gray }]}>周期 (RRULE)</Text>
            <Pressable
              style={styles.helpButton}
              onPress={() => { haptic.light(); setShowRruleBuilder(true); }}
              hitSlop={8}
            >
              <Ionicons name="help-circle-outline" size={18} color={BRAND_COLOR} />
            </Pressable>
          </View>
          <Pressable
            style={[styles.dateField, { borderColor: colors.separator, backgroundColor: colors.white }]}
            onPress={() => { haptic.light(); setShowRruleBuilder(true); }}
          >
            <Ionicons name="repeat" size={20} color={BRAND_COLOR} />
            <Text
              style={[styles.dateText, { color: colors.black }]}
              numberOfLines={2}
            >
              {summarizeRule(parseRule(recurrence))}
            </Text>
            <Ionicons name="chevron-forward" size={18} color={colors.grayLight} />
          </Pressable>
        </View>

        <View style={styles.row}>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={[styles.label, { color: colors.gray }]}>開始時刻</Text>
            <TextInput
              style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
              value={startTime}
              onChangeText={setStartTime}
              placeholder="09:00"
              placeholderTextColor={colors.grayLight}
            />
          </View>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={[styles.label, { color: colors.gray }]}>終了時刻</Text>
            <TextInput
              style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
              value={endTime}
              onChangeText={setEndTime}
              placeholder="10:00"
              placeholderTextColor={colors.grayLight}
            />
          </View>
        </View>

        <View style={styles.row}>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={[styles.label, { color: colors.gray }]}>avg (分)</Text>
            <TextInput
              style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
              value={avgMinutes}
              onChangeText={setAvgMinutes}
              keyboardType="numeric"
            />
          </View>
          <View style={[styles.field, { flex: 1 }]}>
            <Text style={[styles.label, { color: colors.gray }]}>sigma (分)</Text>
            <TextInput
              style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
              value={sigmaMinutes}
              onChangeText={setSigmaMinutes}
              keyboardType="numeric"
            />
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
  label: {
    fontSize: 13,
    fontWeight: '500',
  },
  input: {
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    fontSize: 16,
  },
});
