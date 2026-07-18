import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  AppState,
  FlatList,
  Keyboard,
  KeyboardAvoidingView,
  PermissionsAndroid,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  useWindowDimensions,
  View,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useRouter, useFocusEffect } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Reanimated, {
  runOnJS,
  useAnimatedStyle,
  useSharedValue,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import Markdown, {
  MarkdownIt,
  type ASTNode,
  type MarkdownStyles,
  type RenderRules,
} from 'react-native-markdown-renderer';
import type { NativeScrollEvent, NativeSyntheticEvent } from 'react-native';
import { DEFAULT_PORT, useServer } from '@/src/api/ServerProvider';
import { markdownToSpeech } from '@/src/utils/markdownToSpeech';
import TakusuAudioModule from '../../modules/takusu-server/src/TakusuAudioModule';
import {
  AGENT_SESSION_HISTORY_DEFAULT,
  loadAgentApiKey,
  loadSettings,
} from '@/src/api/settingsStore';
import { AgentClient, AgentApiError, AbortError } from '@/src/api/agentClient';
import type { ApprovalRequest, TurnEvent } from '@/src/api/agentTypes';
import {
  deleteSessionSnapshot,
  loadSessionHistory,
  loadSessionSnapshot,
  type AgentSessionSnapshot,
  type Message,
  type MessageSegment,
  type ToolCallItem,
  saveSessionHistory,
  saveSessionSnapshot,
} from '@/src/api/agentSessionStore';
import { ApprovalPanel } from '@/src/components/ApprovalPanel';
import { BRAND_COLOR, COLORS, useColors, type ColorSet } from '@/src/theme';

function newId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function appendSegment(
  segments: MessageSegment[],
  segment: MessageSegment,
): MessageSegment[] {
  const last = segments[segments.length - 1];
  if (last && last.type === 'text' && segment.type === 'text') {
    return [
      ...segments.slice(0, -1),
      { ...last, text: last.text + segment.text },
    ];
  }
  if (last && last.type === 'thinking' && segment.type === 'thinking') {
    return [
      ...segments.slice(0, -1),
      { ...last, text: last.text + segment.text },
    ];
  }
  return [...segments, segment];
}

function appendTextSegment(
  segments: MessageSegment[],
  text: string,
): MessageSegment[] {
  return appendSegment(segments, { type: 'text', text });
}

function finalizeTextSegment(
  segments: MessageSegment[],
  text: string,
): MessageSegment[] {
  const last = segments[segments.length - 1];
  if (last && last.type === 'text') {
    return [...segments.slice(0, -1), { ...last, text }];
  }
  return [...segments, { type: 'text', text }];
}

function buildFallbackSegments(message: Message): MessageSegment[] {
  const segments: MessageSegment[] = [];
  if (message.thinking && message.thinking.length > 0) {
    segments.push({ type: 'thinking', text: message.thinking });
  }
  message.toolCalls?.forEach((_, index) => {
    segments.push({ type: 'toolCall', callIndex: index });
  });
  if (message.text.length > 0) {
    segments.push({ type: 'text', text: message.text });
  }
  return segments;
}

function getCollapsed(message: Message, groupIndex: number): boolean {
  if (message.collapsedGroups && message.collapsedGroups.length > groupIndex) {
    return message.collapsedGroups[groupIndex] ?? true;
  }
  return message.collapsed ?? true;
}

function updateCollapsedGroup(message: Message, groupIndex: number): boolean[] {
  const next = [...(message.collapsedGroups ?? [])];
  while (next.length <= groupIndex) {
    next.push(true);
  }
  next[groupIndex] = !getCollapsed(message, groupIndex);
  return next;
}

interface TextItem {
  type: 'text';
  text: string;
}

interface ContextItem {
  type: 'context';
  thinking?: string;
  callIndices: number[];
  groupIndex: number;
}

type AssistantItem = TextItem | ContextItem;

function buildAssistantItems(message: Message): AssistantItem[] {
  const segments = message.segments ?? buildFallbackSegments(message);
  const items: AssistantItem[] = [];
  let context: ContextItem | null = null;
  let text: TextItem | null = null;
  let groupCount = 0;
  for (const segment of segments) {
    if (segment.type === 'text') {
      if (context) {
        items.push(context);
        context = null;
      }
      if (text) {
        text.text += segment.text;
      } else {
        text = { type: 'text', text: segment.text };
      }
      continue;
    }
    if (text) {
      items.push(text);
      text = null;
    }
    if (!context) {
      context = {
        type: 'context',
        callIndices: [],
        groupIndex: groupCount++,
      };
    }
    if (segment.type === 'thinking') {
      context.thinking = (context.thinking ?? '') + segment.text;
    } else {
      context.callIndices.push(segment.callIndex);
    }
  }
  if (text) items.push(text);
  if (context) items.push(context);
  return items;
}

interface ToolNameChipProps {
  call: ToolCallItem;
  colors: ColorSet;
}

function ToolNameChip({ call, colors }: ToolNameChipProps) {
  return (
    <View
      style={[
        styles.toolChip,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
        },
      ]}
    >
      <View
        style={[
          styles.toolChipDot,
          {
            backgroundColor: call.isError ? colors.red : colors.green,
          },
        ]}
      />
      <Text style={[styles.toolChipText, { color: colors.black }]}>
        {call.name}
      </Text>
    </View>
  );
}

