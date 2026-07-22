// TaskProgressSheet — bottom sheet for recording task progress
// Supports entering either delta (this-time) or cumulative quantity, and
// allows editing the total. Used from HomeView and TaskDetailView.

import { useEffect, useMemo, useState } from 'react';
import {
  Modal,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import type { TaskRow } from '@/src/api/types';
import { useTheme, BRAND_COLOR } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

export interface TaskProgressSheetPayload {
  quantityDone: number;
  note?: string;
  quantityTotal?: number;
}

interface TaskProgressSheetProps {
  visible: boolean;
  task: TaskRow;
  mode: 'record' | 'pause';
  onConfirm: (payload: TaskProgressSheetPayload) => void;
  onCancel: () => void;
}

export function TaskProgressSheet({
  visible,
  task,
  mode,
  onConfirm,
  onCancel,
}: TaskProgressSheetProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const currentDone = useMemo(() => task.quantity_done ?? 0, [task]);
  const currentTotal = useMemo(() => task.quantity_total, [task]);

  const [delta, setDelta] = useState('');
  const [cumulative, setCumulative] = useState(String(currentDone));
  const [total, setTotal] = useState(
    currentTotal !== undefined ? String(currentTotal) : '',
  );
  const [note, setNote] = useState('');

  useEffect(() => {
    if (visible) {
      setDelta('');
      setCumulative(String(currentDone));
      setTotal(currentTotal !== undefined ? String(currentTotal) : '');
      setNote('');
    }
  }, [visible, currentDone, currentTotal]);

  function digitsOnly(text: string) {
    return text.replace(/[^0-9]/g, '');
  }

  function updateFromCumulative(text: string) {
    const filtered = digitsOnly(text);
    setCumulative(filtered);
    const q = parseInt(filtered, 10);
    if (Number.isNaN(q)) return;
    setDelta(String(q - currentDone));
  }

  function updateFromDelta(text: string) {
    const filtered = digitsOnly(text);
    setDelta(filtered);
    const d = parseInt(filtered, 10);
    if (Number.isNaN(d)) return;
    setCumulative(String(currentDone + d));
  }

  function handleConfirm() {
    haptic.medium();
    const cumulativeNum = parseInt(cumulative, 10);
    const quantityDone = Number.isNaN(cumulativeNum)
      ? currentDone
      : cumulativeNum;
    const totalNum = parseInt(total, 10);
    const quantityTotal =
      !Number.isNaN(totalNum) && totalNum > 0 ? totalNum : undefined;
    onConfirm({
      quantityDone,
      note: note.trim() || undefined,
      quantityTotal,
    });
  }

  return (
    <Modal
      visible={visible}
      transparent
      animationType="slide"
      onRequestClose={onCancel}
    >
      <Pressable style={styles.overlay} onPress={onCancel}>
        <View
          style={[
            styles.sheet,
            {
              backgroundColor: colors.surface,
              paddingBottom: 16 + insets.bottom,
            },
          ]}
        >
          <Text style={[styles.title, { color: colors.black }]}>
            {mode === 'pause' ? '進捗を記録して一時停止' : '進捗を記録'}
          </Text>

          <View style={styles.row}>
            <TextInput
              style={[
                styles.input,
                styles.inputFlex,
                {
                  borderColor: colors.separator,
                  color: colors.black,
                  backgroundColor: colors.white,
                },
              ]}
              placeholder="差分"
              placeholderTextColor={colors.grayLight}
              keyboardType="number-pad"
              value={delta}
              onChangeText={updateFromDelta}
            />
            <TextInput
              style={[
                styles.input,
                styles.inputFlex,
                {
                  borderColor: colors.separator,
                  color: colors.black,
                  backgroundColor: colors.white,
                },
              ]}
              placeholder="累積"
              placeholderTextColor={colors.grayLight}
              keyboardType="number-pad"
              value={cumulative}
              onChangeText={updateFromCumulative}
            />
          </View>

          <View style={styles.row}>
            <TextInput
              style={[
                styles.input,
                styles.inputFlex,
                {
                  borderColor: colors.separator,
                  color: colors.black,
                  backgroundColor: colors.white,
                },
              ]}
              placeholder="全体"
              placeholderTextColor={colors.grayLight}
              keyboardType="number-pad"
              value={total}
              onChangeText={(text) => setTotal(text.replace(/[^0-9]/g, ''))}
            />
          </View>

          <TextInput
            style={[
              styles.input,
              styles.note,
              {
                borderColor: colors.separator,
                color: colors.black,
                backgroundColor: colors.white,
              },
            ]}
            placeholder="メモ（任意）"
            placeholderTextColor={colors.grayLight}
            value={note}
            onChangeText={setNote}
          />

          <View style={styles.actions}>
            <Pressable
              style={[
                styles.button,
                styles.secondary,
                { borderColor: colors.separator },
              ]}
              onPress={onCancel}
            >
              <Text style={{ color: colors.black }}>キャンセル</Text>
            </Pressable>
            <Pressable
              style={[styles.button, { backgroundColor: BRAND_COLOR }]}
              onPress={handleConfirm}
            >
              <Text style={styles.primaryText}>
                {mode === 'pause' ? '記録して一時停止' : '記録'}
              </Text>
            </Pressable>
          </View>
        </View>
      </Pressable>
    </Modal>
  );
}

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    justifyContent: 'flex-end',
    backgroundColor: 'rgba(0,0,0,0.35)',
  },
  sheet: {
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    padding: 20,
    gap: 12,
  },
  title: {
    fontSize: 16,
    fontWeight: '700',
    marginBottom: 4,
  },
  row: {
    flexDirection: 'row',
    gap: 8,
  },
  input: {
    borderWidth: 1,
    borderRadius: 10,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
  },
  inputFlex: {
    flex: 1,
  },
  note: {
    marginTop: 4,
  },
  actions: {
    flexDirection: 'row',
    gap: 8,
    marginTop: 8,
  },
  button: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 12,
    borderRadius: 10,
  },
  secondary: {
    borderWidth: 1,
  },
  primaryText: {
    color: '#FFFFFF',
    fontWeight: '600',
  },
});
