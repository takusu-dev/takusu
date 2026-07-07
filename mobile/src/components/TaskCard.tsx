// TaskCard component — displays a single task in the list
// Left: start/end time, Center: title, Right-bottom: cost (avg, sigma)
// Background color based on abandonability
// Slide right cycles: start → complete → revert (#312)
// Slide left = delete (strong haptics)
// Slide actions show a background preview with icon
// Done tasks: strikethrough + gray

import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { GestureDetector, Gesture } from 'react-native-gesture-handler';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  runOnJS,
  withSpring,
} from 'react-native-reanimated';
import { Ionicons } from '@expo/vector-icons';
import type { TaskRow } from '@/src/api/types';
import { parseDepends } from '@/src/api/types';
import { taskCardColor, BRAND_COLOR, COLORS, useTheme } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

interface TaskCardProps {
  task: TaskRow;
  scheduleStart?: string;
  scheduleEnd?: string;
  isDone: boolean;
  onPress: () => void;
  onDone?: () => void;
  onDelete?: () => void;
  onLongPress?: () => void;
  selected?: boolean;
  // Habit display_id for habit-based coloring (#309). Undefined when the
  // task has no habit or the habit map is unavailable.
  habitDisplayId?: number;
}

function formatTime(iso?: string): string {
  if (!iso) return '--:--';
  const d = new Date(iso);
  return `${d.getHours().toString().padStart(2, '0')}:${d
    .getMinutes()
    .toString()
    .padStart(2, '0')}`;
}

// Format a deadline hint "〜M/D" when the task's deadline (end_at) falls on
// a different day than the scheduled start — i.e. a multi-day window
// (period-mode habits, #window_mode). Returns '' for same-day tasks.
function deadlineHint(task: TaskRow, scheduleStart?: string): string {
  if (!task.end_at || !scheduleStart) return '';
  const start = new Date(scheduleStart);
  const end = new Date(task.end_at);
  if (
    start.getFullYear() === end.getFullYear() &&
    start.getMonth() === end.getMonth() &&
    start.getDate() === end.getDate()
  ) {
    return '';
  }
  return `〜${end.getMonth() + 1}/${end.getDate()}`;
}

