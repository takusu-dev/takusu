import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
} from 'react';
import {
  ActivityIndicator,
  Alert,
  AppState,
  FlatList,
  Keyboard,
  KeyboardAvoidingView,
  Platform,
  type LayoutChangeEvent,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  useWindowDimensions,
  View,
} from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { Ionicons } from '@expo/vector-icons';
import { useRouter, useFocusEffect } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Reanimated, {
  cancelAnimation,
  Easing,
  runOnJS,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withTiming,
} from 'react-native-reanimated';
import Markdown, {
  MarkdownIt,
  renderRules,
  type ASTNode,
  type MarkdownStyles,
  type RenderRules,
} from 'react-native-markdown-renderer';
import type { NativeScrollEvent, NativeSyntheticEvent } from 'react-native';
import { DEFAULT_PORT, useServer } from '@/src/api/ServerProvider';
import { useVoice } from '@/src/api/VoiceContext';
import { markdownToSpeech } from '@/src/utils/markdownToSpeech';
import {
  ensureAudioConfigured,
  voiceBridge,
  type VoiceResult,
} from '@/src/utils/voice';
import TakusuAudioModule from '../../modules/takusu-server/src/TakusuAudioModule';
import {
  AGENT_SESSION_HISTORY_DEFAULT,
  loadSettings,
} from '@/src/api/settingsStore';
import {
  AgentClient,
  AgentApiError,
  AbortError,
  type AgentTurnResult,
} from '@/src/api/agentClient';
import type {
  ApprovalRequest,
  TurnEvent,
  UserInputQuestion,
  UserInputAnswer,
} from '@/src/api/agentTypes';
import {
  deleteSessionSnapshot,
  loadSessionHistory,
  loadSessionSnapshot,
  type Message,
  type MessageSegment,
  type ToolCallItem,
  saveSessionHistory,
  saveSessionSnapshot,
} from '@/src/api/agentSessionStore';
import { getTurnIndex } from '@/src/utils/getTurnIndex';
import { ApprovalPanel } from '@/src/components/ApprovalPanel';
import { ComposerRecordButton } from '@/src/components/ComposerRecordButton';
import { EditMessageModal } from '@/src/components/EditMessageModal';
import { MessageContextMenu } from '@/src/components/MessageContextMenu';
import { haptic } from '@/src/components/haptics';
import { BRAND_COLOR, COLORS, useColors, type ColorSet } from '@/src/theme';

function newId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function formatJson(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value === 'string') {
    try {
      return JSON.stringify(JSON.parse(value), null, 2);
    } catch {
      return value;
    }
  }
  return JSON.stringify(value, null, 2);
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

type ContextStage =
  | { type: 'thinking'; text: string; active: boolean }
  | { type: 'toolCall'; callIndex: number };

interface TextItem {
  type: 'text';
  text: string;
}

interface ContextItem {
  type: 'context';
  stages: ContextStage[];
  groupIndex: number;
}

type AssistantItem = TextItem | ContextItem;

function buildAssistantItems(
  message: Message,
  isLatest: boolean,
): AssistantItem[] {
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
      context = { type: 'context', stages: [], groupIndex: groupCount++ };
    }
    if (segment.type === 'thinking') {
      context.stages.push({
        type: 'thinking',
        text: segment.text,
        active: false,
      });
    } else {
      context.stages.push({ type: 'toolCall', callIndex: segment.callIndex });
    }
  }
  if (text) items.push(text);
  if (context) items.push(context);

  if (message.state === 'thinking' && isLatest) {
    let activated = false;
    for (let i = items.length - 1; i >= 0 && !activated; i--) {
      const it = items[i];
      if (it.type === 'context') {
        for (let j = it.stages.length - 1; j >= 0; j--) {
          const stage = it.stages[j];
          if (stage.type === 'thinking') {
            stage.active = true;
            activated = true;
            break;
          }
        }
      }
    }
  }

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
            backgroundColor:
              call.result === undefined
                ? colors.gray
                : call.isError
                  ? colors.red
                  : colors.green,
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
  const isAsr = call.name === 'correct_asr';
  const asrCount = isAsr
    ? ((call.arguments as { questions?: unknown[] } | undefined)?.questions
        ?.length ?? 0)
    : 0;
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
          {isAsr ? 'ASR訂正' : call.name}
        </Text>
      </View>
      {!isAsr && call.arguments !== undefined && (
        <Text style={[styles.toolArgs, { color: colors.gray }]}>
          {formatJson(call.arguments)}
        </Text>
      )}
      {isAsr && call.arguments !== undefined && (
        <Text style={[styles.toolArgs, { color: colors.gray }]}>
          {asrCount} 件の認識テキストを確認
        </Text>
      )}
      {call.result !== undefined && (
        <Text
          style={{
            color: call.isError ? colors.red : colors.green,
          }}
        >
          {isAsr
            ? call.result
            : call.isError
              ? `エラー: ${formatJson(call.result)}`
              : formatJson(call.result)}
        </Text>
      )}
    </View>
  );
}

