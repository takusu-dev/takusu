// TaskCard component — displays a single task in the list
// Left: start/end time, Center: title, Right-bottom: cost (avg, sigma)
// Background color based on abandonability
// Slide right = done (weak haptics), slide left = delete (strong haptics)
// Done tasks: strikethrough + gray
// allows_parallel: shows receiver task on left (1:3 width ratio)

import { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import * as Haptics from 'expo-haptics';
import { GestureDetector, Gesture } from 'react-native-gesture-handler';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  runOnJS,
  withSpring,
} from 'react-native-reanimated';
import type { TaskRow } from '@/src/api/types';
import { parseDepends } from '@/src/api/types';
import { abandonabilityColor, COLORS } from '@/src/theme';

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
}

function formatTime(iso?: string): string {
  if (!iso) return '--:--';
  const d = new Date(iso);
  return `${d.getHours().toString().padStart(2, '0')}:${d
    .getMinutes()
    .toString()
    .padStart(2, '0')}`;
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
}: TaskCardProps) {
  const translateX = useSharedValue(0);

  const flingRight = Gesture.Pan()
    .onStart(() => {})
    .onUpdate((e) => {
      translateX.value = Math.max(0, e.translationX);
    })
    .onEnd((e) => {
      if (e.translationX > 80 && onDone) {
        runOnJS(Haptics.impactAsync)(Haptics.ImpactFeedbackStyle.Light);
        runOnJS(onDone)();
      }
      translateX.value = withSpring(0);
    });

  const flingLeft = Gesture.Pan()
    .onStart(() => {})
    .onUpdate((e) => {
      translateX.value = Math.min(0, e.translationX);
    })
    .onEnd((e) => {
      if (e.translationX < -80 && onDelete) {
        runOnJS(Haptics.impactAsync)(Haptics.ImpactFeedbackStyle.Medium);
        runOnJS(onDelete)();
      }
      translateX.value = withSpring(0);
    });

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: translateX.value }],
  }));

  const bgColor = abandonabilityColor(task.abandonability);
  const deps = parseDepends(task.depends);

  return (
    <GestureDetector gesture={Gesture.Race(flingRight, flingLeft)}>
      <Reanimated.View style={[styles.container, animatedStyle]}>
        <Pressable
          style={({ pressed }) => [
            styles.card,
            { backgroundColor: bgColor },
            pressed && styles.pressed,
          ]}
          onPress={onPress}
          onLongPress={onLongPress}
        >
          {/* Left: times */}
          <View style={styles.times}>
            <Text style={styles.timeText}>{formatTime(scheduleStart)}</Text>
            <Text style={styles.timeText}>{formatTime(scheduleEnd)}</Text>
          </View>

          {/* Center: title */}
          <View style={styles.titleContainer}>
            <Text
              style={[
                styles.title,
                isDone && { textDecorationLine: 'line-through', color: COLORS.done },
              ]}
              numberOfLines={2}
            >
              {task.title}
            </Text>
            {deps.length > 0 && (
              <Text style={styles.depsCount}>↳ {deps.length} deps</Text>
            )}
            {selected && <Text style={styles.selectedIndicator}>✓</Text>}
          </View>

          {/* Right-bottom: cost */}
          <View style={styles.cost}>
            <Text style={styles.costText}>
              {task.avg_minutes}m ±{task.sigma_minutes}
            </Text>
          </View>
        </Pressable>
      </Reanimated.View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  container: {
    marginHorizontal: 12,
    marginVertical: 4,
  },
  card: {
    flexDirection: 'row',
    padding: 12,
    borderRadius: 12,
    minHeight: 72,
    alignItems: 'center',
    gap: 12,
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
    color: COLORS.grayDark,
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
    color: COLORS.black,
  },
  depsCount: {
    fontSize: 11,
    color: COLORS.gray,
  },
  selectedIndicator: {
    fontSize: 16,
    color: COLORS.brand,
    fontWeight: 'bold',
  },
  cost: {
    alignSelf: 'flex-end',
  },
  costText: {
    fontSize: 11,
    color: COLORS.gray,
    fontVariant: ['tabular-nums'],
  },
});

export const TaskCard = memo(TaskCardImpl);
