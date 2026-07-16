// TaskCard component — displays a single task in the list
// Left: start/end time, Center: title, Right-bottom: cost (avg, sigma)
// Background color based on abandonability
// Slide right cycles: start → complete → revert (#312)
// Slide left reveals a delete button (two-step delete #393)
// Slide actions show a background preview with icon
// Done tasks: strikethrough + gray

import { memo, useState } from 'react';
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  type ViewStyle,
} from 'react-native';
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
  onDone?: () => void | Promise<void>;
  onDelete?: () => void | Promise<void>;
  onLongPress?: () => void;
  selected?: boolean;
  // Habit display_id for habit-based coloring (#309). Undefined when the
  // task has no habit or the habit map is unavailable.
  habitDisplayId?: number;
  // Number of tasks that depend on this task (reverse dependencies).
  dependentCount?: number;
  // Optional override for the outer container (e.g. to remove margins in a group).
  containerStyle?: ViewStyle;
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
  dependentCount,
  containerStyle,
}: TaskCardProps) {
  const translateX = useSharedValue(0);
  // Track which direction the haptic last fired for (0=none, 1=right, -1=left)
  // so reversing swipe direction mid-gesture re-fires the haptic (#313).
  const hapticFiredDir = useSharedValue(0);
  const { dark, colors } = useTheme();
  // #393: two-step delete — swipe left reveals a delete button instead of
  // deleting immediately. When revealed, tapping the card snaps it back.
  // Use a SharedValue for the UI-thread worklet logic (avoids stale React
  // state in gesture callbacks) and mirror to React state for rendering.
  const deleteRevealedSV = useSharedValue(false);
  const [deleteRevealed, setDeleteRevealed] = useState(false);

  // Width of the revealed delete button (used to keep the card offset).
  const DELETE_REVEAL_WIDTH = 72;

  // Single pan gesture handles both swipe-right (done) and swipe-left (delete).
  // Using Gesture.Race with two separate pans was unreliable for left swipe
  // (#230): Race resolution between gestures with activeOffsetX in opposite
  // directions can fail to activate. A single gesture with bidirectional
  // activeOffsetX avoids the issue entirely.
  const pan = Gesture.Pan()
    .activeOffsetX([-10, 10])
    .failOffsetY([-10, 10])
    .onUpdate((e) => {
      // If already revealed, start from the revealed position.
      const base = deleteRevealedSV.value ? -DELETE_REVEAL_WIDTH : 0;
      translateX.value = base + e.translationX;
      // Fire haptic when crossing the action threshold mid-slide (#313).
      // Suppress haptics when delete is revealed — no action will fire
      // regardless of swipe direction (#393).
      if (
        e.translationX > 80 &&
        onDone &&
        hapticFiredDir.value !== 1 &&
        !deleteRevealedSV.value
      ) {
        hapticFiredDir.value = 1;
        runOnJS(haptic.light)();
      } else if (
        e.translationX < -80 &&
        onDelete &&
        hapticFiredDir.value !== -1 &&
        !deleteRevealedSV.value
      ) {
        hapticFiredDir.value = -1;
        runOnJS(haptic.medium)();
      }
    })
    .onEnd((e) => {
      if (deleteRevealedSV.value) {
        // When delete is revealed, any right swipe just hides it (#393).
        // Don't trigger onDone even if the swipe passes the threshold.
        if (e.translationX > -20) {
          deleteRevealedSV.value = false;
          runOnJS(setDeleteRevealed)(false);
          translateX.value = withSpring(0);
        } else {
          translateX.value = withSpring(-DELETE_REVEAL_WIDTH);
        }
      } else if (e.translationX > 80 && onDone) {
        runOnJS(onDone)();
        translateX.value = withSpring(0);
      } else if (e.translationX < -80 && onDelete) {
        // #393: reveal the delete button instead of deleting immediately.
        deleteRevealedSV.value = true;
        runOnJS(setDeleteRevealed)(true);
        translateX.value = withSpring(-DELETE_REVEAL_WIDTH);
      } else {
        translateX.value = withSpring(0);
      }
    })
    // onFinalize fires for both END and CANCELLED terminal states, ensuring
    // hapticFiredDir is always reset even if the gesture is interrupted.
    // Only snap to resting position when the gesture was cancelled (not
    // when it ended normally — onEnd already handles that) to avoid
    // restarting the spring animation (#393).
    .onFinalize((_e, success) => {
      hapticFiredDir.value = 0;
      if (!success) {
        translateX.value = withSpring(
          deleteRevealedSV.value ? -DELETE_REVEAL_WIDTH : 0,
        );
      }
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
    if (deleteRevealed) {
      // Tapping the card when delete is revealed snaps it back (#393).
      haptic.light();
      deleteRevealedSV.value = false;
      setDeleteRevealed(false);
      translateX.value = withSpring(0);
      return;
    }
    haptic.light();
    onPress();
  };
  const handleLongPress = onLongPress
    ? () => {
        if (deleteRevealed) {
          haptic.light();
          deleteRevealedSV.value = false;
          setDeleteRevealed(false);
          translateX.value = withSpring(0);
          return;
        }
        haptic.medium();
        onLongPress();
      }
    : undefined;

  return (
    <View style={[styles.container, containerStyle]}>
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
      {/* #393: revealed delete button — tap to confirm deletion */}
      {deleteRevealed && onDelete && (
        <Pressable
          style={styles.deleteButton}
          onPress={() => {
            haptic.medium();
            deleteRevealedSV.value = false;
            setDeleteRevealed(false);
            translateX.value = withSpring(0);
            onDelete();
          }}
        >
          <Ionicons name="trash" size={24} color={COLORS.white} />
        </Pressable>
      )}
      <GestureDetector gesture={pan}>
        <Reanimated.View
          style={[styles.card, { backgroundColor: bgColor }, animatedStyle]}
        >
          <Pressable
            style={({ pressed }) => [
              styles.cardInner,
              pressed && styles.pressed,
              selected && styles.cardSelected,
              isInProgress && {
                borderLeftColor: BRAND_COLOR,
                borderLeftWidth: 4,
              },
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
            </View>

            {/* Right: deps, dependents, and cost stacked vertically */}
            <View style={styles.meta}>
              {deps.length > 0 && (
                <Text style={[styles.metaText, { color: colors.gray }]}>
                  ↳ {deps.length}
                </Text>
              )}
              {(dependentCount ?? 0) > 0 && (
                <Text style={[styles.metaText, { color: colors.gray }]}>
                  ↗ {dependentCount}
                </Text>
              )}
              {task.avg_minutes > 0 && (
                <Text style={[styles.metaText, { color: colors.gray }]}>
                  {task.avg_minutes}m ±{task.sigma_minutes}
                </Text>
              )}
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
  // #393: revealed delete button positioned on the right edge
  deleteButton: {
    position: 'absolute',
    top: 0,
    right: 0,
    bottom: 0,
    width: 72,
    backgroundColor: COLORS.red,
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'center',
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
  meta: {
    alignSelf: 'stretch',
    justifyContent: 'center',
    alignItems: 'flex-end',
    gap: 1,
  },
  metaText: {
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
// Renders a parallel task group as a rotated "L": host on top, a thin
// vertical rail (same color as the host) extending down, and guests
// indented on the right as normal TaskCards. Each task keeps its own
// 3-state swipe gesture; the rail is static and does not move (#573).

const RAIL_WIDTH = 10;
const OUTLINE_WIDTH = 1;
const INDENT_WIDTH = RAIL_WIDTH + OUTLINE_WIDTH;

interface ParallelGroupCardProps {
  host: TaskRow;
  guests: TaskRow[];
  hostScheduleStart?: string;
  hostScheduleEnd?: string;
  guestScheduleStarts: (string | undefined)[];
  guestScheduleEnds: (string | undefined)[];
  selected?: boolean;
  onHostPress: () => void;
  onGuestPress: (index: number) => void;
  onLongPress: () => void;
  onDone?: (task: TaskRow) => void | Promise<void>;
  onDelete?: (task: TaskRow) => void | Promise<void>;
  // habit_id → display_id map for habit-based coloring (#309).
  habitDisplayIdMap?: Map<string, number>;
  // task_id → number of tasks that depend on it (reverse dependency count).
  dependentCountMap?: Map<string, number>;
}

function ParallelGroupCardImpl({
  host,
  guests,
  hostScheduleStart,
  hostScheduleEnd,
  guestScheduleStarts,
  guestScheduleEnds,
  selected,
  onHostPress,
  onGuestPress,
  onLongPress,
  onDone,
  onDelete,
  habitDisplayIdMap,
  dependentCountMap,
}: ParallelGroupCardProps) {
  const { dark } = useTheme();
  const hostHabitDisplayId = host.habit_id
    ? habitDisplayIdMap?.get(host.habit_id)
    : undefined;
  const hostBgColor = taskCardColor(
    host.abandonability,
    host.habit_id,
    hostHabitDisplayId,
    dark,
  );
  const outlineColor = dark ? 'rgba(255,255,255,0.10)' : 'rgba(0,0,0,0.08)';

  return (
    <View
      style={[groupStyles.container, selected && { borderColor: BRAND_COLOR }]}
    >
      <View
        style={[
          groupStyles.rail,
          { backgroundColor: hostBgColor, borderRightColor: outlineColor },
        ]}
      />
      <View style={groupStyles.cards}>
        <TaskCard
          task={host}
          scheduleStart={hostScheduleStart}
          scheduleEnd={hostScheduleEnd}
          isDone={host.status === 'completed' || host.status === 'skipped'}
          onPress={onHostPress}
          onDone={onDone ? () => onDone(host) : undefined}
          onDelete={onDelete ? () => onDelete(host) : undefined}
          onLongPress={onLongPress}
          habitDisplayId={hostHabitDisplayId}
          dependentCount={dependentCountMap?.get(host.id)}
          containerStyle={groupStyles.groupCard}
        />
        {guests.map((guest, idx) => {
          const guestHabitDisplayId = guest.habit_id
            ? habitDisplayIdMap?.get(guest.habit_id)
            : undefined;
          return (
            <TaskCard
              key={guest.id}
              task={guest}
              scheduleStart={guestScheduleStarts[idx]}
              scheduleEnd={guestScheduleEnds[idx]}
              isDone={
                guest.status === 'completed' || guest.status === 'skipped'
              }
              onPress={() => onGuestPress(idx)}
              onDone={onDone ? () => onDone(guest) : undefined}
              onDelete={onDelete ? () => onDelete(guest) : undefined}
              onLongPress={onLongPress}
              habitDisplayId={guestHabitDisplayId}
              dependentCount={dependentCountMap?.get(guest.id)}
              containerStyle={groupStyles.groupCard}
            />
          );
        })}
      </View>
    </View>
  );
}

const groupStyles = StyleSheet.create({
  container: {
    marginHorizontal: 12,
    marginVertical: 4,
    borderTopLeftRadius: 6,
    borderTopRightRadius: 12,
    borderBottomLeftRadius: 6,
    borderBottomRightRadius: 12,
    overflow: 'hidden',
    borderWidth: 2,
    borderColor: 'transparent',
    position: 'relative',
    minHeight: 72,
  },
  rail: {
    position: 'absolute',
    left: 0,
    top: 0,
    bottom: 0,
    width: INDENT_WIDTH,
    borderTopLeftRadius: 4,
    borderTopRightRadius: 4,
    borderBottomLeftRadius: 4,
    borderBottomRightRadius: 4,
    borderRightWidth: OUTLINE_WIDTH,
  },
  cards: {
    flexDirection: 'column',
  },
  groupCard: {
    marginHorizontal: 0,
    marginVertical: 0,
    marginLeft: INDENT_WIDTH,
  },
});

export const ParallelGroupCard = memo(ParallelGroupCardImpl);
