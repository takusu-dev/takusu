// NavigationButtons — right side floating, vertically arranged
// From top to bottom:
//   ⏫ (scroll up by day — fast)
//   ↑  (scroll up by page)
//   ↓  (scroll down by page)
//   ⏬ (scroll down by day — fast)
//   Calendar button (opens calendar overlay)

import { useState } from 'react';
import { Modal, Pressable, StyleSheet, Text, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { COLORS } from '@/src/theme';

interface NavigationButtonsProps {
  onScrollUpByDay?: () => void;
  onScrollUpByPage?: () => void;
  onScrollDownByDay?: () => void;
  onScrollDownByPage?: () => void;
  onJumpToDate?: (date: Date) => void;
  markedDates?: Set<string>; // YYYY-MM-DD
}

export function NavigationButtons({
  onScrollUpByDay,
  onScrollUpByPage,
  onScrollDownByDay,
  onScrollDownByPage,
  onJumpToDate,
  markedDates,
}: NavigationButtonsProps) {
  const [calendarOpen, setCalendarOpen] = useState(false);
  const [calMonth, setCalMonth] = useState(() => {
    const now = new Date();
    return new Date(now.getFullYear(), now.getMonth(), 1);
  });

  const monthLabel = `${calMonth.getFullYear()}年 ${calMonth.getMonth() + 1}月`;
  const daysInMonth = new Date(
    calMonth.getFullYear(),
    calMonth.getMonth() + 1,
    0,
  ).getDate();
  const firstDay = new Date(
    calMonth.getFullYear(),
    calMonth.getMonth(),
    1,
  ).getDay();

  function prevMonth() {
    setCalMonth(new Date(calMonth.getFullYear(), calMonth.getMonth() - 1, 1));
  }
  function nextMonth() {
    setCalMonth(new Date(calMonth.getFullYear(), calMonth.getMonth() + 1, 1));
  }

  function selectDay(day: number) {
    const date = new Date(calMonth.getFullYear(), calMonth.getMonth(), day);
    onJumpToDate?.(date);
    setCalendarOpen(false);
  }

  function dateKey(day: number): string {
    const d = new Date(calMonth.getFullYear(), calMonth.getMonth(), day);
    return `${d.getFullYear()}-${(d.getMonth() + 1)
      .toString()
      .padStart(2, '0')}-${d.getDate().toString().padStart(2, '0')}`;
  }

  return (
    <>
      <View style={styles.container}>
        <NavButton icon="arrow-up" onPress={onScrollUpByDay} />
        <NavButton icon="chevron-up" onPress={onScrollUpByPage} />
        <NavButton icon="chevron-down" onPress={onScrollDownByPage} />
        <NavButton icon="arrow-down" onPress={onScrollDownByDay} />
        <NavButton icon="calendar" onPress={() => setCalendarOpen(true)} />
      </View>

      <Modal visible={calendarOpen} transparent animationType="fade">
        <Pressable style={styles.overlay} onPress={() => setCalendarOpen(false)}>
          <View style={styles.calendar}>
            <View style={styles.calHeader}>
              <Pressable onPress={prevMonth}>
                <Text style={styles.calNav}>‹</Text>
              </Pressable>
              <Text style={styles.calMonthLabel}>{monthLabel}</Text>
              <Pressable onPress={nextMonth}>
                <Text style={styles.calNav}>›</Text>
              </Pressable>
            </View>
            <View style={styles.calGrid}>
              {['日', '月', '火', '水', '木', '金', '土'].map((d) => (
                <Text key={d} style={styles.calWeekday}>
                  {d}
                </Text>
              ))}
              {Array.from({ length: firstDay }).map((_, i) => (
                <View key={`empty-${i}`} />
              ))}
              {Array.from({ length: daysInMonth }).map((_, i) => {
                const day = i + 1;
                const marked = markedDates?.has(dateKey(day));
                return (
                  <Pressable
                    key={day}
                    style={[styles.calDay, marked && styles.calDayMarked]}
                    onPress={() => selectDay(day)}
                  >
                    <Text
                      style={[styles.calDayText, marked && styles.calDayTextMarked]}
                    >
                      {day}
                    </Text>
                  </Pressable>
                );
              })}
            </View>
          </View>
        </Pressable>
      </Modal>
    </>
  );
}

function NavButton({ icon, onPress }: { icon: keyof typeof Ionicons.glyphMap; onPress?: () => void }) {
  return (
    <Pressable
      style={({ pressed }) => [styles.navButton, pressed && styles.navButtonPressed]}
      onPress={onPress}
    >
      <Ionicons name={icon} size={20} color={COLORS.brand} />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  container: {
    position: 'absolute',
    right: 8,
    top: '40%',
    transform: [{ translateY: -100 }],
    gap: 4,
    zIndex: 10,
  },
  navButton: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: 'rgba(255,255,255,0.9)',
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.2,
    shadowRadius: 2,
    elevation: 2,
  },
  navButtonPressed: {
    backgroundColor: COLORS.brandLight,
  },
  navButtonText: {
    fontSize: 16,
    color: COLORS.brand,
  },
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.3)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  calendar: {
    backgroundColor: COLORS.white,
    borderRadius: 16,
    padding: 16,
    width: 320,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 8,
    elevation: 8,
  },
  calHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  calNav: {
    fontSize: 24,
    color: COLORS.brand,
    paddingHorizontal: 8,
  },
  calMonthLabel: {
    fontSize: 16,
    fontWeight: '600',
    color: COLORS.black,
  },
  calGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 2,
  },
  calWeekday: {
    width: 40,
    textAlign: 'center',
    fontSize: 12,
    color: COLORS.gray,
    fontWeight: '600',
  },
  calDay: {
    width: 40,
    height: 40,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 8,
  },
  calDayMarked: {
    backgroundColor: COLORS.brand,
  },
  calDayText: {
    fontSize: 14,
    color: COLORS.black,
  },
  calDayTextMarked: {
    color: COLORS.white,
    fontWeight: '600',
  },
});