function TaskCardImpl({
  task,
  scheduleStart,
  scheduleEnd,
  isDone,
  onPress,
  onDone,
  onDelete,
  onLongPress,
  selected,
  habitDisplayId,
}: TaskCardProps) {
  const translateX = useSharedValue(0);
  // Track which direction the haptic last fired for (0=none, 1=right, -1=left)
  // so reversing swipe direction mid-gesture re-fires the haptic (#313).
  const hapticFiredDir = useSharedValue(0);
  const { dark, colors } = useTheme();

  // Single pan gesture handles both swipe-right (done) and swipe-left (delete).
  // Using Gesture.Race with two separate pans was unreliable for left swipe
  // (#230): Race resolution between gestures with activeOffsetX in opposite
  // directions can fail to activate. A single gesture with bidirectional
  // activeOffsetX avoids the issue entirely.
  const pan = Gesture.Pan()
    .activeOffsetX([-10, 10])
    .failOffsetY([-10, 10])
    .onUpdate((e) => {
      translateX.value = e.translationX;
      // Fire haptic when crossing the action threshold mid-slide (#313).
      if (e.translationX > 80 && onDone && hapticFiredDir.value !== 1) {
        hapticFiredDir.value = 1;
        runOnJS(haptic.light)();
      } else if (
        e.translationX < -80 &&
        onDelete &&
        hapticFiredDir.value !== -1
      ) {
        hapticFiredDir.value = -1;
        runOnJS(haptic.medium)();
      }
    })
    .onEnd((e) => {
      if (e.translationX > 80 && onDone) {
        runOnJS(onDone)();
      } else if (e.translationX < -80 && onDelete) {
        runOnJS(onDelete)();
      }
      translateX.value = withSpring(0);
    })
    // onFinalize fires for both END and CANCELLED terminal states, ensuring
    // hapticFiredDir is always reset even if the gesture is interrupted.
    .onFinalize(() => {
      hapticFiredDir.value = 0;
    });

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: translateX.value }],
  }));

  // Background preview opacity for slide actions (#170)
  const doneBgStyle = useAnimatedStyle(() => ({
    opacity: Math.min(1, Math.max(0, translateX.value / 80)),
  }));
  const deleteBgStyle = useAnimatedStyle(() => ({
    opacity: Math.min(1, Math.max(0, -translateX.value / 80)),
  }));

  const bgColor = taskCardColor(
    task.abandonability,
    task.habit_id,
    habitDisplayId,
    dark,
  );
  const deps = parseDepends(task.depends);

  // Slide-right background preview: icon and color depend on what the
  // next state in the cycle will be (#312).
  // pending → completed (checkmark, green)
  // scheduled → in_progress (play, blue), in_progress → completed (check, green),
  // completed → scheduled (refresh, red)
  const isPending = task.status === 'pending';
  const isInProgress = task.status === 'in_progress';
  const doneIcon = isDone
    ? 'refresh'
    : isPending
      ? 'checkmark'
      : isInProgress
        ? 'checkmark'
        : 'play';
  const doneColor = isDone
    ? COLORS.red
    : isPending
      ? COLORS.green
      : isInProgress
        ? COLORS.green
        : BRAND_COLOR;

  const handlePress = () => {
    haptic.light();
    onPress();
  };
  const handleLongPress = onLongPress
    ? () => {
        haptic.medium();
        onLongPress();
      }
    : undefined;

  return (
    <View style={styles.container}>
      {/* Slide action preview backgrounds (#170) */}
      <Reanimated.View
        style={[styles.doneBg, { backgroundColor: doneColor }, doneBgStyle]}
        pointerEvents="none"
      >
        <Ionicons name={doneIcon} size={28} color={COLORS.white} />
      </Reanimated.View>
      <Reanimated.View
        style={[
          styles.deleteBg,
          { backgroundColor: COLORS.red },
          deleteBgStyle,
        ]}
        pointerEvents="none"
      >
        <Ionicons name="trash" size={28} color={COLORS.white} />
      </Reanimated.View>
      <GestureDetector gesture={pan}>
        <Reanimated.View
          style={[styles.card, { backgroundColor: bgColor }, animatedStyle]}
        >
          <Pressable
            style={({ pressed }) => [
              styles.cardInner,
              pressed && styles.pressed,
              selected && styles.cardSelected,
            ]}
            onPress={handlePress}
            onLongPress={handleLongPress}
          >
            {/* Left: times */}
            <View style={styles.times}>
              <Text style={[styles.timeText, { color: colors.grayDark }]}>
                {formatTime(scheduleStart)}
              </Text>
              <Text style={[styles.timeText, { color: colors.grayDark }]}>
                {formatTime(scheduleEnd)}
              </Text>
            </View>

            {/* Center: title */}
            <View style={styles.titleContainer}>
              <Text style={[styles.taskId, { color: colors.gray }]}>
                {task.habit_id && habitDisplayId !== undefined
                  ? `h${habitDisplayId}#${task.display_id}`
                  : `#${task.display_id}`}
              </Text>
              <Text
                style={[
                  styles.title,
                  { color: colors.black },
                  isDone && {
                    textDecorationLine: 'line-through',
                    color: colors.done,
                  },
                ]}
                numberOfLines={2}
              >
                {task.title}
              </Text>
              {deps.length > 0 && (
                <Text style={[styles.depsCount, { color: colors.gray }]}>
                  ↳ {deps.length} deps
                </Text>
              )}
            </View>

            {/* Right-bottom: cost */}
            <View style={styles.cost}>
              <Text style={[styles.costText, { color: colors.gray }]}>
                {task.avg_minutes}m ±{task.sigma_minutes}
              </Text>
              {(() => {
                const hint = deadlineHint(task, scheduleStart);
                return hint ? (
                  <Text style={[styles.deadlineHint, { color: colors.gray }]}>
                    {hint}
                  </Text>
                ) : null;
              })()}
            </View>
          </Pressable>
        </Reanimated.View>
      </GestureDetector>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    marginHorizontal: 12,
    marginVertical: 4,
    position: 'relative',
  },
  card: {
    borderRadius: 12,
    minHeight: 72,
  },
  cardInner: {
    flexDirection: 'row',
    padding: 12,
    borderRadius: 12,
    minHeight: 72,
    alignItems: 'center',
    gap: 12,
    borderWidth: 2,
    borderColor: 'transparent',
  },
  cardSelected: {
    borderColor: BRAND_COLOR,
  },
  // Slide action preview backgrounds (#170)
  doneBg: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'flex-start',
    paddingLeft: 20,
  },
  deleteBg: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'flex-end',
    paddingRight: 20,
  },
  pressed: {
    opacity: 0.8,
  },
  times: {
    width: 48,
    alignItems: 'center',
    gap: 4,
  },
  timeText: {
    fontSize: 12,
    fontVariant: ['tabular-nums'],
  },
  titleContainer: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  title: {
    fontSize: 16,
    fontWeight: '500',
    flex: 1,
  },
  taskId: {
    fontSize: 11,
    fontVariant: ['tabular-nums'],
  },
  depsCount: {
    fontSize: 11,
  },
  cost: {
    alignSelf: 'flex-end',
  },
  costText: {
    fontSize: 11,
    fontVariant: ['tabular-nums'],
  },
  deadlineHint: {
    fontSize: 10,
    fontVariant: ['tabular-nums'],
    textAlign: 'right',
    marginTop: 1,
  },
});

