import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  FlatList,
  Keyboard,
  KeyboardAvoidingView,
  PermissionsAndroid,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useRouter } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { DEFAULT_PORT, useServer } from '@/src/api/ServerProvider';
import TakusuAudioModule from '../../modules/takusu-server/src/TakusuAudioModule';
import { loadAgentApiKey, loadSettings } from '@/src/api/settingsStore';
import { AgentClient, AgentApiError } from '@/src/api/agentClient';
import type { ApprovalRequest, TurnEvent } from '@/src/api/agentTypes';
import { BRAND_COLOR, COLORS, useColors } from '@/src/theme';

interface ToolCallItem {
  name: string;
  arguments?: unknown;
  result?: string;
  isError?: boolean;
}

interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  thinking?: string;
  toolCalls?: ToolCallItem[];
  state?: 'thinking' | 'tool_call' | 'answering' | 'done';
  collapsed?: boolean;
}

const SESSION_KEY = 'takusu.agent.sessionId';

function newId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function AgentView() {
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const { workersToken, ready } = useServer();
  const client = useMemo(
    () => new AgentClient(`http://127.0.0.1:${DEFAULT_PORT}`, workersToken),
    [workersToken],
  );
  const [messages, setMessages] = useState<Message[]>([]);
  const [text, setText] = useState('');
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [approval, setApproval] = useState<ApprovalRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);
  const [audioReady, setAudioReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [keyboardVisible, setKeyboardVisible] = useState(Keyboard.isVisible());
  const [inputHeight, setInputHeight] = useState(44);

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

  const ensureSession = useCallback(async (): Promise<string> => {
    if (sessionId) return sessionId;
    const stored = await AsyncStorage.getItem(SESSION_KEY);
    if (stored) {
      try {
        const pending = await client.getApproval(stored);
        setSessionId(stored);
        setApproval(pending);
        return stored;
      } catch {
        await AsyncStorage.removeItem(SESSION_KEY);
      }
    }
    const created = await client.createSession();
    await AsyncStorage.setItem(SESSION_KEY, created);
    setSessionId(created);
    return created;
  }, [client, sessionId]);

  useEffect(() => {
    if (!ready || !workersToken) return;
    ensureSession().catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
    loadSettings()
      .then(async (settings) => {
        const provider = settings.ttsProviders.find(
          (item) => item.id === settings.activeTtsProvider,
        );
        if (!provider) return;
        const apiKey = await loadAgentApiKey('tts', provider.id);
        await TakusuAudioModule.configure({
          modelDir: '',
          apiKey,
          voiceId: provider.voiceId,
          language: provider.language,
          sampleRate: provider.sampleRate,
        });
        setAudioReady(true);
      })
      .catch((e: unknown) => {
        setAudioReady(false);
        setError(
          `音声モデルを準備してください: ${e instanceof Error ? e.message : String(e)}`,
        );
      });
  }, [ensureSession, ready, workersToken]);

  async function sendText(value: string) {
    if (!value.trim() || busy) return;
    setError(null);
    setMessages((current) => [
      ...current,
      { id: newId('user'), role: 'user', text: value.trim() },
    ]);
    setBusy(true);
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
          collapsed: false,
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
                next.state = 'thinking';
                break;
              case 'ToolCall':
                next.toolCalls = [
                  ...(next.toolCalls ?? []),
                  {
                    name: event.data.name,
                    arguments: event.data.arguments,
                  },
                ];
                next.state = 'tool_call';
                break;
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
                next.state = 'answering';
                next.collapsed = true;
                break;
              case 'Error':
                next.text = event.data;
                next.state = 'done';
                break;
              case 'Done':
                next.text = event.data.text;
                next.state = 'done';
                next.collapsed = true;
                break;
            }
            return [
              ...current.slice(0, index),
              next,
              ...current.slice(index + 1),
            ];
          });
        },
      );
      setApproval(result.approval_request);
      if (audioReady && result.text.trim()) {
        await TakusuAudioModule.synthesizeAndPlay(result.text);
      }
    } catch (e: unknown) {
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
        await AsyncStorage.removeItem(SESSION_KEY);
        setSessionId(null);
        setApproval(null);
        setError('Agentセッションが終了しました。もう一度送信してください');
      } else {
        setError(message);
      }
    } finally {
      setBusy(false);
    }
  }

  async function send() {
    const value = text.trim();
    if (!value || busy) return;
    setText('');
    setInputHeight(44);
    await sendText(value);
  }

  function toggleCollapsed(id: string) {
    setMessages((current) =>
      current.map((m) => (m.id === id ? { ...m, collapsed: !m.collapsed } : m)),
    );
  }

  async function toggleRecording() {
    if (busy) return;
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
    if (!sessionId || !approval || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await client.resolveApproval(
        sessionId,
        approval.id,
        approve,
        newId('approval'),
      );
      setApproval(null);
      setMessages((current) => [
        ...current,
        {
          id: newId('assistant'),
          role: 'assistant',
          text: result.approved
            ? '変更を適用しました。'
            : '変更を取り消しました。',
        },
      ]);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <KeyboardAvoidingView
      style={[styles.container, { backgroundColor: colors.white }]}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
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
        <View style={styles.headerSpace} />
      </View>
      <FlatList
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
          const hasContext =
            (item.thinking && item.thinking.length > 0) ||
            (item.toolCalls && item.toolCalls.length > 0);
          return (
            <View
              style={[
                styles.bubble,
                styles.assistantBubble,
                { backgroundColor: colors.separator },
              ]}
            >
              {hasContext && (
                <Pressable
                  onPress={() => toggleCollapsed(item.id)}
                  style={styles.contextHeader}
                >
                  <Text
                    style={[styles.contextHeaderText, { color: colors.black }]}
                  >
                    {item.collapsed ? '▶ ' : '▼ '}
                    {item.thinking && item.thinking.length > 0 ? '考え中' : ''}
                    {item.thinking &&
                    item.toolCalls &&
                    item.toolCalls.length > 0
                      ? ' / '
                      : ''}
                    {item.toolCalls && item.toolCalls.length > 0
                      ? `ツール (${item.toolCalls.length})`
                      : ''}
                  </Text>
                </Pressable>
              )}
              {!item.collapsed && hasContext && (
                <View style={styles.contextBody}>
                  {item.thinking && item.thinking.length > 0 && (
                    <Text style={[styles.thinkingText, { color: colors.gray }]}>
                      {item.thinking}
                    </Text>
                  )}
                  {item.toolCalls?.map((call, index) => (
                    <View key={index} style={styles.toolCall}>
                      <Text style={{ color: colors.black, fontWeight: '700' }}>
                        {call.name}
                      </Text>
                      {call.arguments !== undefined && (
                        <Text style={[styles.toolArgs, { color: colors.gray }]}>
                          {JSON.stringify(call.arguments, null, 2)}
                        </Text>
                      )}
                      {call.result !== undefined && (
                        <Text
                          style={{
                            color: call.isError ? '#B33A3A' : '#2E7D32',
                          }}
                        >
                          {call.isError
                            ? `エラー: ${call.result}`
                            : call.result}
                        </Text>
                      )}
                    </View>
                  ))}
                </View>
              )}
              {item.text.length > 0 && (
                <Text
                  style={{
                    color: colors.black,
                    marginTop: hasContext ? 8 : 0,
                  }}
                >
                  {item.text}
                </Text>
              )}
              {item.state !== 'done' && item.text.length === 0 && (
                <ActivityIndicator color={BRAND_COLOR} />
              )}
            </View>
          );
        }}
        ListEmptyComponent={
          <Text style={[styles.empty, { color: colors.gray }]}>
            何を予定しますか？
          </Text>
        }
      />
      {approval && (
        <View style={[styles.approval, { borderColor: colors.separator }]}>
          <Text style={[styles.approvalTitle, { color: colors.black }]}>
            確認が必要です
          </Text>
          <Text style={{ color: colors.black }}>{approval.why}</Text>
          {approval.changes.map((change, index) => (
            <Text
              key={`${change.operation}-${index}`}
              style={{ color: colors.black }}
            >
              ・{change.description}
            </Text>
          ))}
          {approval.warnings.map((warning) => (
            <Text key={warning} style={{ color: '#A65B00' }}>
              注意: {warning}
            </Text>
          ))}
          <View style={styles.approvalActions}>
            <Pressable
              disabled={busy}
              onPress={() => resolve(false)}
              style={styles.deny}
            >
              <Text style={styles.denyText}>拒否</Text>
            </Pressable>
            <Pressable
              disabled={busy}
              onPress={() => resolve(true)}
              style={styles.approve}
            >
              {busy ? (
                <ActivityIndicator color={COLORS.white} />
              ) : (
                <Text style={styles.approveText}>承認</Text>
              )}
            </Pressable>
          </View>
        </View>
      )}
      {error && (
        <Pressable onPress={() => Alert.alert('Agentエラー', error)}>
          <Text style={styles.error}>{error}</Text>
        </Pressable>
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
        <Pressable
          disabled={busy}
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
          editable={!busy}
          multiline
          textAlignVertical="top"
          onContentSizeChange={(e) => {
            const h = e.nativeEvent.contentSize.height;
            setInputHeight(Math.max(44, Math.min(120, h)));
          }}
        />
        <Pressable
          disabled={busy || !text.trim()}
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
  headerSpace: { width: 56 },
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
  contextHeader: { marginBottom: 4 },
  contextHeaderText: { fontSize: 12, fontWeight: '700' },
  contextBody: { gap: 6, marginBottom: 8 },
  thinkingText: { fontSize: 12, fontStyle: 'italic' },
  toolCall: {
    backgroundColor: 'rgba(0,0,0,0.03)',
    borderRadius: 8,
    padding: 8,
    gap: 4,
  },
  toolArgs: { fontSize: 11, fontFamily: 'monospace' },
  approval: {
    margin: 12,
    padding: 12,
    borderWidth: 1,
    borderRadius: 12,
    gap: 6,
  },
  approvalTitle: { fontWeight: '700', fontSize: 16 },
  approvalActions: { flexDirection: 'row', gap: 8, marginTop: 8 },
  deny: {
    flex: 1,
    padding: 12,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: '#B33A3A',
    alignItems: 'center',
  },
  denyText: { color: '#B33A3A', fontWeight: '700' },
  approve: {
    flex: 1,
    padding: 12,
    borderRadius: 8,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
  },
  approveText: { color: COLORS.white, fontWeight: '700' },
  error: { color: '#B33A3A', paddingHorizontal: 16, paddingBottom: 8 },
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
