// TaskAddSheet — bottom-sheet overlay that wraps TaskAddView.
//
// The sheet's vertical position is driven by a Reanimated shared value
// (`sheetY`) that the AddButton writes to during an upward drag.  While the
// sheet is in preview mode (`open === false`) the content is rendered but
// non-interactive (pointerEvents="none") so the user can see the form
// filling in from the bottom without being able to focus inputs mid-drag.
// Once the parent sets `open` to true the sheet becomes fully interactive.
//
// Closing gesture: dragging the grabber handle downward (top→bottom) slides
// the sheet back towards the bottom of the screen.  Releasing past the
// commit threshold dismisses the sheet; otherwise it springs back open.
// This is the inverse of the AddButton's upward slide-to-open gesture.

import { Pressable, StyleSheet, View } from 'react-native';
import { GestureDetector, Gesture } from 'react-native-gesture-handler';
import Reanimated, {
  useAnimatedStyle,
  useSharedValue,
  withTiming,
  runOnJS,
  type SharedValue,
} from 'react-native-reanimated';
import { TaskAddView } from '@/src/views/TaskAddView';
import { BRAND_COLOR, useColors } from '@/src/theme';

/** Drag distance (px) past which a release commits the sheet to closed. */
const CLOSE_COMMIT_THRESHOLD = 80;

interface TaskAddSheetProps {
  /** Shared translateY for the sheet. `screenHeight` = hidden, `0` = fully open. */
  sheetY: SharedValue<number>;
  /** Screen height in px (sheet height matches the screen). */
  screenHeight: number;
  /** When true the sheet content + scrim are interactive. */
  open: boolean;
  /** Called when the user requests closing (back button, scrim tap, or save). */
  onClose: () => void;
  /** Optional pre-selected dependency IDs forwarded to TaskAddView. */
  initialDeps?: string[];
}

export function TaskAddSheet({
  sheetY,
  screenHeight,
  open,
  onClose,
  initialDeps,
}: TaskAddSheetProps) {
  const colors = useColors();

  // Starting translateY captured at gesture begin (normally 0 when open).
  const startY = useSharedValue(0);

  const sheetStyle = useAnimatedStyle(() => ({
    transform: [{ translateY: sheetY.value }],
  }));

  // Scrim fades in as the sheet reveals.
  const scrimStyle = useAnimatedStyle(() => {
    const revealed = screenHeight - sheetY.value;
    const opacity = Math.min(0.4, (revealed / screenHeight) * 0.5);
    return { opacity };
  });

  // Downward drag on the grabber handle drives the sheet towards the bottom.
  // Only active while the sheet is fully open so it doesn't fight the
  // AddButton's upward reveal gesture during preview.
  const closeGesture = Gesture.Pan()
    .enabled(open)
    .activeOffsetY(10)
    .failOffsetX([-20, 20])
    .onBegin(() => {
      startY.value = sheetY.value;
    })
    .onUpdate((e) => {
      // Only allow downward movement (positive translationY) so an upward
      // flick can't push the sheet past the top of the screen.
      sheetY.value = Math.max(0, Math.min(screenHeight, startY.value + e.translationY));
    })
    .onEnd((e) => {
      if (e.translationY > CLOSE_COMMIT_THRESHOLD) {
        sheetY.value = withTiming(screenHeight, { duration: 200 });
        runOnJS(onClose)();
      } else {
        sheetY.value = withTiming(0, { duration: 200 });
      }
    });

  return (
    <View
      style={[StyleSheet.absoluteFill, { zIndex: 20 }]}
      pointerEvents={open ? 'auto' : 'none'}
    >
      {/* Scrim — tap to close (only when open) */}
      <Pressable
        style={StyleSheet.absoluteFill}
        onPress={open ? onClose : undefined}
      >
        <Reanimated.View
          style={[StyleSheet.absoluteFill, { backgroundColor: '#000' }, scrimStyle]}
        />
      </Pressable>

      {/* Sheet — full-height panel sliding from the bottom */}
      <Reanimated.View
        style={[
          styles.sheet,
          { height: screenHeight, backgroundColor: colors.white },
          sheetStyle,
        ]}
      >
        {/* Grabber handle — drag down to close (inverse of slide-up to open) */}
        <GestureDetector gesture={closeGesture}>
          <View style={styles.handleContainer}>
            <View style={[styles.handle, { backgroundColor: colors.grayLight }]} />
          </View>
        </GestureDetector>

        <TaskAddView onClose={onClose} initialDeps={initialDeps} />
      </Reanimated.View>
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    borderRadius: 16,
    overflow: 'hidden',
    // Subtle shadow separating the sheet from the content beneath.
    shadowColor: '#000',
    shadowOffset: { width: 0, height: -2 },
    shadowOpacity: 0.15,
    shadowRadius: 8,
    elevation: 8,
  },
  handleContainer: {
    alignItems: 'center',
    paddingVertical: 12,
    minHeight: 32,
  },
  handle: {
    width: 36,
    height: 4,
    borderRadius: 2,
  },
});
