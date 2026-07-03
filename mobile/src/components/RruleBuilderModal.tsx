// RruleBuilderModal — bottom-sheet UI for constructing a RecurrenceRule.
//
// Replaces the raw RRULE text input with structured controls:
//   - Frequency (daily / weekly / monthly / yearly)
//   - Interval (every N units)
//   - Weekday chips (weekly)
//   - Month-day chips (monthly)
//   - Month chips (yearly)
//   - Optional occurrence count
//
// Emits a JSON-serialized RecurrenceRule string (see src/api/rrule.ts).

import { useEffect, useState } from 'react';
import {
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { haptic } from '@/src/components/haptics';
import {
  type Frequency,
  type RecurrenceRule,
  type Weekday,
  FREQUENCIES,
  FREQUENCY_LABELS,
  MONTHS,
  WEEKDAYS,
  WEEKDAY_LABELS,
  defaultRule,
  parseRule,
  serializeRule,
  summarizeRule,
} from '@/src/api/rrule';

/** Split an array into rows of `size` elements for grid layout. */
function chunk<T>(arr: T[], size: number): T[][] {
  const rows: T[][] = [];
  for (let i = 0; i < arr.length; i += size) {
    rows.push(arr.slice(i, i + size));
  }
  return rows;
}

const MONTH_DAYS = Array.from({ length: 31 }, (_, i) => i + 1);
const MONTH_DAY_ROWS = chunk(MONTH_DAYS, 7);

interface RruleBuilderModalProps {
  visible: boolean;
  value: string; // current JSON recurrence string
  onConfirm: (json: string) => void;
  onCancel: () => void;
}

export function RruleBuilderModal({
  visible,
  value,
  onConfirm,
  onCancel,
}: RruleBuilderModalProps) {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const [rule, setRule] = useState<RecurrenceRule>(defaultRule());
  const [showHelp, setShowHelp] = useState(false);

  useEffect(() => {
    if (visible) setRule(parseRule(value));
  }, [visible, value]);

  function update(patch: Partial<RecurrenceRule>) {
    setRule((r) => ({ ...r, ...patch }));
  }

  function toggleWeekday(wd: Weekday) {
    haptic.select();
    const has = rule.by_day.some((d) => d.n === null && d.weekday === wd);
    if (has) {
      update({ by_day: rule.by_day.filter((d) => !(d.n === null && d.weekday === wd)) });
    } else {
      update({ by_day: [...rule.by_day, { n: null, weekday: wd }] });
    }
  }

  function toggleMonthDay(day: number) {
    haptic.select();
    const has = rule.by_month_day.includes(day);
    update({
      by_month_day: has
        ? rule.by_month_day.filter((d) => d !== day)
        : [...rule.by_month_day, day].sort((a, b) => a - b),
    });
  }

  function toggleMonth(m: number) {
    haptic.select();
    const has = rule.by_month.includes(m);
    update({
      by_month: has
        ? rule.by_month.filter((x) => x !== m)
        : [...rule.by_month, m].sort((a, b) => a - b),
    });
  }

  const unit = rule.freq === 'daily' ? '日' : rule.freq === 'weekly' ? '週' : rule.freq === 'monthly' ? '月' : '年';

  return (
    <Modal visible={visible} transparent animationType="slide">
      <Pressable style={styles.overlay} onPress={onCancel}>
        <Pressable
          style={[styles.sheet, { backgroundColor: colors.white, paddingBottom: 32 + insets.bottom }]}
          onPress={(e) => e.stopPropagation()}
        >
          <View style={styles.header}>
            <View style={styles.headerLeft}>
              <Text style={[styles.title, { color: colors.black }]}>周期 (RRULE)</Text>
              <Pressable
                style={styles.helpButton}
                onPress={() => { haptic.light(); setShowHelp((v) => !v); }}
                hitSlop={8}
              >
                <Ionicons
                  name={showHelp ? 'help-circle' : 'help-circle-outline'}
                  size={20}
                  color={BRAND_COLOR}
                />
              </Pressable>
            </View>
            <Pressable onPress={() => { haptic.light(); onCancel(); }} hitSlop={8}>
              <Ionicons name="close" size={24} color={colors.gray} />
            </Pressable>
          </View>

          {showHelp && (
            <View style={[styles.helpBox, { backgroundColor: '#F8F5FC' }]}>
              <Text style={[styles.helpText, { color: colors.grayDark }]}>
                RRULEは繰り返しルールの標準形式です。{'\n'}
                ・頻度: 毎日・毎週・毎月・毎年{'\n'}
                ・間隔: N日/N週/Nヶ月/N年ごと{'\n'}
                ・曜日: 毎週のときに実行する曜日{'\n'}
                ・日付: 毎月の実行日 (例: 1日・15日){'\n'}
                ・月: 毎年の実行月{'\n'}
                ・回数: 繰り返す回数 (未設定なら無限)
              </Text>
            </View>
          )}

          <ScrollView style={styles.body} showsVerticalScrollIndicator={false}>
            {/* Frequency */}
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>頻度</Text>
            <View style={styles.segmented}>
              {FREQUENCIES.map((f) => (
                <Pressable
                  key={f}
                  style={[
                    styles.segment,
                    { borderColor: rule.freq === f ? BRAND_COLOR : colors.separator },
                    rule.freq === f && { backgroundColor: BRAND_COLOR },
                  ]}
                  onPress={() => { if (rule.freq !== f) haptic.select(); update({ freq: f as Frequency }); }}
                >
                  <Text
                    style={[
                      styles.segmentText,
                      { color: rule.freq === f ? COLORS.white : colors.black },
                    ]}
                  >
                    {FREQUENCY_LABELS[f]}
                  </Text>
                </Pressable>
              ))}
            </View>

            {/* Interval */}
            <Text style={[styles.sectionLabel, { color: colors.gray }]}>
              間隔 ({unit}ごと)
            </Text>
            <View style={styles.stepper}>
              <Pressable
                style={[styles.stepBtn, { borderColor: colors.separator }]}
                onPress={() => { haptic.select(); update({ interval: Math.max(1, rule.interval - 1) }); }}
              >
                <Ionicons name="remove" size={20} color={BRAND_COLOR} />
              </Pressable>
              <Text style={[styles.stepValue, { color: colors.black }]}>
                {rule.interval}
              </Text>
              <Pressable
                style={[styles.stepBtn, { borderColor: colors.separator }]}
                onPress={() => { haptic.select(); update({ interval: rule.interval + 1 }); }}
              >
                <Ionicons name="add" size={20} color={BRAND_COLOR} />
              </Pressable>
            </View>

            {/* Weekday chips (weekly) */}
            {rule.freq === 'weekly' && (
              <View style={styles.section}>
                <Text style={[styles.sectionLabel, { color: colors.gray }]}>曜日</Text>
                <View style={styles.weekdayChips}>
                  {WEEKDAYS.map((wd) => {
                    const on = rule.by_day.some((d) => d.n === null && d.weekday === wd);
                    return (
                      <Pressable
                        key={wd}
                        style={[
                          styles.weekdayChip,
                          { borderColor: on ? BRAND_COLOR : colors.separator },
                          on && { backgroundColor: BRAND_COLOR },
                        ]}
                        onPress={() => toggleWeekday(wd)}
                      >
                        <Text
                          style={[
                            styles.chipText,
                            { color: on ? COLORS.white : colors.black },
                          ]}
                        >
                          {WEEKDAY_LABELS[wd]}
                        </Text>
                      </Pressable>
                    );
                  })}
                </View>
              </View>
            )}

            {/* Month-day chips (monthly) */}
            {rule.freq === 'monthly' && (
              <View style={styles.section}>
                <Text style={[styles.sectionLabel, { color: colors.gray }]}>実行日</Text>
                {MONTH_DAY_ROWS.map((row, ri) => (
                  <View key={ri} style={styles.dayRow}>
                    {row.map((d) => {
                      const on = rule.by_month_day.includes(d);
                      return (
                        <Pressable
                          key={d}
                          style={[
                            styles.dayChip,
                            { borderColor: on ? BRAND_COLOR : colors.separator },
                            on && { backgroundColor: BRAND_COLOR },
                          ]}
                          onPress={() => toggleMonthDay(d)}
                        >
                          <Text
                            style={[
                              styles.chipText,
                              { color: on ? COLORS.white : colors.black },
                            ]}
                          >
                            {d}
                          </Text>
                        </Pressable>
                      );
                    })}
                    {Array.from({ length: 7 - row.length }, (_, i) => (
                      <View key={`pad-${i}`} style={styles.dayChipPlaceholder} />
                    ))}
                  </View>
                ))}
              </View>
            )}

            {/* Month chips (yearly) */}
            {rule.freq === 'yearly' && (
              <View style={styles.section}>
                <Text style={[styles.sectionLabel, { color: colors.gray }]}>月</Text>
                <View style={styles.chips}>
                  {MONTHS.map((m) => {
                    const on = rule.by_month.includes(m);
                    return (
                      <Pressable
                        key={m}
                        style={[
                          styles.chip,
                          { borderColor: on ? BRAND_COLOR : colors.separator },
                          on && { backgroundColor: BRAND_COLOR },
                        ]}
                        onPress={() => toggleMonth(m)}
                      >
                        <Text
                          style={[
                            styles.chipText,
                            { color: on ? COLORS.white : colors.black },
                          ]}
                        >
                          {m}月
                        </Text>
                      </Pressable>
                    );
                  })}
                </View>
                <Text style={[styles.sectionLabel, { color: colors.gray, marginTop: 12 }]}>
                  実行日
                </Text>
                {MONTH_DAY_ROWS.map((row, ri) => (
                  <View key={ri} style={styles.dayRow}>
                    {row.map((d) => {
                      const on = rule.by_month_day.includes(d);
                      return (
                        <Pressable
                          key={d}
                          style={[
                            styles.dayChip,
                            { borderColor: on ? BRAND_COLOR : colors.separator },
                            on && { backgroundColor: BRAND_COLOR },
                          ]}
                          onPress={() => toggleMonthDay(d)}
                        >
                          <Text
                            style={[
                              styles.chipText,
                              { color: on ? COLORS.white : colors.black },
                            ]}
                          >
                            {d}
                          </Text>
                        </Pressable>
                      );
                    })}
                    {Array.from({ length: 7 - row.length }, (_, i) => (
                      <View key={`pad-${i}`} style={styles.dayChipPlaceholder} />
                    ))}
                  </View>
                ))}
              </View>
            )}

            {/* Count */}
            <View style={styles.section}>
              <Text style={[styles.sectionLabel, { color: colors.gray }]}>
                回数 (任意・空欄で無限)
              </Text>
              <TextInput
                style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
                value={rule.count === null ? '' : String(rule.count)}
                onChangeText={(t) => {
                  const n = parseInt(t, 10);
                  update({ count: Number.isNaN(n) || t === '' ? null : Math.max(1, n) });
                }}
                keyboardType="numeric"
                placeholder="例: 10"
                placeholderTextColor={colors.grayLight}
              />
            </View>
          </ScrollView>

          {/* Summary */}
          <View style={[styles.summary, { backgroundColor: '#F8F5FC' }]}>
            <Text style={[styles.summaryLabel, { color: colors.gray }]}>プレビュー</Text>
            <Text style={[styles.summaryText, { color: colors.black }]}>
              {summarizeRule(rule)}
            </Text>
          </View>

          <View style={styles.actionRow}>
            <Pressable
              style={[styles.cancelButton, { borderColor: colors.separator }]}
              onPress={() => { haptic.light(); onCancel(); }}
            >
              <Text style={[styles.cancelText, { color: colors.grayDark }]}>キャンセル</Text>
            </Pressable>
            <Pressable
              style={styles.confirmButton}
              onPress={() => { haptic.medium(); onConfirm(serializeRule(rule)); }}
            >
              <Text style={styles.confirmText}>設定</Text>
            </Pressable>
          </View>
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
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    padding: 20,
    maxHeight: '85%',
  },
  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  headerLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
  },
  helpButton: {
    padding: 2,
  },
  helpBox: {
    borderRadius: 10,
    padding: 12,
    marginBottom: 12,
  },
  helpText: {
    fontSize: 13,
    lineHeight: 20,
  },
  body: {
    flexGrow: 0,
    flexShrink: 1,
  },
  section: {
    marginTop: 12,
  },
  sectionLabel: {
    fontSize: 13,
    fontWeight: '500',
    marginBottom: 8,
  },
  segmented: {
    flexDirection: 'row',
    gap: 8,
  },
  segment: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 10,
    borderWidth: 1,
    alignItems: 'center',
  },
  segmentText: {
    fontSize: 14,
    fontWeight: '500',
  },
  stepper: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 16,
  },
  stepBtn: {
    width: 40,
    height: 40,
    borderRadius: 20,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  stepValue: {
    fontSize: 20,
    fontWeight: '600',
    minWidth: 40,
    textAlign: 'center',
    fontVariant: ['tabular-nums'],
  },
  chips: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
  },
  chip: {
    paddingHorizontal: 14,
    paddingVertical: 8,
    borderRadius: 8,
    borderWidth: 1,
    alignItems: 'center',
  },
  chipSmall: {
    paddingHorizontal: 10,
    paddingVertical: 6,
    minWidth: 36,
    alignItems: 'center',
  },
  // Weekday chips: 7 across, evenly spaced (no wrap)
  weekdayChips: {
    flexDirection: 'row',
    gap: 6,
  },
  weekdayChip: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 8,
    borderWidth: 1,
    alignItems: 'center',
  },
  // Month-day chips: 7 per row, evenly spaced
  dayRow: {
    flexDirection: 'row',
    gap: 6,
    marginBottom: 6,
  },
  dayChip: {
    flex: 1,
    paddingVertical: 8,
    borderRadius: 8,
    borderWidth: 1,
    alignItems: 'center',
  },
  // Invisible spacer to keep incomplete last row aligned with 7-column rows
  dayChipPlaceholder: {
    flex: 1,
  },
  chipText: {
    fontSize: 14,
    fontWeight: '500',
  },
  input: {
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    fontSize: 16,
  },
  summary: {
    borderRadius: 10,
    padding: 12,
    marginTop: 16,
  },
  summaryLabel: {
    fontSize: 12,
    fontWeight: '500',
    marginBottom: 4,
  },
  summaryText: {
    fontSize: 15,
    fontWeight: '600',
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
    alignItems: 'center',
  },
  cancelText: {
    fontSize: 15,
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
