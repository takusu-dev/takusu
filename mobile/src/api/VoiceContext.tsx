import {
  createContext,
  useContext,
  useState,
  useCallback,
  useMemo,
  type ReactNode,
} from 'react';

interface VoiceContextValue {
  /** Whether the floating voice button should be visible inside AgentView. */
  showInAgent: boolean;
  setShowInAgent: (value: boolean) => void;
}

const VoiceContext = createContext<VoiceContextValue>({
  showInAgent: false,
  setShowInAgent: () => {},
});

export function VoiceProvider({ children }: { children: ReactNode }) {
  const [showInAgent, setShowInAgent] = useState(false);

  const setShowInAgentStable = useCallback((value: boolean) => {
    setShowInAgent(value);
  }, []);

  const value = useMemo<VoiceContextValue>(
    () => ({ showInAgent, setShowInAgent: setShowInAgentStable }),
    [showInAgent, setShowInAgentStable],
  );

  return (
    <VoiceContext.Provider value={value}>{children}</VoiceContext.Provider>
  );
}

export function useVoice(): VoiceContextValue {
  return useContext(VoiceContext);
}