function PulsingDot({ color, size = 6 }: { color: string; size?: number }) {
  const scale = useSharedValue(1);
  const opacity = useSharedValue(1);

  useEffect(() => {
    scale.value = withRepeat(
      withTiming(0.85, { duration: 700, easing: Easing.inOut(Easing.ease) }),
      -1,
      true,
    );
    opacity.value = withRepeat(
      withTiming(0.5, { duration: 700, easing: Easing.inOut(Easing.ease) }),
      -1,
      true,
    );
    return () => {
      cancelAnimation(scale);
      cancelAnimation(opacity);
    };
  }, [scale, opacity]);

  const animatedStyle = useAnimatedStyle(() => ({
    transform: [{ scale: scale.value }],
    opacity: opacity.value,
  }));

  return (
    <Reanimated.View
      style={[
        {
          width: size,
          height: size,
          borderRadius: size / 2,
          backgroundColor: color,
        },
        animatedStyle,
      ]}
    />
  );
}

function BouncingDots({ color, size = 4 }: { color: string; size?: number }) {
  const progress = useSharedValue(0);

  useEffect(() => {
    progress.value = withRepeat(
      withTiming(1, { duration: 600, easing: Easing.linear }),
      -1,
      false,
    );
    return () => {
      cancelAnimation(progress);
    };
  }, [progress]);

  const dot1Style = useAnimatedStyle(() => {
    const t = progress.value % 1;
    return {
      transform: [{ translateY: -Math.abs(Math.sin(t * Math.PI)) * 4 }],
    };
  });
  const dot2Style = useAnimatedStyle(() => {
    const t = (progress.value + 1 / 3) % 1;
    return {
      transform: [{ translateY: -Math.abs(Math.sin(t * Math.PI)) * 4 }],
    };
  });
  const dot3Style = useAnimatedStyle(() => {
    const t = (progress.value + 2 / 3) % 1;
    return {
      transform: [{ translateY: -Math.abs(Math.sin(t * Math.PI)) * 4 }],
    };
  });

  const dotStyle = {
    width: size,
    height: size,
    borderRadius: size / 2,
    backgroundColor: color,
  };

  return (
    <View style={{ flexDirection: 'row', gap: 3, alignItems: 'center' }}>
      <Reanimated.View style={[dotStyle, dot1Style]} />
      <Reanimated.View style={[dotStyle, dot2Style]} />
      <Reanimated.View style={[dotStyle, dot3Style]} />
    </View>
  );
}

interface ThinkingChipProps {
  active: boolean;
  colors: ColorSet;
}

function ThinkingChip({ active, colors }: ThinkingChipProps) {
  return (
    <View
      style={[
        styles.toolChip,
        {
          backgroundColor: colors.surface,
          borderColor: active ? colors.brand : colors.separator,
        },
      ]}
    >
      {active ? (
        <PulsingDot color={colors.brand} size={6} />
      ) : (
        <View style={[styles.toolChipDot, { backgroundColor: colors.gray }]} />
      )}
      <Text
        style={[
          styles.toolChipText,
          { color: active ? colors.brand : colors.black },
        ]}
      >
        考え中
      </Text>
    </View>
  );
}

interface ThinkingCardProps {
  text: string;
  active: boolean;
  colors: ColorSet;
}

function ThinkingCard({ text, active, colors }: ThinkingCardProps) {
  return (
    <View
      style={[
        styles.thinkingCard,
        {
          backgroundColor: colors.surface,
          borderColor: colors.separator,
        },
      ]}
    >
      <View style={styles.thinkingHeader}>
        <Text
          style={[
            styles.thinkingLabel,
            { color: active ? colors.brand : colors.gray },
          ]}
        >
          考え中
        </Text>
        {active && <BouncingDots color={colors.brand} size={4} />}
      </View>
      <Text style={[styles.thinkingText, { color: colors.gray }]}>{text}</Text>
    </View>
  );
}

interface ContextGroupProps {
  context: ContextItem;
  message: Message;
  colors: ColorSet;
  availableHeight: number;
  onToggle: () => void;
}

