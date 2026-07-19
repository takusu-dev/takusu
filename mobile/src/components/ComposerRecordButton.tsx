import { useCallback, useEffect, useRef, useState } from 'react';
import { StyleSheet } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Reanimated, {
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  runOnJS,
} from 'react-native-reanimated';
import { startRecording, stopAndTranscribe } from '@/src/utils/voice';
import { BRAND_COLOR, COLORS } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

const LOCK_THRESHOLD = 80;
const CANCEL_THRESHOLD = 80;

interface ComposerRecordButtonProps {
  audioReady: boolean;
  historyReady: boolean;
  busy: boolean;
  onAppend: (transcript: string) => void;
}

export function ComposerRecordButton({
  audioReady,
  historyReady,
  busy,
  onAppend,
}: ComposerRecordButtonProps) {
  const [isRecording, setIsRecording] = useState(false);

  const recordingRef = useRef(false);
  const willCancelRef = useRef(false);
  const willLockRef = useRef(false);

  const translateX = useSharedValue(0);
  const translateY = useSharedValue(0);
  const willCancelShared = useSharedValue(false);
  const willLockShared = useSharedValue(false);
  const isLockedShared = useSharedValue(false);

  useEffect(() => {
    recordingRef.current = isRecording;
  }, [isRecording]);

  const reset = useCallback(() => {
    recordingRef.current = false;
    willCancelRef.current = false;
    willLockRef.current = false;
    isLockedShared.value = false;
    willCancelShared.value = false;
    willLockShared.value = false;
    setIsRecording(false);
    translateX.value = withSpring(0);
    translateY.value = withSpring(0);
  }, [
    isLockedShared,
    translateX,
    translateY,
    willCancelShared,
    willLockShared,
  ]);

  const stopAndAppend = useCallback(async () => {
    if (!recordingRef.current) return;
    recordingRef.current = false;
    try {
      const transcript = await stopAndTranscribe();
      if (transcript) onAppend(transcript);
    } catch {
      // ignore
    } finally {
      reset();
    }
  }, [onAppend, reset]);

  const discardCurrent = useCallback(async () => {
    if (!recordingRef.current) return;
    recordingRef.current = false;
    try {
      await stopAndTranscribe();
    } catch {
      // ignore
    } finally {
      reset();
    }
  }, [reset]);

  const beginRecording = useCallback(() => {
    if (busy || !historyReady || !audioReady) return;
    recordingRef.current = true;
    startRecording().then(
      () => setIsRecording(true),
      () => {
        recordingRef.current = false;
        setIsRecording(false);
      },
    );
  }, [audioReady, busy, historyReady]);

  const handleBegin = useCallback(() => {
    willCancelRef.current = false;
    willLockRef.current = false;
    willCancelShared.value = false;
    willLockShared.value = false;
    if (recordingRef.current) {
      // Already locked: tapping the close icon stops and appends.
      if (isLockedShared.value) {
        stopAndAppend();
      }
      return;
    }
    beginRecording();
  }, [
    beginRecording,
    isLockedShared,
    stopAndAppend,
    willCancelShared,
    willLockShared,
  ]);

  const handleLock = useCallback(() => {
    if (willLockRef.current || willCancelRef.current || isLockedShared.value) {
      return;
    }
    willLockRef.current = true;
    willLockShared.value = true;
    isLockedShared.value = true;
    haptic.light();
  }, [isLockedShared, willLockShared]);

  const handleCancel = useCallback(() => {
    if (willCancelRef.current) return;
    willCancelRef.current = true;
    willCancelShared.value = true;
    if (isLockedShared.value) {
      isLockedShared.value = false;
    }
    haptic.light();
  }, [isLockedShared, willCancelShared]);

  const handleEnd = useCallback(async () => {
    if (willCancelRef.current) {
      await discardCurrent();
      return;
    }
    if (willLockRef.current) {
      // Lock confirmed: keep recording, hide lock-hint bounce and reset position.
      willLockRef.current = false;
      willLockShared.value = false;
      translateX.value = withSpring(0);
      translateY.value = withSpring(0);
      return;
    }
    if (isLockedShared.value) {
      // Already locked; keep recording until the user taps the close icon.
      return;
    }
    if (recordingRef.current) {
      await stopAndAppend();
      return;
    }
    // Nothing started (permission denied or very quick tap); just reset.
    reset();
  }, [
    discardCurrent,
    isLockedShared,
    reset,
    stopAndAppend,
    translateX,
    translateY,
    willLockShared,
  ]);

  const panGesture = Gesture.Pan()
    .onBegin(() => {
      runOnJS(handleBegin)();
    })
    .onUpdate((e) => {
      translateX.value = e.translationX;
      translateY.value = e.translationY;
      if (
        e.translationY < -LOCK_THRESHOLD &&
        !isLockedShared.value &&
        e.translationX > -CANCEL_THRESHOLD &&
        !willCancelShared.value &&
        !willLockShared.value
      ) {
        runOnJS(handleLock)();
      } else if (
        e.translationX < -CANCEL_THRESHOLD &&
        !willCancelShared.value
      ) {
        runOnJS(handleCancel)();
      }
    })
    .onEnd(() => {
      runOnJS(handleEnd)();
    })
    .onFinalize((_e, success) => {
      if (!success) {
        runOnJS(handleEnd)();
      }
    });

  const animatedStyle = useAnimatedStyle(
    () => ({
      transform: [
        { translateX: translateX.value },
        { translateY: translateY.value },
      ],
      backgroundColor: isRecording ? '#B33A3A' : 'transparent',
      borderColor: willCancelShared.value
        ? '#B33A3A'
        : isRecording
          ? '#B33A3A'
          : BRAND_COLOR,
    }),
    [isRecording],
  );

  const lockHintStyle = useAnimatedStyle(() => ({
    opacity: willLockShared.value ? 1 : 0,
    transform: [{ translateY: willLockShared.value ? -8 : 0 }],
  }));

  const cancelHintStyle = useAnimatedStyle(() => ({
    opacity: willCancelShared.value ? 1 : 0,
    transform: [{ translateX: willCancelShared.value ? -8 : 0 }],
  }));

  return (
    <GestureDetector gesture={panGesture}>
      <Reanimated.View
        style={[styles.button, animatedStyle]}
        pointerEvents="auto"
      >
        <Ionicons
          name={isRecording ? 'close' : 'mic'}
          size={24}
          color={isRecording ? COLORS.white : BRAND_COLOR}
        />
        <Reanimated.View style={[styles.hint, styles.lockHint, lockHintStyle]}>
          <Ionicons name="lock-closed" size={16} color="#7c3aed" />
        </Reanimated.View>
        <Reanimated.View
          style={[styles.hint, styles.cancelHint, cancelHintStyle]}
        >
          <Ionicons name="trash" size={16} color="#B33A3A" />
        </Reanimated.View>
      </Reanimated.View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  button: {
    width: 44,
    height: 44,
    borderRadius: 22,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: 'transparent',
  },
  hint: {
    position: 'absolute',
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: 'rgba(255,255,255,0.95)',
    alignItems: 'center',
    justifyContent: 'center',
    opacity: 0,
  },
  lockHint: {
    top: -44,
    borderColor: '#7c3aed',
    borderWidth: 1,
  },
  cancelHint: {
    left: -44,
    borderColor: '#B33A3A',
    borderWidth: 1,
  },
});
