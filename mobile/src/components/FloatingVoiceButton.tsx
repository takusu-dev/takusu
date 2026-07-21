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
import { BRAND_COLOR, COLORS } from '@/src/theme';
import { haptic } from '@/src/components/haptics';

const TASKADD_SLIDE_THRESHOLD = 60;

type ButtonState = 'idle' | 'pending' | 'gesture';

export function FloatingVoiceButton() {
  const pathname = usePathname();
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const { workersToken } = useServer();
  const { setPendingSessionId } = useVoice();

  const agentClient = useMemo(
    () => new AgentClient(`http://127.0.0.1:${DEFAULT_PORT}`, workersToken),
    [workersToken],
  );

  const [state, setState] = useState<ButtonState>('idle');
  const stateRef = useRef<ButtonState>('idle');
  const isSlideRef = useRef(false);
  const transitionedRef = useRef(false);
  const slideResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const buttonY = useSharedValue(0);

  const isHome = pathname === '/' || pathname === '' || pathname === '/index';

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    return () => {
      if (slideResetTimerRef.current) clearTimeout(slideResetTimerRef.current);
    };
  }, []);

  // FloatingVoiceButton is mounted at the root (_layout.tsx), so it stays
  // alive while navigating to /agent or /task/add. Reset only the button UI
  // when leaving the home screen; pendingSessionId must stay intact so
  // AgentView can consume the queued session.
  useEffect(() => {
    if (isHome) return;
    setState('idle');
    stateRef.current = 'idle';
    isSlideRef.current = false;
    transitionedRef.current = false;
    buttonY.value = 0;
    if (slideResetTimerRef.current) {
      clearTimeout(slideResetTimerRef.current);
      slideResetTimerRef.current = null;
    }
  }, [isHome, buttonY]);

  const reset = useCallback(() => {
    if (slideResetTimerRef.current) {
      clearTimeout(slideResetTimerRef.current);
      slideResetTimerRef.current = null;
    }
    setState('idle');
    stateRef.current = 'idle';
    isSlideRef.current = false;
    transitionedRef.current = false;
    buttonY.value = withSpring(0);
    setPendingSessionId(null);
  }, [buttonY, setPendingSessionId]);

  const pushAgent = useCallback(async () => {
    if (transitionedRef.current || pathname === '/agent') return;

    // Mark the transition immediately so a second press/release while
    // createSession is still in flight cannot open another Agent and create
    // a duplicate empty session.
    transitionedRef.current = true;

    // When the user is not authenticated we have no session to create, but
    // we still open Agent so the setup flow is reachable.
    if (workersToken) {
      try {
        const sessionId = await agentClient.createSession();
        if (sessionId) {
          setPendingSessionId(sessionId);
        }
      } catch (e) {
        // If creating a session fails we still open Agent; the user can start
        // a new session from the composer.
        Sentry.captureException(e);
      }
    }

    router.push('/agent');
  }, [agentClient, pathname, router, setPendingSessionId, workersToken]);

  const pushTaskAdd = useCallback(() => {
    if (transitionedRef.current) return;
    transitionedRef.current = true;
    router.push('/task/add');
  }, [router]);

  const handleSlide = useCallback(() => {
    if (isSlideRef.current || transitionedRef.current) return;
    isSlideRef.current = true;
    stateRef.current = 'gesture';
    setState('gesture');
    pushTaskAdd();
    haptic.light();
    slideResetTimerRef.current = setTimeout(reset, 100);
  }, [pushTaskAdd, reset]);

  const handlePressIn = useCallback(() => {
    if (stateRef.current !== 'idle' || transitionedRef.current) return;

    setState('pending');
    stateRef.current = 'pending';
    isSlideRef.current = false;
    transitionedRef.current = false;
    buttonY.value = 0;
  }, [buttonY]);

  const handleRelease = useCallback(async () => {
    if (isSlideRef.current || transitionedRef.current) {
      reset();
      return;
    }
    if (stateRef.current === 'pending') {
      await pushAgent();
      return;
    }
    reset();
  }, [pushAgent, reset]);

  const isActive = state !== 'idle';

  const panGesture = Gesture.Pan()
    .activeOffsetY([-10, 10])
    .failOffsetX([-20, 20])
    .onBegin(() => {
      runOnJS(handlePressIn)();
    })
    .onUpdate((e) => {
      buttonY.value = Math.min(0, e.translationY);
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
      backgroundColor: isActive ? '#B33A3A' : BRAND_COLOR,
    }),
    [isActive],
  );

  const hintStyle = useAnimatedStyle(() => {
    const progress = Math.min(
      1,
      Math.max(0, -buttonY.value / (TASKADD_SLIDE_THRESHOLD * 0.7)),
    );
    return {
      opacity: isActive ? progress : 0,
      transform: [{ scale: 0.8 + progress * 0.2 }],
    };
  }, [isActive]);

  const iconName = isActive ? 'close' : 'add';

  if (!isHome) {
    return null;
  }

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