function ContextGroup({
  context,
  message,
  colors,
  availableHeight,
  onToggle,
}: ContextGroupProps) {
  const collapsed = getCollapsed(message, context.groupIndex);
  const [isTall, setIsTall] = useState(false);
  const bodyHeightRef = useRef(0);

  const hasThinking = context.stages.some((s) => s.type === 'thinking');
  const hasActiveThinking = context.stages.some(
    (s) => s.type === 'thinking' && s.active,
  );
  const toolStages = context.stages.filter(
    (s): s is { type: 'toolCall'; callIndex: number } => s.type === 'toolCall',
  );

  const chips: ReactElement[] = [];
  if (hasThinking) {
    chips.push(
      <ThinkingChip
        key="thinking"
        active={hasActiveThinking}
        colors={colors}
      />,
    );
  }
  for (let i = 0; i < toolStages.length && chips.length < 3; i++) {
    const call = message.toolCalls?.[toolStages[i].callIndex];
    if (call) {
      chips.push(
        <ToolNameChip key={`tool-${i}`} call={call} colors={colors} />,
      );
    }
  }
  const totalChips = (hasThinking ? 1 : 0) + toolStages.length;
  const more =
    totalChips > 3 ? (
      <Text style={[styles.toolChipText, { color: colors.gray }]}>
        +{totalChips - 3}
      </Text>
    ) : null;

  const updateTall = useCallback(() => {
    if (availableHeight <= 0) return;
    setIsTall(bodyHeightRef.current > availableHeight * 0.6);
  }, [availableHeight]);

  useEffect(() => {
    updateTall();
  }, [updateTall]);

  const handleBodyLayout = (event: LayoutChangeEvent) => {
    bodyHeightRef.current = event.nativeEvent.layout.height;
    updateTall();
  };

  return (
    <View
      style={[
        styles.contextHeader,
        {
          backgroundColor: colors.surfaceTint,
          borderColor: colors.separator,
        },
      ]}
    >
      <Pressable onPress={onToggle}>
        <View style={styles.contextHeaderInner}>
          <Ionicons
            name={collapsed ? 'chevron-forward' : 'chevron-down'}
            size={14}
            color={colors.gray}
          />
          <View style={styles.toolChips}>
            {chips}
            {more}
          </View>
        </View>
      </Pressable>
      {!collapsed && (
        <View style={styles.contextBody} onLayout={handleBodyLayout}>
          {context.stages.map((stage, idx) => {
            if (stage.type === 'thinking') {
              return (
                <ThinkingCard
                  key={`s-${idx}`}
                  text={stage.text}
                  active={stage.active}
                  colors={colors}
                />
              );
            }
            const call = message.toolCalls?.[stage.callIndex];
            return call ? (
              <ToolCallCard key={`s-${idx}`} call={call} colors={colors} />
            ) : null;
          })}
          {isTall && (
            <Pressable onPress={onToggle}>
              <View
                style={[
                  styles.contextFooter,
                  { borderTopColor: colors.separator },
                ]}
              >
                <Ionicons name="chevron-up" size={14} color={colors.gray} />
                <Text
                  style={[styles.contextFooterText, { color: colors.gray }]}
                >
                  畳む
                </Text>
              </View>
            </Pressable>
          )}
        </View>
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
  availableHeight: number;
  isLatest: boolean;
  onToggleGroupCollapsed: (messageId: string, groupIndex: number) => void;
  onLongPress: (message: Message, position: { x: number; y: number }) => void;
}

const AssistantMessage = memo(function AssistantMessageImpl({
  item,
  colors,
  markdownStyles,
  markdownIt,
  markdownRules,
  availableHeight,
  isLatest,
  onToggleGroupCollapsed,
  onLongPress,
}: AssistantMessageProps) {
  const items = useMemo(
    () => buildAssistantItems(item, isLatest),
    [item, isLatest],
  );
  const handleLongPress = (event: {
    nativeEvent: { pageX?: number; pageY?: number };
  }) => {
    onLongPress(item, {
      x: event.nativeEvent.pageX ?? 0,
      y: event.nativeEvent.pageY ?? 0,
    });
  };

  return (
    <Pressable
      style={[
        styles.bubble,
        styles.assistantBubble,
        { backgroundColor: colors.separator, gap: 8 },
      ]}
      onLongPress={handleLongPress}
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
        return (
          <ContextGroup
            key={`c-${idx}`}
            context={it}
            message={item}
            colors={colors}
            availableHeight={availableHeight}
            onToggle={() => onToggleGroupCollapsed(item.id, it.groupIndex)}
          />
        );
      })}
      {item.state !== 'done' &&
        item.text.length === 0 &&
        items.length === 0 && (
          <View style={styles.loadingIndicator}>
            <ActivityIndicator color={BRAND_COLOR} />
          </View>
        )}
    </Pressable>
  );
});

interface UserMessageProps {
  message: Message;
  onLongPress: (message: Message, position: { x: number; y: number }) => void;
}

