import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useMemo,
  type ReactNode,
} from 'react';

interface VoiceContextValue {
  /** Whether any voice button is currently recording. */
  isRecording: boolean;
  setIsRecording: (value: boolean) => void;
  /** Session id queued by the floating voice button for AgentView to activate as a new session. */
  pendingSessionId: string | null;
  setPendingSessionId: (value: string | null) => void;
}

const VoiceContext = createContext<VoiceContextValue>({
  isRecording: false,
  setIsRecording: () => {},
  pendingSessionId: null,
  setPendingSessionId: () => {},
});

export function VoiceProvider({
  children,
  onRecordingChange,
}: {
  children: ReactNode;
  onRecordingChange?: (
    listener: (recording: boolean) => void,
  ) => (() => void) | void;
}) {
  const [isRecording, setIsRecording] = useState(false);
  const [pendingSessionId, setPendingSessionIdState] = useState<string | null>(
    null,
  );

  const setIsRecordingStable = useCallback((value: boolean) => {
    setIsRecording(value);
  }, []);

  useEffect(() => {
    if (!onRecordingChange) return;
    return onRecordingChange(setIsRecordingStable);
  }, [onRecordingChange, setIsRecordingStable]);

  const setPendingSessionId = useCallback((value: string | null) => {
    setPendingSessionIdState(value);
  }, []);

  const value = useMemo<VoiceContextValue>(
    () => ({
      isRecording,
      setIsRecording: setIsRecordingStable,
      pendingSessionId,
      setPendingSessionId,
    }),
    [isRecording, setIsRecordingStable, pendingSessionId, setPendingSessionId],
  );

  return (
    <VoiceContext.Provider value={value}>{children}</VoiceContext.Provider>
  );
}

export function useVoice(): VoiceContextValue {
  return useContext(VoiceContext);
}