interface ToolCallCardProps {
  call: ToolCallItem;
  colors: ColorSet;
}

function ToolCallCard({ call, colors }: ToolCallCardProps) {
  return (
    <View
      style={[
        styles.toolCall,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
        },
      ]}
    >
      <View style={styles.toolCallHeader}>
        <View
          style={[
            styles.toolStatus,
            {
              backgroundColor: call.isError ? colors.red : colors.green,
            },
          ]}
        />
        <Text style={{ color: colors.black, fontWeight: '700' }}>
          {call.name}
        </Text>
      </View>
      {call.arguments !== undefined && (
        <Text style={[styles.toolArgs, { color: colors.gray }]}>
          {JSON.stringify(call.arguments, null, 2)}
        </Text>
      )}
      {call.result !== undefined && (
        <Text
          style={{
            color: call.isError ? colors.red : colors.green,
          }}
        >
          {call.isError ? `エラー: ${call.result}` : call.result}
        </Text>
      )}
    </View>
  );
}

interface AssistantMessageProps {
  item: Message;
  colors: ColorSet;
  markdownStyles: Partial<MarkdownStyles>;
  markdownIt: MarkdownIt;
  markdownRules: RenderRules;
  onToggleGroupCollapsed: (messageId: string, groupIndex: number) => void;
}

function AssistantMessage({
  item,
  colors,
  markdownStyles,
  markdownIt,
  markdownRules,
  onToggleGroupCollapsed,
}: AssistantMessageProps) {
  const items = useMemo(() => buildAssistantItems(item), [item]);
  return (
    <View
      style={[
        styles.bubble,
        styles.assistantBubble,
        { backgroundColor: colors.separator, gap: 8 },
      ]}
    >
      {items.map((it, idx) => {
        if (it.type === 'text') {
          return (
            <View key={`t-${idx}`}>
              {item.state === 'done' || item.state === undefined ? (
                <Markdown
                  style={markdownStyles}
                  markdownit={markdownIt}
                  rules={markdownRules}
                >
                  {it.text}
                </Markdown>
              ) : (
                <Text style={{ color: colors.black }}>{it.text}</Text>
              )}
            </View>
          );
        }
        const collapsed = getCollapsed(item, it.groupIndex);
        const calls = it.callIndices
          .map((i) => item.toolCalls?.[i])
          .filter((c): c is ToolCallItem => c !== undefined);
        const hasThinking = it.thinking && it.thinking.length > 0;
        return (
          <View key={`c-${idx}`} style={{ gap: 6 }}>
            <Pressable
              onPress={() => onToggleGroupCollapsed(item.id, it.groupIndex)}
              style={[
                styles.contextHeader,
                {
                  backgroundColor: colors.surfaceTint,
                  borderColor: colors.separator,
                },
              ]}
            >
              <View style={styles.contextHeaderInner}>
                <Ionicons
                  name={collapsed ? 'chevron-forward' : 'chevron-down'}
                  size={14}
                  color={colors.gray}
                />
                <View style={styles.toolChips}>
                  {hasThinking && (
                    <View
                      style={[
                        styles.toolChip,
                        {
                          backgroundColor: colors.surface,
                          borderColor: colors.separator,
                        },
                      ]}
                    >
                      <Text
                        style={[styles.toolChipText, { color: colors.black }]}
                      >
                        考え中
                      </Text>
                    </View>
                  )}
                  {calls.slice(0, 3).map((call, i) => (
                    <ToolNameChip key={i} call={call} colors={colors} />
                  ))}
                  {calls.length > 3 && (
                    <Text style={[styles.toolChipText, { color: colors.gray }]}>
                      +{calls.length - 3}
                    </Text>
                  )}
                </View>
              </View>
            </Pressable>
            {!collapsed && (
              <View style={styles.contextBody}>
                {it.thinking && (
                  <Text style={[styles.thinkingText, { color: colors.gray }]}>
                    {it.thinking}
                  </Text>
                )}
                {calls.map((call, i) => (
                  <ToolCallCard key={i} call={call} colors={colors} />
                ))}
              </View>
            )}
          </View>
        );
      })}
      {item.state !== 'done' && item.text.length === 0 && (
        <ActivityIndicator color={BRAND_COLOR} />
      )}
    </View>
  );
}

const SWIPE_THRESHOLD = 40;
const SCROLL_THRESHOLD = 40;