const UserMessage = memo(function UserMessageImpl({
  message,
  onLongPress,
}: UserMessageProps) {
  const handleLongPress = (event: {
    nativeEvent: { pageX?: number; pageY?: number };
  }) => {
    onLongPress(message, {
      x: event.nativeEvent.pageX ?? 0,
      y: event.nativeEvent.pageY ?? 0,
    });
  };

  return (
    <Pressable
      style={[
        styles.bubble,
        styles.userBubble,
        { backgroundColor: BRAND_COLOR },
      ]}
      onLongPress={handleLongPress}
    >
      <Text style={{ color: COLORS.white }}>{message.text}</Text>
    </Pressable>
  );
});

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
      listUnorderedItemIcon: {
        color: colors.gray,
        fontSize: 14,
        lineHeight: 24,
      },
      listOrderedItemIcon: {
        color: colors.gray,
        fontSize: 14,
        lineHeight: 24,
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
      list_item: (node, children, parent, styles) => {
        if (parent.some((p) => p.type === 'bullet_list')) {
          return (
            <View key={node.key} style={styles.listUnorderedItem as any}>
              <Text style={styles.listUnorderedItemIcon as any}>
                {'\u2022'}
              </Text>
              <View style={styles.listItem as any}>{children}</View>
            </View>
          );
        }
        return renderRules.list_item(node, children, parent, styles);
      },
    }),
    [colors.gray],
  );
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const { workersToken, ready } = useServer();
  const { pendingSessionId, setPendingSessionId } = useVoice();
  const [pendingResult, setPendingResult] = useState<VoiceResult | null>(null);
  const sendTextRef = useRef(sendText);
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
  const [audioReady, setAudioReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [keyboardVisible, setKeyboardVisible] = useState(Keyboard.isVisible());
  const [inputHeight, setInputHeight] = useState(44);
  const [messagesHeight, setMessagesHeight] = useState(0);
  const [historyReady, setHistoryReady] = useState(false);
  const [isSwitching, setIsSwitching] = useState(false);
  const [userInput, setUserInput] = useState<{
    sessionId: string;
    callId: string;
    questions: UserInputQuestion[];
    values: string[];
  } | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    visible: boolean;
    messageId: string;
    position: { x: number; y: number };
  }>({ visible: false, messageId: '', position: { x: 0, y: 0 } });
  const [editModal, setEditModal] = useState<{
    visible: boolean;
    messageId: string;
    text: string;
  }>({ visible: false, messageId: '', text: '' });

  const sessionIdsRef = useRef<string[]>([]);
  const activeIndexRef = useRef(0);
  const sessionIdRef = useRef<string | null>(null);
  const sessionHistoryCountRef = useRef(AGENT_SESSION_HISTORY_DEFAULT);
  const isSwitchingRef = useRef(false);
  const streamAbortRef = useRef<AbortController | null>(null);
  const backgroundAbortedRef = useRef(false);
  const flatListRef = useRef<FlatList<Message>>(null);
  const autoScrollRef = useRef(true);
  const skipSnapshotSaveRef = useRef(false);
  const lastPendingSessionIdRef = useRef<string | null>(null);
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
      setError(null);
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
        if (!Array.isArray(ids)) ids = [];

        if (pendingSessionId) {
          if (lastPendingSessionIdRef.current !== pendingSessionId) {
            // FloatingVoiceButton queued a brand-new session.
            lastPendingSessionIdRef.current = pendingSessionId;
            await activateSessionId(pendingSessionId, true);
            setPendingSessionId(null);
          }
          if (!cancelled) setHistoryReady(true);
          return;
        }

        if (lastPendingSessionIdRef.current !== null) {
          // We consumed the queued new session in a previous effect run.
          // `pendingSessionId` is already null (FloatingVoiceButton cleared it),
          // so `pendingSessionId == null` here. Without this guard we would fall
          // through to the normal history-load path and overwrite the just-created
          // session with the previous active session. The ref is intentionally not
          // reset to null; it is a marker that a queued session was already handled.
          if (!cancelled) setHistoryReady(true);
          return;
        }

        let index = history.activeIndex;
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
              // The server session no longer exists, but keep the local snapshot
              // so the history remains visible and swipable. Clear any stale
              // pending approval because it can no longer be resolved.
              setApproval(null);
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
  }, [client, ready, workersToken, pendingSessionId, setPendingSessionId]);

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
      ensureAudioConfigured().then(
        () => {
          if (cancelled) return;
          setAudioReady(true);
          setError(null);
        },
        (e: unknown) => {
          if (cancelled) return;
          setAudioReady(false);
          setError(
            `音声モデルを準備してください: ${e instanceof Error ? e.message : String(e)}`,
          );
        },
      );
      return () => {
        cancelled = true;
      };
    }, [ready, workersToken]),
  );

  useEffect(() => {
    if (!sessionIdRef.current || busy || skipSnapshotSaveRef.current) return;
    saveSessionSnapshot(sessionIdRef.current, { messages, approval }).catch(
      () => {},
    );
  }, [messages, approval, busy]);

  useEffect(() => {
    return voiceBridge.subscribe((r) => {
      if (r) setPendingResult(r);
    });
  }, []);

  useEffect(() => {
    if (!pendingResult) return;
    const { transcript, sendNow } = pendingResult;
    if (sendNow) {
      sendTextRef.current(transcript);
    } else {
      setText((t) => {
        const prefix = t && !t.endsWith(' ') ? `${t} ` : t;
        return `${prefix}${transcript}`;
      });
    }
    voiceBridge.consume();
    setPendingResult(null);
  }, [pendingResult]);

  async function sendText(value: string) {
    if (!value.trim() || busy || isSwitchingRef.current || !historyReady)
      return;
    setError(null);
    backgroundAbortedRef.current = false;
    autoScrollRef.current = true;
    setApproval(null);
    setUserInput(null);
    const trimmed = value.trim();
    setMessages((current) => [
      ...current,
      { id: newId('user'), role: 'user', text: trimmed },
    ]);
    setBusy(true);
    try {
      const session = await ensureSession();
      const assistantId = newId('assistant');
      setMessages((current) => [
        ...current,
        {
          id: assistantId,
          role: 'assistant',
          text: '',
          thinking: '',
          toolCalls: [],
          state: 'thinking',
          segments: [],
          collapsedGroups: [],
        },
      ]);
      await runAssistantStream(assistantId, (onEvent, signal) =>
        client.runTurnStream(session, trimmed, newId('turn'), onEvent, signal),
      );
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      setBusy(false);
    }
  }

  async function editTurn(turnIndex: number, userId: string, newText: string) {
    if (!newText.trim() || busy || isSwitchingRef.current || !historyReady)
      return;
    setError(null);
    backgroundAbortedRef.current = false;
    autoScrollRef.current = true;
    setApproval(null);
    setUserInput(null);
    const trimmed = newText.trim();
    setBusy(true);
    try {
      const session = await ensureSession();
      const assistantId = newId('assistant');
      let userIndex = -1;
      setMessages((current) => {
        const index = current.findIndex((m) => m.id === userId);
        if (index === -1) return current;
        userIndex = index;
        return [
          ...current.slice(0, index),
          { ...current[index], text: trimmed },
          {
            id: assistantId,
            role: 'assistant',
            text: '',
            thinking: '',
            toolCalls: [],
            state: 'thinking',
            segments: [],
            collapsedGroups: [],
          },
        ];
      });
      if (userIndex === -1) {
        setBusy(false);
        return;
      }
      await runAssistantStream(assistantId, (onEvent, signal) =>
        client.editTurnStream(
          session,
          turnIndex,
          trimmed,
          newId('turn'),
          onEvent,
          signal,
        ),
      );
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      setBusy(false);
    }
  }

  async function runAssistantStream(
    assistantId: string,
    apiCall: (
      onEvent: (event: TurnEvent) => void,
      signal: AbortSignal,
    ) => Promise<AgentTurnResult>,
  ) {
    setBusy(true);
    const abortController = new AbortController();
    streamAbortRef.current = abortController;
    try {
      const result = await apiCall((event: TurnEvent) => {
        setMessages((current) => {
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
              if (event.data.name === 'correct_asr' && sessionIdRef.current) {
                const args = event.data.arguments as
                  | { questions: UserInputQuestion[] }
                  | undefined;
                const questions = args?.questions ?? [];
                if (questions.length > 0) {
                  setUserInput({
                    sessionId: sessionIdRef.current,
                    callId: event.data.call_id,
                    questions,
                    values: questions.map((q) => q.text),
                  });
                }
              }
              const callIndex = (next.toolCalls ?? []).length;
              next.toolCalls = [
                ...(next.toolCalls ?? []),
                {
                  name: event.data.name,
                  callId: event.data.call_id,
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
              const match = calls.find((c) => c.callId === event.data.call_id);
              if (match) {
                if (event.data.name === 'correct_asr') {
                  match.result = '訂正を反映しました';
                  match.isError = false;
                } else {
                  match.result = event.data.content;
                  match.isError = event.data.is_error;
                }
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
      }, abortController.signal);
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
        setMessages((current) =>
          current.map((m) => {
            const fallback = m.text || '応答が中断されました';
            return m.id === assistantId
              ? {
                  ...m,
                  text: fallback,
                  state: 'done',
                  toolCalls: [],
                  segments: undefined,
                }
              : m;
          }),
        );
      } else {
        const message = e instanceof Error ? e.message : String(e);
        setMessages((current) =>
          current.map((m) =>
            m.id === assistantId
              ? {
                  ...m,
                  text: message,
                  state: 'done',
                  toolCalls: [],
                }
              : m,
          ),
        );
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
      setUserInput(null);
      setBusy(false);
    }
  }
  sendTextRef.current = sendText;

  async function send() {
    const value = text.trim();
    if (!value || busy || isSwitchingRef.current || !historyReady) return;
    setText('');
    setInputHeight(44);
    await sendText(value);
  }

  const appendText = useCallback((transcript: string) => {
    setText((current) => {
      const prefix =
        current && !current.endsWith(' ') ? `${current} ` : current;
      return `${prefix}${transcript.trim()}`;
    });
  }, []);

  const toggleGroupCollapsed = useCallback((id: string, groupIndex: number) => {
    setMessages((current) =>
      current.map((m) =>
        m.id === id
          ? { ...m, collapsedGroups: updateCollapsedGroup(m, groupIndex) }
          : m,
      ),
    );
  }, []);

  const handleMessageLongPress = useCallback(
    (message: Message, position: { x: number; y: number }) => {
      haptic.medium();
      setContextMenu({
        visible: true,
        messageId: message.id,
        position,
      });
    },
    [],
  );

  const contextMenuMessage = useMemo(
    () => messages.find((m) => m.id === contextMenu.messageId) ?? null,
    [messages, contextMenu.messageId],
  );

  const handleCopy = useCallback(async () => {
    if (!contextMenuMessage) return;
    await Clipboard.setStringAsync(contextMenuMessage.text);
    haptic.success();
    setContextMenu((current) => ({ ...current, visible: false }));
  }, [contextMenuMessage]);

  const handleEdit = useCallback(() => {
    if (!contextMenuMessage || busy || isSwitchingRef.current || !historyReady)
      return;
    setEditModal({
      visible: true,
      messageId: contextMenuMessage.id,
      text: contextMenuMessage.text,
    });
    setContextMenu((current) => ({ ...current, visible: false }));
  }, [contextMenuMessage, busy, historyReady]);

  function handleSaveEdit(newText: string) {
    setEditModal({ visible: false, messageId: '', text: '' });
    if (
      !sessionIdRef.current ||
      !newText.trim() ||
      busy ||
      isSwitchingRef.current ||
      !historyReady
    )
      return;
    const index = messages.findIndex((m) => m.id === editModal.messageId);
    if (index === -1 || messages[index].role !== 'user') return;
    const turnIndex = getTurnIndex(messages, index);
    editTurn(turnIndex, editModal.messageId, newText.trim());
  }

  const handleRevert = useCallback(async () => {
    if (
      !contextMenuMessage ||
      !sessionIdRef.current ||
      busy ||
      isSwitchingRef.current ||
      !historyReady
    )
      return;
    streamAbortRef.current?.abort();
    const index = messages.findIndex((m) => m.id === contextMenuMessage.id);
    if (index === -1 || index === messages.length - 1) {
      setContextMenu((current) => ({ ...current, visible: false }));
      return;
    }
    const turnIndex = getTurnIndex(messages, index);
    const afterUser = contextMenuMessage.role === 'user';
    haptic.medium();
    setBusy(true);
    try {
      await client.revertTurn(sessionIdRef.current, turnIndex, afterUser);
      setMessages((current) => current.slice(0, index + 1));
      setApproval(null);
      setUserInput(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setContextMenu((current) => ({ ...current, visible: false }));
    }
  }, [contextMenuMessage, messages, client, historyReady, busy]);

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

  const keyExtractor = useCallback((item: Message) => item.id, []);

  const handleMessagesLayout = useCallback((event: LayoutChangeEvent) => {
    setMessagesHeight(event.nativeEvent.layout.height);
  }, []);

  const renderItem = useCallback(
    ({ item }: { item: Message }) => {
      if (item.role === 'user') {
        return (
          <UserMessage message={item} onLongPress={handleMessageLongPress} />
        );
      }
      const isLatest = item.id === messages[messages.length - 1]?.id;
      return (
        <AssistantMessage
          item={item}
          colors={colors}
          markdownStyles={markdownStyles}
          markdownIt={markdownIt}
          markdownRules={markdownRules}
          availableHeight={messagesHeight}
          isLatest={isLatest}
          onToggleGroupCollapsed={toggleGroupCollapsed}
          onLongPress={handleMessageLongPress}
        />
      );
    },
    [
      messages,
      colors,
      markdownStyles,
      markdownIt,
      markdownRules,
      messagesHeight,
      toggleGroupCollapsed,
      handleMessageLongPress,
    ],
  );

  const listEmpty = useMemo(
    () => (
      <Text style={[styles.empty, { color: colors.gray }]}>
        何を予定しますか？
      </Text>
    ),
    [colors.gray],
  );

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

  async function submitUserInput(answers: UserInputAnswer[]) {
    if (!userInput) return;
    const { sessionId, callId } = userInput;
    setUserInput(null);
    try {
      await client.submitUserInput(sessionId, callId, answers);
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setError(`訂正の送信に失敗しました: ${message}`);
      streamAbortRef.current?.abort();
    }
  }

  function resetSwitchState() {
    setIsSwitching(false);
    isSwitchingRef.current = false;
  }

  function resetToCenter() {
    if (viewOffset.value === 0) {
      resetSwitchState();
      return;
    }
    viewOffset.value = withTiming(
      0,
      { duration: 200, easing: Easing.out(Easing.exp) },
      (finished) => {
        'worklet';
        if (finished) runOnJS(resetSwitchState)();
      },
    );
  }

  async function switchSession(delta: number) {
    if (isSwitchingRef.current || busy || !historyReady) {
      resetToCenter();
      return;
    }
    const nextIndex = activeIndexRef.current + delta;
    if (nextIndex < 0 || nextIndex >= sessionIdsRef.current.length) {
      resetToCenter();
      return;
    }
    const nextId = sessionIdsRef.current[nextIndex];
    const currentId = sessionIdRef.current;
    isSwitchingRef.current = true;
    setIsSwitching(true);
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
      sessionIdRef.current = nextId;
      setActiveIndex(nextIndex);
      activeIndexRef.current = nextIndex;
      setText('');
      autoScrollRef.current = true;
      setMessages(snapshot.messages);
      setApproval(snapshot.approval);
      saveSessionHistory({
        ids: sessionIdsRef.current,
        activeIndex: nextIndex,
      }).catch(() => {});
      resetToCenter();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      resetToCenter();
    }
  }

  async function startNewSession() {
    if (isSwitchingRef.current || busy || !historyReady) return;
    setError(null);
    isSwitchingRef.current = true;
    setIsSwitching(true);
    const currentId = sessionIdRef.current;
    const previousMessages = messages;
    const previousApproval = approval;
    if (currentId) {
      await saveSessionSnapshot(currentId, {
        messages: previousMessages,
        approval: previousApproval,
      }).catch(() => {});
    }
    skipSnapshotSaveRef.current = true;
    setMessages([]);
    setApproval(null);
    try {
      const created = await client.createSession();
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
      sessionIdRef.current = created;
      setText('');
      autoScrollRef.current = true;
      saveSessionHistory({ ids, activeIndex: nextIndex }).catch(() => {});
      resetToCenter();
    } catch (e: unknown) {
      sessionIdRef.current = currentId;
      setMessages(previousMessages);
      setApproval(previousApproval);
      setError(e instanceof Error ? e.message : String(e));
      resetToCenter();
    } finally {
      skipSnapshotSaveRef.current = false;
    }
  }

  const switcherGesture = Gesture.Pan()
    .minDistance(20)
    .onUpdate((e) => {
      const x = e.translationX;
      viewOffset.value = Math.max(-width, Math.min(width, x));
    })
    .onEnd((e) => {
      if (e.translationX > SWIPE_THRESHOLD) {
        runOnJS(switchSession)(-1);
      } else if (e.translationX < -SWIPE_THRESHOLD) {
        runOnJS(switchSession)(1);
      } else {
        viewOffset.value = withTiming(0, {
          duration: 200,
          easing: Easing.out(Easing.exp),
        });
      }
    });

  return (
    <KeyboardAvoidingView
      style={[styles.container, { backgroundColor: colors.white }]}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
      enabled={Platform.OS === 'ios' || keyboardVisible}
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
          keyExtractor={keyExtractor}
          onLayout={handleMessagesLayout}
          renderItem={renderItem}
          onScroll={handleScroll}
          scrollEventThrottle={16}
          onContentSizeChange={handleMessagesContentSizeChange}
          ListEmptyComponent={listEmpty}
          initialNumToRender={10}
          maxToRenderPerBatch={10}
          windowSize={5}
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
        {userInput && (
          <View
            style={[
              styles.userInputSheet,
              {
                borderColor: colors.separator,
                backgroundColor: colors.surface,
              },
            ]}
          >
            <Text style={[styles.userInputTitle, { color: colors.black }]}>
              認識結果を確認してください
            </Text>
            <Text style={[styles.userInputSubtitle, { color: colors.gray }]}>
              自明な部分は推測済みです。曖昧な語だけ確認します
            </Text>
            <ScrollView style={styles.userInputScroll}>
              {userInput.questions.map((q, i) => (
                <View
                  key={i}
                  style={[
                    styles.userInputCard,
                    {
                      borderColor: colors.separator,
                      backgroundColor: colors.surfaceTint,
                    },
                  ]}
                >
                  <Text style={[styles.userInputLabel, { color: colors.gray }]}>
                    元の認識テキスト
                  </Text>
                  <Text
                    style={[styles.userInputOriginal, { color: colors.black }]}
                  >
                    {q.text}
                  </Text>
                  <Text style={[styles.userInputLabel, { color: colors.gray }]}>
                    目的
                  </Text>
                  <Text
                    style={[styles.userInputPurpose, { color: colors.black }]}
                  >
                    {q.for}
                  </Text>
                  <TextInput
                    style={[
                      styles.userInputField,
                      {
                        color: colors.black,
                        borderColor: colors.separator,
                        backgroundColor: colors.surface,
                      },
                    ]}
                    value={userInput.values[i]}
                    onChangeText={(value) =>
                      setUserInput((current) =>
                        current
                          ? {
                              ...current,
                              values: current.values.map((v, idx) =>
                                idx === i ? value : v,
                              ),
                            }
                          : null,
                      )
                    }
                    placeholder={q.text}
                    placeholderTextColor={colors.gray}
                    editable={!isSwitching}
                  />
                </View>
              ))}
            </ScrollView>
            <View style={styles.userInputActions}>
              <Pressable
                disabled={isSwitching}
                onPress={() =>
                  submitUserInput(
                    userInput.questions.map((q) => ({ text: q.text })),
                  )
                }
                style={[styles.userInputButton, styles.userInputSecondary]}
              >
                <Text style={styles.userInputSecondaryText}>
                  そのまま続ける
                </Text>
              </Pressable>
              <Pressable
                disabled={isSwitching}
                onPress={() =>
                  submitUserInput(
                    userInput.values.map((value) => ({ text: value })),
                  )
                }
                style={[styles.userInputButton, styles.userInputPrimary]}
              >
                <Text style={styles.userInputPrimaryText}>確定</Text>
              </Pressable>
            </View>
          </View>
        )}
        {error && (
          <Pressable onPress={() => Alert.alert('Agentエラー', error)}>
            <Text style={styles.error}>{error}</Text>
          </Pressable>
        )}
        {sessionIds.length > 1 && (
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
        )}
        <View
          style={[
            styles.composer,
            {
              borderTopColor: colors.separator,
              paddingBottom: 12 + (keyboardVisible ? 0 : insets.bottom),
            },
          ]}
        >
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
          <ComposerRecordButton
            audioReady={audioReady}
            historyReady={historyReady}
            busy={busy || isSwitching}
            onAppend={appendText}
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

        <MessageContextMenu
          visible={contextMenu.visible}
          position={contextMenu.position}
          canEdit={!busy && contextMenuMessage?.role === 'user'}
          canRevert={
            !busy &&
            !!contextMenuMessage &&
            messages.findIndex((m) => m.id === contextMenuMessage.id) <
              messages.length - 1
          }
          onClose={() =>
            setContextMenu((current) => ({ ...current, visible: false }))
          }
          onCopy={handleCopy}
          onEdit={handleEdit}
          onRevert={handleRevert}
        />

        <EditMessageModal
          visible={editModal.visible}
          text={editModal.text}
          onClose={() =>
            setEditModal({ visible: false, messageId: '', text: '' })
          }
          onSave={handleSaveEdit}
        />
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
    flexGrow: 1,
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
  loadingIndicator: { alignItems: 'center', minWidth: 120 },
  thinkingCard: {
    borderRadius: 8,
    padding: 10,
    borderWidth: 1,
    gap: 4,
  },
  thinkingHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  thinkingLabel: { fontSize: 11, fontWeight: '700' },
  thinkingText: { fontSize: 12, fontStyle: 'italic' },
  contextFooter: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 4,
    paddingTop: 8,
    marginTop: 8,
    borderTopWidth: 1,
  },
  contextFooterText: { fontSize: 11 },
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
  userInputSheet: {
    margin: 12,
    padding: 12,
    borderWidth: 1,
    borderRadius: 16,
    gap: 10,
    maxHeight: '60%',
  },
  userInputTitle: { fontWeight: '700', fontSize: 16 },
  userInputSubtitle: { fontSize: 12 },
  userInputScroll: { maxHeight: 320 },
  userInputCard: {
    borderWidth: 1,
    borderRadius: 10,
    padding: 10,
    marginBottom: 10,
    gap: 4,
  },
  userInputLabel: { fontSize: 11, fontWeight: '600' },
  userInputOriginal: { fontSize: 15, fontWeight: '700' },
  userInputPurpose: { fontSize: 13, marginBottom: 6 },
  userInputField: {
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 10,
    paddingVertical: 8,
    fontSize: 14,
  },
  userInputActions: { flexDirection: 'row', gap: 8, marginTop: 4 },
  userInputButton: {
    flex: 1,
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
  },
  userInputSecondary: { borderWidth: 1, borderColor: '#B33A3A' },
  userInputSecondaryText: { color: '#B33A3A', fontWeight: '700' },
  userInputPrimary: { backgroundColor: BRAND_COLOR },
  userInputPrimaryText: { color: COLORS.white, fontWeight: '700' },
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
