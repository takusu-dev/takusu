// DateTimePickerModal — modal wrapper around @react-native-community/datetimepicker
// Allows picking a date and time (or date-only) via native Android/iOS pickers.
// On Android, the native picker shows as a dialog; we wrap it in a Modal for
// consistent UX and provide a "clear" button for optional fields.

import { useEffect, useState } from 'react';
import { Modal, Pressable, StyleSheet, Text, View } from 'react-native';
import DateTimePicker, { type DateTimePickerEvent } from '@react-native-community/datetimepicker';
import { Ionicons } from '@expo/vector-icons';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { COLORS, BRAND_COLOR } from '@/src/theme';

interface DateTimePickerModalProps {
  visible: boolean;
  value: Date | null;
  mode: 'date' | 'datetime' | 'time';
  label: string;
  onConfirm: (date: Date | null) => void;
  onCancel: () => void;
  optional?: boolean;
  minimumDate?: Date;
}

export function DateTimePickerModal({
  visible,
  value,
  mode,
  label,
  onConfirm,
  onCancel,
  optional,
  minimumDate,
}: DateTimePickerModalProps) {
  const [tempDate, setTempDate] = useState<Date>(value ?? new Date());
  const [pickerMode, setPickerMode] = useState<'date' | 'time'>(
    mode === 'time' ? 'time' : 'date',
  );
  const [showPicker, setShowPicker] = useState(false);
  const insets = useSafeAreaInsets();

  // Sync tempDate with value prop when modal opens
  useEffect(() => {
    if (visible) {
      setTempDate(value ?? new Date());
    }
  }, [visible, value]);

  function openPicker(pMode: 'date' | 'time') {
    setPickerMode(pMode);
    setShowPicker(true);
  }

  function handlePickerChange(event: DateTimePickerEvent, selected?: Date) {
    setShowPicker(false);
    if (event.type === 'set' && selected) {
      setTempDate(selected);
    }
  }

  function formatDisplay(d: Date | null): string {
    if (!d) return '未設定';
    const dateStr = `${d.getFullYear()}/${(d.getMonth() + 1).toString().padStart(2, '0')}/${d.getDate().toString().padStart(2, '0')}`;
    if (mode === 'datetime') {
      const timeStr = `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
      return `${dateStr} ${timeStr}`;
    }
    if (mode === 'time') {
      return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
    }
    return dateStr;
  }

  return (
    <Modal visible={visible} transparent animationType="slide">
      <Pressable style={styles.overlay} onPress={onCancel}>
        <Pressable style={[styles.sheet, { paddingBottom: 32 + insets.bottom }]} onPress={(e) => e.stopPropagation()}>
          <View style={styles.header}>
            <Text style={styles.title}>{label}</Text>
            <Pressable onPress={onCancel}>
              <Ionicons name="close" size={24} color={COLORS.gray} />
            </Pressable>
          </View>

          {mode === 'time' ? (
            <Pressable style={styles.fieldRow} onPress={() => openPicker('time')}>
              <Ionicons name="time-outline" size={20} color={BRAND_COLOR} />
              <Text style={styles.fieldLabel}>時刻</Text>
              <Text style={styles.fieldValue}>
                {formatDisplay(tempDate)}
              </Text>
              <Ionicons name="chevron-forward" size={18} color={COLORS.grayLight} />
            </Pressable>
          ) : (
            <>
              <Pressable style={styles.fieldRow} onPress={() => openPicker('date')}>
                <Ionicons name="calendar-outline" size={20} color={BRAND_COLOR} />
                <Text style={styles.fieldLabel}>日付</Text>
                <Text style={styles.fieldValue}>
                  {formatDisplay(tempDate)}
                </Text>
                <Ionicons name="chevron-forward" size={18} color={COLORS.grayLight} />
              </Pressable>

              {mode === 'datetime' && (
                <Pressable
                  style={styles.fieldRow}
                  onPress={() => openPicker('time')}
                  disabled={!tempDate}
                >
                  <Ionicons name="time-outline" size={20} color={tempDate ? BRAND_COLOR : COLORS.grayLight} />
                  <Text style={[styles.fieldLabel, !tempDate && { color: COLORS.grayLight }]}>時間</Text>
                  <Text style={styles.fieldValue}>
                    {tempDate
                      ? `${tempDate.getHours().toString().padStart(2, '0')}:${tempDate.getMinutes().toString().padStart(2, '0')}`
                      : '—'}
                  </Text>
                  <Ionicons name="chevron-forward" size={18} color={tempDate ? COLORS.gray : COLORS.grayLight} />
                </Pressable>
              )}
            </>
          )}

          {optional && (
            <Pressable
              style={styles.clearButton}
              onPress={() => onConfirm(null)}
            >
              <Ionicons name="trash-outline" size={16} color={COLORS.red} />
              <Text style={styles.clearText}>クリア</Text>
            </Pressable>
          )}

          <View style={styles.actionRow}>
            <Pressable style={styles.cancelButton} onPress={onCancel}>
              <Text style={styles.cancelText}>キャンセル</Text>
            </Pressable>
            <Pressable
              style={styles.confirmButton}
              onPress={() => onConfirm(tempDate)}
            >
              <Text style={styles.confirmText}>設定</Text>
            </Pressable>
          </View>

          {showPicker && (
            <DateTimePicker
              value={tempDate}
              mode={pickerMode}
              display="default"
              onChange={handlePickerChange}
              minimumDate={minimumDate}
              timeZoneName={undefined}
            />
          )}
        </Pressable>
      </Pressable>
    </Modal>
  );
}

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.4)',
    justifyContent: 'flex-end',
  },
  sheet: {
    backgroundColor: COLORS.white,
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    padding: 20,
    paddingBottom: 32,
  },
  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 16,
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
    color: COLORS.black,
  },
  fieldRow: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 14,
    paddingHorizontal: 12,
    backgroundColor: '#F8F5FC',
    borderRadius: 10,
    marginBottom: 8,
    gap: 8,
  },
  fieldLabel: {
    fontSize: 15,
    color: COLORS.grayDark,
    flex: 1,
  },
  fieldValue: {
    fontSize: 15,
    color: COLORS.black,
    fontWeight: '500',
  },
  clearButton: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingVertical: 10,
    alignSelf: 'flex-start',
  },
  clearText: {
    fontSize: 14,
    color: COLORS.red,
  },
  actionRow: {
    flexDirection: 'row',
    gap: 12,
    marginTop: 16,
  },
  cancelButton: {
    flex: 1,
    paddingVertical: 12,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: COLORS.separator,
    alignItems: 'center',
  },
  cancelText: {
    fontSize: 15,
    color: COLORS.grayDark,
  },
  confirmButton: {
    flex: 1,
    paddingVertical: 12,
    borderRadius: 10,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
  },
  confirmText: {
    fontSize: 15,
    color: COLORS.white,
    fontWeight: '600',
  },
});
