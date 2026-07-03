// AddButton — floating add button with gesture support
// Slide up (bottom→top): opens task add view (via bottom-sheet preview)
// Tap: shows "Assistant未実装" message (Assistant is planned for later)
//
// During an upward drag the button writes the live translation into `sheetY`
// (a Reanimated shared value owned by the parent) so a TaskAddSheet can
// reveal itself from the bottom of the screen in lock-step with the finger.
// On release the parent decides whether to commit (sheet fully opens) or
// cancel (sheet animates closed) via `onDragEnd`.

import { Alert, Pressable, StyleSheet } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { GestureDetector, Gesture } from 'react-native-gesture-handler';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  runOnJS,
  withSpring,
  type SharedValue,
} from 'react-native-reanimated';
import { BRAND_COLOR, COLORS } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

/** Multiplier: 1px of finger drag → DRAG_SCALE px of sheet reveal. */
const DRAG_SCALE = 4;
/** Drag distance (px) past which a release commits the sheet to full open. */
const COMMIT_THRESHOLD = 60;

interface AddButtonProps {
  /** Called when the user releases past the commit threshold. */
  onSlideUp: () => void;
  /** Shared value the button writes the sheet's translateY into during drag.
   *  Parent should initialise this to `screenHeight` (sheet hidden). */
  sheetY: SharedValue<number>;
  /** Screen height in px — used to compute the sheet position from the drag. */
  screenHeight: number;
  /** Fired when a drag begins (sheet should mount in preview mode). */
  onDragStart?: () => void;
  /** Fired when a drag ends. `committed` is true when the drag passed the
   *  threshold. The parent animates `sheetY` accordingly. */
  onDragEnd?: (committed: boolean) => void;
}

export function AddButton({
  onSlideUp,
  sheetY,
  screenHeight,
  onDragStart,
  onDragEnd,
}: AddButtonProps) {
  // The button's own visual offset (follows finger, springs back on release).
  const buttonY = useSharedValue(0);

  const panGesture = Gesture.Pan()
    .onBegin(() => {
      runOnJS(onDragStart ?? (() => {}))();
    })
    .onUpdate((e) => {
      // Only allow upward movement for the button itself.
      buttonY.value = Math.min(0, e.translationY);
      // Sheet reveals from the bottom: as the finger moves up (negative
      // translationY) the sheet's translateY decreases from screenHeight.
      // Clamp at 0 so a long drag can't push the sheet past the top of
      // the screen (which would leave a gap at the bottom).
      sheetY.value = Math.max(0, screenHeight + e.translationY * DRAG_SCALE);
    })
    .onEnd((e) => {
      const committed = e.translationY < -COMMIT_THRESHOLD;
      if (committed) {
        runOnJS(haptic.light)();
        runOnJS(onSlideUp)();
      }
      runOnJS(onDragEnd ?? (() => {}))(committed);
      // Button springs back to rest; the parent animates sheetY separately.
      buttonY.value = withSpring(0);
    })
    // onEnd only fires when the gesture was ACTIVE (finger moved past the
    // activation distance).  A pure tap goes BEGAN → FAILED, skipping onEnd,
    // so onFinalize cleans up the sheet mount in that case.  When the gesture
    // succeeded, onFinalize fires with success=true and the guard skips it
    // (onEnd already handled the cleanup).
    .onFinalize((_e, success) => {
      if (!success) {
        runOnJS(onDragEnd ?? (() => {}))(false);
        // Reset the button's visual offset — onEnd is skipped on cancel so
        // the spring-back there never runs.  Without this the button stays
        // visually displaced after a cancelled drag.
        buttonY.value = withSpring(0);
      }
    });

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateY: buttonY.value }],
  }));

  function handleTap() {
    haptic.light();
    Alert.alert('未実装', '音声アシスタントは今後実装予定です');
  }

  return (
    <GestureDetector gesture={panGesture}>
      <Reanimated.View style={[styles.container, animatedStyle]}>
        <Pressable style={styles.button} onPress={handleTap}>
          <Ionicons name="add" size={28} color={COLORS.white} />
        </Pressable>
      </Reanimated.View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  container: {
    width: 56,
    height: 56,
    borderRadius: 28,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
    elevation: 4,
  },
  button: {
    width: '100%',
    height: '100%',
    borderRadius: 28,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