export const TaskCard = memo(TaskCardImpl);

// ── ParallelGroupCard ──
// Renders a parallel task group: host (allows_parallel) on the left (50%),
// guests (parallelizable) stacked on the right (50%).
// A single slide gesture applies to the whole group:
//   slide right → complete all, slide left → delete all (#194).

interface ParallelGroupCardProps {
  host: TaskRow;
  guests: TaskRow[];
  hostScheduleStart?: string;
  hostScheduleEnd?: string;
  guestScheduleStarts: (string | undefined)[];
  guestScheduleEnds: (string | undefined)[];
  isDone: boolean;
  selected?: boolean;
  onHostPress: () => void;
  onGuestPress: (index: number) => void;
  onLongPress: () => void;
  onDone?: () => void;
  onDelete?: () => void;
  // habit_id → display_id map for habit-based coloring (#309).
  habitDisplayIdMap?: Map<string, number>;
}

function ParallelGroupCardImpl({
  host,
  guests,
  hostScheduleStart,
  hostScheduleEnd,
  guestScheduleStarts,
  guestScheduleEnds,
  isDone,
  selected,
  onHostPress,
  onGuestPress,
  onLongPress,
  onDone,
  onDelete,
  habitDisplayIdMap,
}: ParallelGroupCardProps) {
  const translateX = useSharedValue(0);
  // Track which direction the haptic last fired for (0=none, 1=right, -1=left)
  // so reversing swipe direction mid-gesture re-fires the haptic (#313).
  const hapticFiredDir = useSharedValue(0);
  const { dark, colors } = useTheme();

  const pan = Gesture.Pan()
    .activeOffsetX([-10, 10])
    .failOffsetY([-10, 10])
    .onUpdate((e) => {
      translateX.value = e.translationX;
      // Fire haptic when crossing the action threshold mid-slide (#313).
      if (e.translationX > 80 && onDone && hapticFiredDir.value !== 1) {
        hapticFiredDir.value = 1;
        runOnJS(haptic.light)();
      } else if (
        e.translationX < -80 &&
        onDelete &&
        hapticFiredDir.value !== -1
      ) {
        hapticFiredDir.value = -1;
        runOnJS(haptic.medium)();
      }
    })
    .onEnd((e) => {
      if (e.translationX > 80 && onDone) {
        runOnJS(onDone)();
      } else if (e.translationX < -80 && onDelete) {
        runOnJS(onDelete)();
      }
      translateX.value = withSpring(0);
    })
    // onFinalize fires for both END and CANCELLED terminal states, ensuring
    // hapticFiredDir is always reset even if the gesture is interrupted.
    .onFinalize(() => {
      hapticFiredDir.value = 0;
    });

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: translateX.value }],
  }));
  const doneBgStyle = useAnimatedStyle(() => ({
    opacity: Math.min(1, Math.max(0, translateX.value / 80)),
  }));
  const deleteBgStyle = useAnimatedStyle(() => ({
    opacity: Math.min(1, Math.max(0, -translateX.value / 80)),
  }));

  const hostBgColor = taskCardColor(
    host.abandonability,
    host.habit_id,
    host.habit_id ? habitDisplayIdMap?.get(host.habit_id) : undefined,
    dark,
  );
  const hostDone = host.status === 'completed' || host.status === 'skipped';

  const handleHostPress = () => {
    haptic.light();
    onHostPress();
  };
  const handleLongPress = () => {
    haptic.medium();
    onLongPress();
  };

  return (
    <View style={groupStyles.container}>
      {/* Slide action preview backgrounds */}
      <Reanimated.View
        style={[
          styles.doneBg,
          { backgroundColor: isDone ? COLORS.red : COLORS.green },
          doneBgStyle,
        ]}
        pointerEvents="none"
      >
        <Ionicons
          name={isDone ? 'refresh' : 'checkmark'}
          size={28}
          color={COLORS.white}
        />
      </Reanimated.View>
      <Reanimated.View
        style={[
          styles.deleteBg,
          { backgroundColor: COLORS.red },
          deleteBgStyle,
        ]}
        pointerEvents="none"
      >
        <Ionicons name="trash" size={28} color={COLORS.white} />
      </Reanimated.View>

      <GestureDetector gesture={pan}>
        <Reanimated.View
          style={[
            groupStyles.groupContainer,
            selected && styles.cardSelected,
            animatedStyle,
          ]}
        >
          {/* Left: host (50%) */}
          <Pressable
            style={({ pressed }) => [
              groupStyles.hostCard,
              { backgroundColor: hostBgColor },
              pressed && styles.pressed,
            ]}
            onPress={handleHostPress}
            onLongPress={handleLongPress}
          >
            <Text style={[groupStyles.hostTime, { color: colors.grayDark }]}>
              {formatTime(hostScheduleStart)}
            </Text>
            <Text style={[groupStyles.hostTime, { color: colors.grayDark }]}>
              {formatTime(hostScheduleEnd)}
            </Text>
            <Text
              style={[
                groupStyles.hostTitle,
                { color: colors.black },
                hostDone && {
                  textDecorationLine: 'line-through',
                  color: colors.done,
                },
              ]}
              numberOfLines={4}
            >
              {host.title}
            </Text>
          </Pressable>

          {/* Right: guests stacked (50%) */}
          <View style={groupStyles.guestsColumn}>
            {guests.map((guest, idx) => {
              const guestDone =
                guest.status === 'completed' || guest.status === 'skipped';
              const guestBg = taskCardColor(
                guest.abandonability,
                guest.habit_id,
                guest.habit_id
                  ? habitDisplayIdMap?.get(guest.habit_id)
                  : undefined,
                dark,
              );
              const guestDeps = parseDepends(guest.depends);
              return (
                <Pressable
                  key={guest.id}
                  style={({ pressed }) => [
                    groupStyles.guestCard,
                    { backgroundColor: guestBg },
                    idx === guests.length - 1 && groupStyles.guestCardLast,
                    pressed && styles.pressed,
                  ]}
                  onPress={() => {
                    haptic.light();
                    onGuestPress(idx);
                  }}
                >
                  <View style={groupStyles.guestTimes}>
                    <Text
                      style={[
                        groupStyles.guestTime,
                        { color: colors.grayDark },
                      ]}
                    >
                      {formatTime(guestScheduleStarts[idx])}
                    </Text>
                    <Text
                      style={[
                        groupStyles.guestTime,
                        { color: colors.grayDark },
                      ]}
                    >
                      {formatTime(guestScheduleEnds[idx])}
                    </Text>
                  </View>
                  <View style={groupStyles.guestTitleContainer}>
                    <Text
                      style={[groupStyles.guestTaskId, { color: colors.gray }]}
                    >
                      {guest.habit_id &&
                      habitDisplayIdMap?.get(guest.habit_id) !== undefined
                        ? `h${habitDisplayIdMap.get(guest.habit_id)}#${guest.display_id}`
                        : `#${guest.display_id}`}
                    </Text>
                    <Text
                      style={[
                        groupStyles.guestTitle,
                        { color: colors.black },
                        guestDone && {
                          textDecorationLine: 'line-through',
                          color: colors.done,
                        },
                      ]}
                      numberOfLines={2}
                    >
                      {guest.title}
                    </Text>
                    {guestDeps.length > 0 && (
                      <Text
                        style={[groupStyles.guestDeps, { color: colors.gray }]}
                      >
                        ↳ {guestDeps.length}
                      </Text>
                    )}
                  </View>
                  <View style={groupStyles.guestCost}>
                    <Text
                      style={[
                        groupStyles.guestCostText,
                        { color: colors.gray },
                      ]}
                    >
                      {guest.avg_minutes}m
                    </Text>
                  </View>
                </Pressable>
              );
            })}
          </View>
        </Reanimated.View>
      </GestureDetector>
    </View>
  );
}

