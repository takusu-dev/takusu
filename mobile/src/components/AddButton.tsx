// AddButton — floating add button with gesture support
// Slide up (bottom→top): opens task add view
// Tap: shows "Assistant未実装" message (Assistant is planned for later)

import { Alert, Pressable, StyleSheet } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { GestureDetector, Gesture } from 'react-native-gesture-handler';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  runOnJS,
  withSpring,
} from 'react-native-reanimated';
import { BRAND_COLOR, COLORS } from '@/src/theme';

interface AddButtonProps {
  onSlideUp: () => void;
}

export function AddButton({ onSlideUp }: AddButtonProps) {
  const translateY = useSharedValue(0);

  const panGesture = Gesture.Pan()
    .onUpdate((e) => {
      // Only allow upward movement
      translateY.value = Math.min(0, e.translationY);
    })
    .onEnd((e) => {
      if (e.translationY < -60) {
        runOnJS(onSlideUp)();
      }
      translateY.value = withSpring(0);
    });

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateY: translateY.value }],
  }));

  function handleTap() {
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
