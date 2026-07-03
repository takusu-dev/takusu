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
import { abandonabilityColorFor, BRAND_COLOR, useTheme } from '@/src/theme';

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
  // Receiver task (allows_parallel=true) that overlaps in schedule time
  parallelTask?: TaskRow;
  parallelScheduleStart?: string;
  parallelScheduleEnd?: string;
  onParallelPress?: () => void;
  onParallelDone?: () => void;
  onParallelDelete?: () => void;
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
  parallelTask,
  parallelScheduleStart,
  parallelScheduleEnd,
  onParallelPress,
  onParallelDone,
  onParallelDelete,
}: TaskCardProps) {
  const translateX = useSharedValue(0);
  const { dark, colors } = useTheme();

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

  const bgColor = abandonabilityColorFor(task.abandonability, dark);
  const deps = parseDepends(task.depends);

  // Parallel receiver task (left side, 1:3 width ratio)
  if (parallelTask) {
    const parallelBgColor = abandonabilityColorFor(parallelTask.abandonability, dark);
    const parallelDone =
      parallelTask.status === 'completed' || parallelTask.status === 'skipped';

    return (
      <View style={styles.container}>
        <View style={styles.parallelContainer}>
          {/* Left: receiver task (1:3 ratio → 25%) */}
          <GestureDetector
            gesture={Gesture.Race(
              Gesture.Pan()
                .onUpdate((e) => {
                  translateX.value = Math.max(0, e.translationX);
                })
                .onEnd((e) => {
                  if (e.translationX > 80 && onParallelDone) {
                    runOnJS(Haptics.impactAsync)(Haptics.ImpactFeedbackStyle.Light);
                    runOnJS(onParallelDone)();
                  }
                  translateX.value = withSpring(0);
                }),
              Gesture.Pan()
                .onUpdate((e) => {
                  translateX.value = Math.min(0, e.translationX);
                })
                .onEnd((e) => {
                  if (e.translationX < -80 && onParallelDelete) {
                    runOnJS(Haptics.impactAsync)(Haptics.ImpactFeedbackStyle.Medium);
                    runOnJS(onParallelDelete)();
                  }
                  translateX.value = withSpring(0);
                }),
            )}
          >
            <Reanimated.View style={[styles.parallelCard, { backgroundColor: parallelBgColor }, animatedStyle]}>
              <Pressable
                onPress={onParallelPress}
                style={styles.parallelPressable}
              >
                <Text
                  style={[
                    styles.parallelTitle,
                    { color: colors.black },
                    parallelDone && {
                      textDecorationLine: 'line-through',
                      color: colors.done,
                    },
                  ]}
                  numberOfLines={3}
                >
                  {parallelTask.title}
                </Text>
                <Text style={[styles.parallelTime, { color: colors.grayDark }]}>
                  {formatTime(parallelScheduleStart)}
                </Text>
              </Pressable>
            </Reanimated.View>
          </GestureDetector>

          {/* Right: main task (3:4 ratio → 75%) */}
          <Pressable
            style={({ pressed }) => [
              styles.mainCard,
              { backgroundColor: bgColor },
              pressed && styles.pressed,
            ]}
            onPress={onPress}
            onLongPress={onLongPress}
          >
            <View style={styles.times}>
              <Text style={[styles.timeText, { color: colors.grayDark }]}>{formatTime(scheduleStart)}</Text>
              <Text style={[styles.timeText, { color: colors.grayDark }]}>{formatTime(scheduleEnd)}</Text>
            </View>
            <View style={styles.titleContainer}>
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
                <Text style={[styles.depsCount, { color: colors.gray }]}>↳ {deps.length} deps</Text>
              )}
              {selected && <Text style={styles.selectedIndicator}>✓</Text>}
            </View>
            <View style={styles.cost}>
              <Text style={[styles.costText, { color: colors.gray }]}>
                {task.avg_minutes}m ±{task.sigma_minutes}
              </Text>
            </View>
          </Pressable>
        </View>
      </View>
    );
  }

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
            <Text style={[styles.timeText, { color: colors.grayDark }]}>{formatTime(scheduleStart)}</Text>
            <Text style={[styles.timeText, { color: colors.grayDark }]}>{formatTime(scheduleEnd)}</Text>
          </View>

          {/* Center: title */}
          <View style={styles.titleContainer}>
            <Text
              style={[
                styles.title,
                { color: colors.black },
                isDone && { textDecorationLine: 'line-through', color: colors.done },
              ]}
              numberOfLines={2}
            >
              {task.title}
            </Text>
            {deps.length > 0 && (
              <Text style={[styles.depsCount, { color: colors.gray }]}>↳ {deps.length} deps</Text>
            )}
            {selected && <Text style={styles.selectedIndicator}>✓</Text>}
          </View>

          {/* Right-bottom: cost */}
          <View style={styles.cost}>
            <Text style={[styles.costText, { color: colors.gray }]}>
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
  parallelContainer: {
    flexDirection: 'row',
    borderRadius: 12,
    overflow: 'hidden',
    minHeight: 72,
  },
  parallelCard: {
    width: '25%',
    padding: 6,
    borderRadius: 0,
  },
  parallelPressable: {
    flex: 1,
    justifyContent: 'center',
    gap: 2,
  },
  parallelTitle: {
    fontSize: 11,
    fontWeight: '500',
  },
  parallelTime: {
    fontSize: 10,
    fontVariant: ['tabular-nums'],
  },
  mainCard: {
    flex: 1,
    flexDirection: 'row',
    padding: 12,
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
  depsCount: {
    fontSize: 11,
  },
  selectedIndicator: {
    fontSize: 16,
    color: BRAND_COLOR,
    fontWeight: 'bold',
  },
  cost: {
    alignSelf: 'flex-end',
  },
  costText: {
    fontSize: 11,
    fontVariant: ['tabular-nums'],
  },
});

export const TaskCard = memo(TaskCardImpl);