const groupStyles = StyleSheet.create({
  container: {
    marginHorizontal: 12,
    marginVertical: 4,
    position: 'relative',
  },
  groupContainer: {
    flexDirection: 'row',
    alignItems: 'stretch',
    borderRadius: 12,
    overflow: 'hidden',
    minHeight: 72,
    borderWidth: 2,
    borderColor: 'transparent',
  },
  hostCard: {
    flex: 1,
    padding: 6,
    justifyContent: 'center',
    gap: 2,
    alignSelf: 'stretch',
  },
  hostTime: {
    fontSize: 10,
    fontVariant: ['tabular-nums'],
  },
  hostTitle: {
    fontSize: 11,
    fontWeight: '500',
    marginTop: 2,
  },
  guestsColumn: {
    flex: 1,
    flexDirection: 'column',
  },
  guestCard: {
    flex: 1,
    flexDirection: 'row',
    padding: 8,
    alignItems: 'center',
    gap: 8,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: 'rgba(0,0,0,0.08)',
    minHeight: 48,
  },
  guestCardLast: {
    borderBottomWidth: 0,
  },
  guestTimes: {
    width: 36,
    alignItems: 'center',
  },
  guestTime: {
    fontSize: 10,
    fontVariant: ['tabular-nums'],
  },
  guestTitleContainer: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  guestTaskId: {
    fontSize: 9,
    fontVariant: ['tabular-nums'],
  },
  guestTitle: {
    fontSize: 13,
    fontWeight: '500',
    flex: 1,
  },
  guestDeps: {
    fontSize: 9,
  },
  guestCost: {
    alignSelf: 'flex-end',
  },
  guestCostText: {
    fontSize: 10,
    fontVariant: ['tabular-nums'],
  },
});

export const ParallelGroupCard = memo(ParallelGroupCardImpl);
