import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { GestureDetector, Gesture } from 'react-native-gesture-handler';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  withSpring,
  runOnJS,
} from 'react-native-reanimated';
import { usePathname, useRouter } from 'expo-router';
import * as Sentry from '@sentry/react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useServer, DEFAULT_PORT } from '@/src/api/ServerProvider';
import { useVoice } from '@/src/api/VoiceContext';
import { AgentClient } from '@/src/api/agentClient';
import {
  startRecording,
  stopAndTranscribe,
  voiceBridge,
} from '@/src/utils/voice';
import { BRAND_COLOR, COLORS } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

const VOICE_DECISION_MS = 500;
const VOICE_DECISION_PX = 12;
const TASKADD_SLIDE_THRESHOLD = 60;

type ButtonState = 'idle' | 'pending' | 'voice' | 'toggle' | 'gesture';

export function FloatingVoiceButton() {
  const pathname = usePathname();
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const { ready, workersToken } = useServer();
  const { isRecording: isRecordingContext, setPendingSessionId } = useVoice();

  const agentClient = useMemo(
    () => new AgentClient(`http://127.0.0.1:${DEFAULT_PORT}`, workersToken),
    [workersToken],
  );

  const [state, setState] = useState<ButtonState>('idle');
  const stateRef = useRef<ButtonState>('idle');
  const isSlideRef = useRef(false);
  // Local mirror of voice.ts isRecording. We set it just before startRecording()
  // and just before stopAndTranscribe() so that gesture callbacks see a stable
  // synchronous value even while those async calls are in flight.
  const isRecordingActiveRef = useRef(false);
  const decisionTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const slideResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const transitionedRef = useRef(false);
  const buttonY = useSharedValue(0);
  const maxAbsY = useSharedValue(0);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    isRecordingActiveRef.current = isRecordingContext;
  }, [isRecordingContext]);

  useEffect(() => {
    return () => {
      if (decisionTimerRef.current) clearTimeout(decisionTimerRef.current);
      if (slideResetTimerRef.current) clearTimeout(slideResetTimerRef.current);
    };
  }, []);

  const cancelDecisionTimer = useCallback(() => {
    if (decisionTimerRef.current) {
      clearTimeout(decisionTimerRef.current);
      decisionTimerRef.current = null;
    }
  }, []);

  const reset = useCallback(() => {
    cancelDecisionTimer();
    if (slideResetTimerRef.current) {
      clearTimeout(slideResetTimerRef.current);
      slideResetTimerRef.current = null;
    }
    setState('idle');
    stateRef.current = 'idle';
    isSlideRef.current = false;
    isRecordingActiveRef.current = false;
    transitionedRef.current = false;
    buttonY.value = withSpring(0);
    maxAbsY.value = 0;
    setPendingSessionId(null);
  }, [cancelDecisionTimer, maxAbsY, buttonY, setPendingSessionId]);

  const pushAgent = useCallback(async () => {
    if (transitionedRef.current || pathname === '/agent') return;
    let sessionId: string | null = null;
    try {
      sessionId = await agentClient.createSession();
    } catch (e) {
      // If creating a session fails we still open Agent; the user can start a
      // new session from the composer.
      Sentry.captureException(e);
    }
    if (sessionId) {
      setPendingSessionId(sessionId);
    }
    transitionedRef.current = true;
    router.push('/agent');
  }, [agentClient, pathname, router, setPendingSessionId]);

  const pushTaskAdd = useCallback(() => {
    if (transitionedRef.current) return;
    transitionedRef.current = true;
    router.push('/task/add');
  }, [router]);

  const stopAndDiscard = useCallback(async () => {
    if (!isRecordingActiveRef.current) return;
    isRecordingActiveRef.current = false;
    try {
      await stopAndTranscribe();
    } catch {
      // We don't need the transcript; ignore errors from cancellation.
    } finally {
      reset();
    }
  }, [reset]);

  // If the user navigates back home while the button is in a non-idle
  // recording state, abort the recording and reset immediately so the
  // button becomes responsive again as a normal "+" button.
  useEffect(() => {
    if (pathname === '/' && stateRef.current !== 'idle') {
      stopAndDiscard();
    }
  }, [pathname, stopAndDiscard]);

  const stopAndAppend = useCallback(async () => {
    if (!isRecordingActiveRef.current) {
      reset();
      return;
    }
    isRecordingActiveRef.current = false;
    try {
      const transcript = await stopAndTranscribe();
      if (transcript) {
        voiceBridge.setResult({ transcript, sendNow: false });
      }
    } catch {
      // ignore
    } finally {
      reset();
    }
  }, [reset]);

  const stopAndSend = useCallback(async () => {
    if (!isRecordingActiveRef.current) {
      reset();
      return;
    }
    isRecordingActiveRef.current = false;
    try {
      const transcript = await stopAndTranscribe();
      if (transcript) {
        voiceBridge.setResult({ transcript, sendNow: true });
      }
    } catch {
      // ignore
    } finally {
      reset();
    }
  }, [reset]);

  const enterToggle = useCallback(async () => {
    cancelDecisionTimer();
    stateRef.current = 'toggle';
    setState('toggle');
    await pushAgent();
    isSlideRef.current = false;
    maxAbsY.value = 0;
    buttonY.value = withSpring(0);
  }, [buttonY, cancelDecisionTimer, maxAbsY, pushAgent]);

  const handleSlide = useCallback(async () => {
    if (isSlideRef.current || transitionedRef.current) return;
    isSlideRef.current = true;
    stateRef.current = 'gesture';
    setState('gesture');
    cancelDecisionTimer();
    await stopAndDiscard();
    pushTaskAdd();
    haptic.light();
    slideResetTimerRef.current = setTimeout(reset, 100);
  }, [cancelDecisionTimer, pushTaskAdd, reset, stopAndDiscard]);

  const commitVoice = useCallback(async () => {
    if (
      stateRef.current !== 'pending' ||
      isSlideRef.current ||
      maxAbsY.value > VOICE_DECISION_PX
    ) {
      return;
    }
    stateRef.current = 'voice';
    setState('voice');
    await pushAgent();
  }, [pushAgent, maxAbsY]);

  const handlePressIn = useCallback(async () => {
    if (!ready || !workersToken) return;

    if (stateRef.current === 'toggle') {
      await stopAndAppend();
      return;
    }
    if (stateRef.current !== 'idle') {
      // We are already recording in another state (e.g. user navigated back
      // home from /agent while voice was active). Abort the recording and
      // reset so the next tap starts fresh.
      await stopAndDiscard();
      return;
    }

    setState('pending');
    stateRef.current = 'pending';
    isSlideRef.current = false;
    buttonY.value = 0;
    maxAbsY.value = 0;

    isRecordingActiveRef.current = true;
    try {
      await startRecording();
    } catch {
      isRecordingActiveRef.current = false;
      if (stateRef.current === 'pending') reset();
      return;
    }

    decisionTimerRef.current = setTimeout(() => {
      commitVoice();
    }, VOICE_DECISION_MS);
  }, [
    ready,
    workersToken,
    stopAndAppend,
    stopAndDiscard,
    commitVoice,
    reset,
    buttonY,
    maxAbsY,
  ]);

  const handleRelease = useCallback(async () => {
    cancelDecisionTimer();
    const current = stateRef.current;
    if (current === 'voice') {
      await stopAndSend();
    } else if (current === 'pending') {
      await enterToggle();
    } else if (current === 'toggle') {
      await stopAndAppend();
    } else {
      reset();
    }
  }, [cancelDecisionTimer, enterToggle, reset, stopAndAppend, stopAndSend]);

  const isRecording =
    state === 'pending' || state === 'voice' || state === 'toggle';

  const panGesture = Gesture.Pan()
    .activeOffsetY([-10, 10])
    .failOffsetX([-20, 20])
    .onBegin(() => {
      runOnJS(handlePressIn)();
    })
    .onUpdate((e) => {
      buttonY.value = Math.min(0, e.translationY);
      maxAbsY.value = Math.max(maxAbsY.value, Math.abs(e.translationY));
      if (e.translationY < -TASKADD_SLIDE_THRESHOLD) {
        runOnJS(handleSlide)();
      }
    })
    .onEnd(() => {
      runOnJS(handleRelease)();
    })
    .onFinalize((_e, success) => {
      if (!success) {
        runOnJS(handleRelease)();
      }
    });

  const buttonStyle = useAnimatedStyle(
    () => ({
      transform: [{ translateY: buttonY.value }],
      backgroundColor: isRecording ? '#B33A3A' : BRAND_COLOR,
    }),
    [isRecording],
  );

  const hintStyle = useAnimatedStyle(() => {
    const progress = Math.min(
      1,
      Math.max(0, -buttonY.value / (TASKADD_SLIDE_THRESHOLD * 0.7)),
    );
    return {
      opacity: isRecording ? progress : 0,
      transform: [{ scale: 0.8 + progress * 0.2 }],
    };
  }, [isRecording]);

  const isHome = pathname === '/' || pathname === '' || pathname === '/index';
  const isAgent = pathname === '/agent';
  if (!isHome && !(isAgent && isRecordingContext)) {
    return null;
  }

  const iconName = state === 'idle' ? 'add' : 'close';

  return (
    <View
      style={[styles.container, { bottom: 16 + insets.bottom }]}
      pointerEvents="box-none"
    >
      <Reanimated.View style={[styles.hint, hintStyle]}>
        <Ionicons name="arrow-up" size={28} color={BRAND_COLOR} />
      </Reanimated.View>
      <GestureDetector gesture={panGesture}>
        <Reanimated.View style={[styles.button, buttonStyle]}>
          <Ionicons name={iconName} size={28} color={COLORS.white} />
        </Reanimated.View>
      </GestureDetector>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    position: 'absolute',
    left: '50%',
    width: 56,
    zIndex: 100,
    transform: [{ translateX: -28 }],
    alignItems: 'center',
    justifyContent: 'center',
  },
  button: {
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
  hint: {
    position: 'absolute',
    bottom: 76,
    alignItems: 'center',
    justifyContent: 'center',
    padding: 12,
    borderRadius: 32,
    backgroundColor: 'rgba(255,255,255,0.9)',
    opacity: 0,
  },
});