export function AgentView() {
  const router = useRouter();
  const colors = useColors();
  const markdownStyles = useMemo<Partial<MarkdownStyles>>(
    () => ({
      text: { color: colors.black },
      link: { color: BRAND_COLOR },
      codeBlock: {
        backgroundColor: colors.separator,
        padding: 8,
        borderRadius: 8,
      },
      codeInline: {
        backgroundColor: colors.separator,
        paddingHorizontal: 4,
        paddingVertical: 2,
        borderRadius: 4,
      },
    }),
    [colors],
  );
  const markdownIt = useMemo(() => new MarkdownIt({ typographer: false }), []);
  const markdownRules = useMemo<RenderRules>(
    () => ({
      image: (node: ASTNode) => (
        <Text key={node.key} style={{ color: colors.gray }}>
          {node.content}
        </Text>
      ),
    }),
    [colors.gray],
  );
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const { workersToken, ready } = useServer();
  const client = useMemo(
    () => new AgentClient(`http://127.0.0.1:${DEFAULT_PORT}`, workersToken),
    [workersToken],
  );

  const [messages, setMessages] = useState<Message[]>([]);
  const [text, setText] = useState('');
  const [sessionIds, setSessionIds] = useState<string[]>([]);
  const [activeIndex, setActiveIndex] = useState(0);
  const [sessionHistoryCount, setSessionHistoryCount] = useState(
    AGENT_SESSION_HISTORY_DEFAULT,
  );
  const [approval, setApproval] = useState<ApprovalRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);
  const [audioReady, setAudioReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [keyboardVisible, setKeyboardVisible] = useState(Keyboard.isVisible());
  const [inputHeight, setInputHeight] = useState(44);
  const [historyReady, setHistoryReady] = useState(false);
  const [isSwitching, setIsSwitching] = useState(false);

  const sessionIdsRef = useRef<string[]>([]);
  const activeIndexRef = useRef(0);
  const sessionIdRef = useRef<string | null>(null);
  const sessionHistoryCountRef = useRef(AGENT_SESSION_HISTORY_DEFAULT);
  const isSwitchingRef = useRef(false);
  const audioReadyRef = useRef(false);
  const lastTtsRef = useRef<{
    id: string;
    voiceId: string;
    language: string;
    sampleRate: number;
  } | null>(null);
  const streamAbortRef = useRef<AbortController | null>(null);
  const backgroundAbortedRef = useRef(false);
  const flatListRef = useRef<FlatList<Message>>(null);
  const autoScrollRef = useRef(true);
  const viewOffset = useSharedValue(0);
  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: viewOffset.value }],
  }));

  useEffect(() => {
    sessionIdsRef.current = sessionIds;
  }, [sessionIds]);
  useEffect(() => {
    activeIndexRef.current = activeIndex;
  }, [activeIndex]);
  useEffect(() => {
    sessionHistoryCountRef.current = sessionHistoryCount;
  }, [sessionHistoryCount]);
  useEffect(() => {
    isSwitchingRef.current = isSwitching;
  }, [isSwitching]);

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (next) => {
      if (next === 'background' && streamAbortRef.current) {
        backgroundAbortedRef.current = true;
        streamAbortRef.current.abort();
      }
    });
    return () => subscription.remove();
  }, []);

  useEffect(() => {
    const showEvent =
      Platform.OS === 'ios' ? 'keyboardWillShow' : 'keyboardDidShow';
    const hideEvent =
      Platform.OS === 'ios' ? 'keyboardWillHide' : 'keyboardDidHide';
    const showSub = Keyboard.addListener(showEvent, () =>
      setKeyboardVisible(true),
    );
    const hideSub = Keyboard.addListener(hideEvent, () =>
      setKeyboardVisible(false),
    );
    return () => {
      showSub.remove();
      hideSub.remove();
    };
  }, []);

  async function activateSessionId(id: string, isNew = false) {
    let ids = sessionIdsRef.current;
    const existingIndex = ids.indexOf(id);
    if (isNew && existingIndex !== -1) {
      ids = ids.filter((s) => s !== id);
    }
    if (existingIndex === -1 || isNew) {
      ids = [...ids, id];
      const max = sessionHistoryCountRef.current;
      if (ids.length > max) {
        const removed = ids.shift()!;
        await deleteSessionSnapshot(removed);
      }
    }
    const index = ids.indexOf(id);
    const snapshot = await loadSessionSnapshot(id);
    autoScrollRef.current = true;
    setMessages(snapshot?.messages ?? []);
    setApproval(snapshot?.approval ?? null);
    sessionIdRef.current = id;
    setText('');
    setSessionIds(ids);
    sessionIdsRef.current = ids;
    setActiveIndex(index);
    activeIndexRef.current = index;
    await saveSessionHistory({ ids, activeIndex: index });
  }

  function trimSessionIds(
    ids: string[],
    max: number,
    activeId: string | null,
  ): { ids: string[]; index: number; removed: string[] } {
    if (ids.length <= max) {
      return {
        ids,
        index: activeId ? ids.indexOf(activeId) : ids.length - 1,
        removed: [],
      };
    }
    if (!activeId || ids.indexOf(activeId) === -1) {
      const trimmed = ids.slice(-max);
      return {
        ids: trimmed,
        index: trimmed.length - 1,
        removed: ids.slice(0, -max),
      };
    }
    const keep = new Set<string>();
    keep.add(activeId);
    let i = ids.length - 1;
    while (keep.size < max && i >= 0) {
      keep.add(ids[i]);
      i--;
    }
    const trimmed = ids.filter((id) => keep.has(id));
    return {
      ids: trimmed,
      index: trimmed.indexOf(activeId),
      removed: ids.filter((id) => !keep.has(id)),
    };
  }

  async function ensureSession(): Promise<string> {
    if (sessionIdRef.current) return sessionIdRef.current;
    const created = await client.createSession();
    await activateSessionId(created, true);
    return created;
  }

  useEffect(() => {
    let cancelled = false;
    async function init() {
      try {
        const [history, settings] = await Promise.all([
          loadSessionHistory(),
          loadSettings(),
        ]);
        if (cancelled) return;

        const count = settings.agentSessionHistoryCount;
        setSessionHistoryCount(count);
        sessionHistoryCountRef.current = count;

        let ids = history.ids;
        let index = history.activeIndex;
        if (!Array.isArray(ids)) ids = [];
        if (index < 0 || index >= ids.length) {
          index = Math.max(0, ids.length - 1);
        }

        const activeId = ids[index] ?? null;
        const {
          ids: trimmed,
          index: trimmedIndex,
          removed,
        } = trimSessionIds(ids, count, activeId);
        if (removed.length > 0) {
          await Promise.all(
            removed.map((id) => deleteSessionSnapshot(id)),
          ).catch(() => {});
        }

        sessionIdsRef.current = trimmed;
        setSessionIds(trimmed);
        setActiveIndex(trimmedIndex);
        activeIndexRef.current = trimmedIndex;

        if (trimmed.length === 0) {
          const created = await client.createSession();
          await activateSessionId(created, true);
        } else {
          const newActiveId = trimmed[trimmedIndex];
          await activateSessionId(newActiveId, false);
          try {
            const pending = await client.getApproval(newActiveId);
            setApproval(pending);
          } catch (e) {
            if (e instanceof AgentApiError && e.status === 404) {
              const created = await client.createSession();
              const newIds = [...trimmed];
              newIds[trimmedIndex] = created;
              sessionIdsRef.current = newIds;
              setSessionIds(newIds);
              await deleteSessionSnapshot(newActiveId);
              await activateSessionId(created, false);
            } else {
              throw e;
            }
          }
        }

        if (!cancelled) setHistoryReady(true);
      } catch (e: unknown) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    }

    if (ready && workersToken) init();

    return () => {
      cancelled = true;
    };
  }, [client, ready, workersToken]);

  useEffect(() => {
    if (!historyReady) return;
    const max = sessionHistoryCountRef.current;
    const activeId = sessionIdRef.current;
    const currentIds = sessionIdsRef.current;
    const {
      ids: trimmed,
      index,
      removed,
    } = trimSessionIds(currentIds, max, activeId);
    if (removed.length === 0) return;
    Promise.all(removed.map((id) => deleteSessionSnapshot(id)))
      .catch(() => {})
      .finally(async () => {
        sessionIdsRef.current = trimmed;
        setSessionIds(trimmed);
        if (index !== activeIndexRef.current) {
          setActiveIndex(index);
          activeIndexRef.current = index;
        }
        await saveSessionHistory({
          ids: trimmed,
          activeIndex: activeIndexRef.current,
        }).catch(() => {});
      });
  }, [sessionHistoryCount, historyReady]);

  useFocusEffect(
    useCallback(() => {
      loadSettings()
        .then((settings) => {
          const count = settings.agentSessionHistoryCount;
          setSessionHistoryCount(count);
          sessionHistoryCountRef.current = count;
        })
        .catch(() => {});
    }, []),
  );

  useFocusEffect(
    useCallback(() => {
      if (!ready || !workersToken) return;
      let cancelled = false;
      async function configureAudio() {
        try {
          const settings = await loadSettings();
          const provider = settings.ttsProviders.find(
            (item) => item.id === settings.activeTtsProvider,
          );
          if (!provider) return;
          const last = lastTtsRef.current;
          if (
            audioReadyRef.current &&
            last &&
            last.id === provider.id &&
            last.voiceId === provider.voiceId &&
            last.language === provider.language &&
            last.sampleRate === provider.sampleRate
          ) {
            return;
          }
          const apiKey = await loadAgentApiKey('tts', provider.id);
          await TakusuAudioModule.configure({
            modelDir: '',
            apiKey,
            voiceId: provider.voiceId,
            language: provider.language,
            sampleRate: provider.sampleRate,
          });
          if (cancelled) return;
          audioReadyRef.current = true;
          setAudioReady(true);
          setError(null);
          lastTtsRef.current = {
            id: provider.id,
            voiceId: provider.voiceId,
            language: provider.language,
            sampleRate: provider.sampleRate,
          };
        } catch (e: unknown) {
          if (cancelled) return;
          audioReadyRef.current = false;
          setAudioReady(false);
          setError(
            `音声モデルを準備してください: ${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
      configureAudio();
      return () => {
        cancelled = true;
      };
    }, [ready, workersToken]),
  );

  useEffect(() => {
    if (!sessionIdRef.current || busy) return;
    saveSessionSnapshot(sessionIdRef.current, { messages, approval }).catch(
      () => {},
    );
  }, [messages, approval, busy]);

  async function sendText(value: string) {
    if (!value.trim() || busy || isSwitchingRef.current || !historyReady)
      return;
    setError(null);
    backgroundAbortedRef.current = false;
    autoScrollRef.current = true;
    setMessages((current) => [
      ...current,
      { id: newId('user'), role: 'user', text: value.trim() },
    ]);
    setBusy(true);
    const abortController = new AbortController();
    streamAbortRef.current = abortController;
    let assistantId: string | null = null;
    try {
      const session = await ensureSession();
      const id = newId('assistant');
      assistantId = id;
      setMessages((current) => [
        ...current,
        {
          id,
          role: 'assistant',
          text: '',
          thinking: '',
          toolCalls: [],
          state: 'thinking',
          segments: [],
          collapsedGroups: [],
        },
      ]);
      const result = await client.runTurnStream(
        session,
        value.trim(),
        newId('turn'),
        (event: TurnEvent) => {
          setMessages((current) => {
            if (!assistantId) return current;
            const index = current.findIndex((m) => m.id === assistantId);
            if (index === -1) return current;
            const msg = current[index];
            const next = { ...msg };
            switch (event.type) {
              case 'Thinking':
                next.thinking = (next.thinking ?? '') + event.data;
                next.segments = appendSegment(next.segments ?? [], {
                  type: 'thinking',
                  text: event.data,
                });
                next.state = 'thinking';
                break;
              case 'ToolCall': {
                const callIndex = (next.toolCalls ?? []).length;
                next.toolCalls = [
                  ...(next.toolCalls ?? []),
                  {
                    name: event.data.name,
                    arguments: event.data.arguments,
                  },
                ];
                next.segments = appendSegment(next.segments ?? [], {
                  type: 'toolCall',
                  callIndex,
                });
                next.state = 'tool_call';
                break;
              }
              case 'ToolResult': {
                const calls = [...(next.toolCalls ?? [])];
                const last = calls[calls.length - 1];
                if (last) {
                  last.result = event.data.content;
                  last.isError = event.data.is_error;
                }
                next.toolCalls = calls;
                break;
              }
              case 'Text':
                next.text = (next.text ?? '') + event.data;
                next.segments = appendTextSegment(
                  next.segments ?? [],
                  event.data,
                );
                next.state = 'answering';
                break;
              case 'Error':
                next.text = event.data;
                next.segments = finalizeTextSegment(
                  next.segments ?? [],
                  event.data,
                );
                next.state = 'done';
                break;
              case 'Done':
                next.text = event.data.text;
                next.segments = finalizeTextSegment(
                  next.segments ?? [],
                  event.data.text,
                );
                next.state = 'done';
                break;
            }
            return [
              ...current.slice(0, index),
              next,
              ...current.slice(index + 1),
            ];
          });
        },
        abortController.signal,
      );
      setApproval(result.approval_request);
      const ttsText = markdownToSpeech(result.text);
      if (audioReady && ttsText.trim()) {
        try {
          await TakusuAudioModule.synthesizeAndPlay(ttsText);
        } catch (ttsError: unknown) {
          const ttsMessage =
            ttsError instanceof Error ? ttsError.message : String(ttsError);
          console.error('TTS failed:', ttsMessage);
          setError(`音声読み上げに失敗しました: ${ttsMessage}`);
        }
      }
    } catch (e: unknown) {
      const isAbort = backgroundAbortedRef.current || e instanceof AbortError;
      if (isAbort) {
        backgroundAbortedRef.current = false;
        if (assistantId) {
          setMessages((current) =>
            current.map((m) =>
              m.id === assistantId
                ? {
                    ...m,
                    text: m.text || '応答が中断されました',
                    state: 'done',
                    toolCalls: [],
                    segments: undefined,
                  }
                : m,
            ),
          );
        }
      } else {
        const message = e instanceof Error ? e.message : String(e);
        if (assistantId) {
          setMessages((current) =>
            current.map((m) =>
              m.id === assistantId
                ? { ...m, text: message, state: 'done', toolCalls: [] }
                : m,
            ),
          );
        }
        if (e instanceof AgentApiError && e.status === 404) {
          const lostId = sessionIdRef.current;
          if (lostId) {
            await deleteSessionSnapshot(lostId);
          }
          const remaining = sessionIdsRef.current.filter((s) => s !== lostId);
          sessionIdsRef.current = remaining;
          setSessionIds(remaining);
          try {
            const created = await client.createSession();
            await activateSessionId(created, true);
          } catch {
            sessionIdRef.current = null;
          }
          setError('Agentセッションが終了しました。もう一度送信してください');
        } else {
          setError(message);
        }
      }
    } finally {
      streamAbortRef.current = null;
      setBusy(false);
    }
  }

  async function send() {
    const value = text.trim();
    if (!value || busy || isSwitchingRef.current || !historyReady) return;
    setText('');
    setInputHeight(44);
    await sendText(value);
  }

  const toggleGroupCollapsed = useCallback((id: string, groupIndex: number) => {
    setMessages((current) =>
      current.map((m) =>
        m.id === id
          ? { ...m, collapsedGroups: updateCollapsedGroup(m, groupIndex) }
          : m,
      ),
    );
  }, []);

  const handleScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      const { contentOffset, contentSize, layoutMeasurement } =
        event.nativeEvent;
      const distanceFromBottom =
        contentSize.height - contentOffset.y - layoutMeasurement.height;
      autoScrollRef.current = distanceFromBottom <= SCROLL_THRESHOLD;
    },
    [],
  );

  const handleMessagesContentSizeChange = useCallback(() => {
    if (autoScrollRef.current) {
      flatListRef.current?.scrollToEnd({ animated: false });
    }
  }, []);

  async function toggleRecording() {
    if (busy || isSwitchingRef.current || !historyReady) return;
    setError(null);
    try {
      if (!recording) {
        if (!audioReady) throw new Error('音声モデルが準備されていません');
        if (Platform.OS === 'android') {
          const permission = await PermissionsAndroid.request(
            PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
          );
          if (permission !== PermissionsAndroid.RESULTS.GRANTED) {
            throw new Error('マイク権限が許可されていません');
          }
        }
        TakusuAudioModule.startRecording();
        setRecording(true);
        return;
      }
      setRecording(false);
      const transcript = await TakusuAudioModule.stopAndTranscribe();
      if (transcript.trim()) await sendText(transcript);
    } catch (e: unknown) {
      setRecording(false);
      setBusy(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function resolve(approve: boolean) {
    if (!sessionIdRef.current || !approval || busy || isSwitchingRef.current)
      return;
    setBusy(true);
    setError(null);
    try {
      const result = await client.resolveApproval(
        sessionIdRef.current,
        approval.id,
        approve,
        newId('approval'),
      );
      setApproval(null);
      const resolveText = result.approved
        ? '変更を適用しました。'
        : '変更を取り消しました。';
      setMessages((current) => [
        ...current,
        {
          id: newId('assistant'),
          role: 'assistant',
          text: resolveText,
          segments: [{ type: 'text', text: resolveText }],
          collapsedGroups: [],
        },
      ]);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function resetSwitchState() {
    setIsSwitching(false);
    isSwitchingRef.current = false;
  }

  function finishSessionSwitch(
    nextId: string,
    nextIndex: number,
    direction: number,
    snapshot: AgentSessionSnapshot,
  ) {
    try {
      sessionIdRef.current = nextId;
      setActiveIndex(nextIndex);
      activeIndexRef.current = nextIndex;
      setText('');
      viewOffset.value = direction * width;
      autoScrollRef.current = true;
      setMessages(snapshot.messages);
      setApproval(snapshot.approval);
      saveSessionHistory({
        ids: sessionIdsRef.current,
        activeIndex: nextIndex,
      }).catch(() => {});
      viewOffset.value = withSpring(0, { damping: 20, stiffness: 200 });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      viewOffset.value = withSpring(0, { damping: 20, stiffness: 200 });
    } finally {
      resetSwitchState();
    }
  }

  async function switchSession(delta: number) {
    if (isSwitchingRef.current || busy || !historyReady) return;
    const nextIndex = activeIndexRef.current + delta;
    if (nextIndex < 0 || nextIndex >= sessionIdsRef.current.length) return;
    const nextId = sessionIdsRef.current[nextIndex];
    const currentId = sessionIdRef.current;
    try {
      if (currentId) {
        await saveSessionSnapshot(currentId, { messages, approval }).catch(
          () => {},
        );
      }
      const snapshot = (await loadSessionSnapshot(nextId)) ?? {
        messages: [],
        approval: null,
      };
      setActiveIndex(nextIndex);
      activeIndexRef.current = nextIndex;
      isSwitchingRef.current = true;
      setIsSwitching(true);
      const direction = delta;
      viewOffset.value = withTiming(
        -direction * width,
        { duration: 120 },
        (finished) => {
          'worklet';
          if (finished) {
            runOnJS(finishSessionSwitch)(
              nextId,
              nextIndex,
              direction,
              snapshot,
            );
          } else {
            runOnJS(resetSwitchState)();
          }
        },
      );
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      resetViewOffset();
      resetSwitchState();
    }
  }

  async function startNewSession() {
    if (isSwitchingRef.current || busy || !historyReady) return;
    setError(null);
    const currentId = sessionIdRef.current;
    try {
      const created = await client.createSession();
      if (currentId) {
        await saveSessionSnapshot(currentId, { messages, approval }).catch(
          () => {},
        );
      }
      let ids = [...sessionIdsRef.current, created];
      const max = sessionHistoryCountRef.current;
      const removed: string[] = [];
      while (ids.length > max) {
        removed.push(ids.shift()!);
      }
      const nextIndex = ids.length - 1;
      await Promise.all(removed.map((id) => deleteSessionSnapshot(id))).catch(
        () => {},
      );
      sessionIdsRef.current = ids;
      setSessionIds(ids);
      setActiveIndex(nextIndex);
      activeIndexRef.current = nextIndex;
      const snapshot = (await loadSessionSnapshot(created)) ?? {
        messages: [],
        approval: null,
      };
      isSwitchingRef.current = true;
      setIsSwitching(true);
      const direction = 1;
      viewOffset.value = withTiming(
        -direction * width,
        { duration: 120 },
        (finished) => {
          'worklet';
          if (finished) {
            runOnJS(finishSessionSwitch)(
              created,
              nextIndex,
              direction,
              snapshot,
            );
          } else {
            runOnJS(resetSwitchState)();
          }
        },
      );
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      resetViewOffset();
      resetSwitchState();
    }
  }

  function resetViewOffset() {
    viewOffset.value = withSpring(0, { damping: 20, stiffness: 200 });
  }

  const switcherGesture = Gesture.Pan()
    .minDistance(20)
    .onEnd((e) => {
      if (e.translationX > SWIPE_THRESHOLD) {
        runOnJS(switchSession)(-1);
      } else if (e.translationX < -SWIPE_THRESHOLD) {
        runOnJS(switchSession)(1);
      }
    });

  return (
    <KeyboardAvoidingView
      style={[styles.container, { backgroundColor: colors.white }]}
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
    >
      <Reanimated.View style={[styles.container, animatedStyle]}>
        <View
          style={[
            styles.header,
            {
              borderBottomColor: colors.separator,
              paddingTop: 8 + insets.top,
            },
          ]}
        >
          <Pressable onPress={() => router.back()} style={styles.back}>
            <Text style={[styles.backText, { color: BRAND_COLOR }]}>‹</Text>
          </Pressable>
          <Text style={[styles.title, { color: colors.black }]}>Agent</Text>
          <Pressable
            disabled={busy || isSwitching || !historyReady}
            onPress={startNewSession}
            style={styles.newSession}
          >
            <Ionicons
              name="add"
              size={28}
              color={
                busy || isSwitching || !historyReady ? colors.gray : BRAND_COLOR
              }
            />
          </Pressable>
        </View>
        <FlatList
          ref={flatListRef}
          style={styles.messages}
          contentContainerStyle={styles.messageContent}
          data={messages}
          keyExtractor={(item) => item.id}
          renderItem={({ item }) => {
            if (item.role === 'user') {
              return (
                <View
                  style={[
                    styles.bubble,
                    styles.userBubble,
                    { backgroundColor: BRAND_COLOR },
                  ]}
                >
                  <Text style={{ color: COLORS.white }}>{item.text}</Text>
                </View>
              );
            }
            return (
              <AssistantMessage
                item={item}
                colors={colors}
                markdownStyles={markdownStyles}
                markdownIt={markdownIt}
                markdownRules={markdownRules}
                onToggleGroupCollapsed={toggleGroupCollapsed}
              />
            );
          }}
          onScroll={handleScroll}
          scrollEventThrottle={16}
          onContentSizeChange={handleMessagesContentSizeChange}
          ListEmptyComponent={
            <Text style={[styles.empty, { color: colors.gray }]}>
              何を予定しますか？
            </Text>
          }
        />
        {approval && (
          <ApprovalPanel
            approval={approval}
            colors={colors}
            busy={busy || isSwitching}
            onApprove={() => resolve(true)}
            onDeny={() => resolve(false)}
          />
        )}
        {error && (
          <Pressable onPress={() => Alert.alert('Agentエラー', error)}>
            <Text style={styles.error}>{error}</Text>
          </Pressable>
        )}
        <GestureDetector gesture={switcherGesture}>
          <View
            style={[
              styles.switcher,
              {
                borderTopColor: colors.separator,
                paddingBottom: 8,
              },
            ]}
          >
            <Pressable
              disabled={activeIndex === 0 || isSwitching || busy}
              onPress={() => switchSession(-1)}
              style={styles.switcherButton}
            >
              <Ionicons
                name="chevron-back"
                size={20}
                color={
                  activeIndex > 0 && !isSwitching && !busy
                    ? colors.black
                    : colors.gray
                }
              />
            </Pressable>
            <View style={styles.dots}>
              {sessionIds.map((id, i) => (
                <View
                  key={id}
                  style={[
                    styles.dot,
                    {
                      backgroundColor:
                        i === activeIndex ? BRAND_COLOR : colors.grayLight,
                    },
                  ]}
                />
              ))}
            </View>
            <Pressable
              disabled={
                activeIndex >= sessionIds.length - 1 || isSwitching || busy
              }
              onPress={() => switchSession(1)}
              style={styles.switcherButton}
            >
              <Ionicons
                name="chevron-forward"
                size={20}
                color={
                  activeIndex < sessionIds.length - 1 && !isSwitching && !busy
                    ? colors.black
                    : colors.gray
                }
              />
            </Pressable>
          </View>
        </GestureDetector>
        <View
          style={[
            styles.composer,
            {
              borderTopColor: colors.separator,
              paddingBottom: 12 + (keyboardVisible ? 0 : insets.bottom),
            },
          ]}
        >
          <Pressable
            disabled={busy || isSwitching || !historyReady}
            onPress={toggleRecording}
            style={[styles.record, recording && styles.recording]}
          >
            <Text style={styles.recordText}>{recording ? '停止' : '録音'}</Text>
          </Pressable>
          <TextInput
            style={[
              styles.input,
              {
                color: colors.black,
                borderColor: colors.separator,
                height: inputHeight,
              },
            ]}
            value={text}
            onChangeText={setText}
            placeholder="メッセージ"
            placeholderTextColor={colors.gray}
            editable={!busy && !isSwitching && historyReady}
            multiline
            textAlignVertical="top"
            onContentSizeChange={(e) => {
              const h = e.nativeEvent.contentSize.height;
              setInputHeight(Math.max(44, Math.min(120, h)));
            }}
          />
          <Pressable
            disabled={busy || isSwitching || !historyReady || !text.trim()}
            onPress={send}
            style={styles.send}
          >
            {busy ? (
              <ActivityIndicator color={COLORS.white} />
            ) : (
              <Text style={styles.sendText}>送信</Text>
            )}
          </Pressable>
        </View>
      </Reanimated.View>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    borderBottomWidth: 1,
    paddingBottom: 10,
  },
  back: { width: 56, alignItems: 'center' },
  backText: { fontSize: 40, lineHeight: 40 },
  title: { flex: 1, fontSize: 20, fontWeight: '700' },
  newSession: { width: 56, alignItems: 'center', justifyContent: 'center' },
  messages: { flex: 1 },
  messageContent: {
    padding: 16,
    gap: 10,
    flexGrow: 1,
    justifyContent: 'flex-end',
  },
  empty: { textAlign: 'center', marginBottom: 20 },
  bubble: { maxWidth: '85%', padding: 12, borderRadius: 14 },
  userBubble: { alignSelf: 'flex-end' },
  assistantBubble: { alignSelf: 'flex-start' },
  contextHeader: {
    borderRadius: 10,
    borderWidth: 1,
    overflow: 'hidden',
  },
  contextHeaderInner: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    padding: 8,
  },
  toolChips: {
    flex: 1,
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: 6,
  },
  toolChip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 12,
    borderWidth: 1,
  },
  toolChipDot: { width: 6, height: 6, borderRadius: 3 },
  toolChipText: { fontSize: 11 },
  contextBody: { gap: 6 },
  thinkingText: { fontSize: 12, fontStyle: 'italic' },
  toolCall: {
    borderRadius: 10,
    padding: 10,
    borderWidth: 1,
    gap: 4,
  },
  toolCallHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginBottom: 4,
  },
  toolStatus: { width: 8, height: 8, borderRadius: 4 },
  toolArgs: { fontSize: 11, fontFamily: 'monospace' },
  error: { color: '#B33A3A', paddingHorizontal: 16, paddingBottom: 8 },
  switcher: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 12,
    paddingTop: 8,
    borderTopWidth: 1,
  },
  switcherButton: {
    width: 56,
    height: 32,
    alignItems: 'center',
    justifyContent: 'center',
  },
  dots: { flexDirection: 'row', gap: 8, alignItems: 'center' },
  dot: { width: 8, height: 8, borderRadius: 4 },
  composer: {
    flexDirection: 'row',
    gap: 8,
    paddingHorizontal: 12,
    paddingTop: 12,
    borderTopWidth: 1,
  },
  record: {
    minWidth: 52,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
  },
  recording: { backgroundColor: '#B33A3A', borderColor: '#B33A3A' },
  recordText: { color: BRAND_COLOR, fontWeight: '700' },
  input: {
    flex: 1,
    minHeight: 44,
    borderWidth: 1,
    borderRadius: 10,
    paddingHorizontal: 12,
  },
  send: {
    minWidth: 64,
    borderRadius: 10,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
  },
  sendText: { color: COLORS.white, fontWeight: '700' },
});

export default AgentView;
