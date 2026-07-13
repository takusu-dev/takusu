import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  FlatList,
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
import AsyncStorage from '@react-native-async-storage/async-storage';
import { DEFAULT_PORT, useServer } from '@/src/api/ServerProvider';
import TakusuAudioModule from '../../modules/takusu-server/src/TakusuAudioModule';
import { loadAgentApiKey, loadSettings } from '@/src/api/settingsStore';
import {
  AgentClient,
  AgentApiError,
  type AgentTurnResult,
} from '@/src/api/agentClient';
import type { ApprovalRequest } from '@/src/api/agentTypes';
import { BRAND_COLOR, COLORS, useColors } from '@/src/theme';

interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
}

const SESSION_KEY = 'takusu.agent.sessionId';

function newId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function AgentView() {
  const router = useRouter();
  const colors = useColors();
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
    try {
      const session = await ensureSession();
      const result: AgentTurnResult = await client.runTurn(
        session,
        value.trim(),
        newId('turn'),
      );
      setMessages((current) => [
        ...current,
        { id: newId('assistant'), role: 'assistant', text: result.text },
      ]);
      setApproval(result.approval_request);
      if (audioReady && result.text.trim()) {
        await TakusuAudioModule.synthesizeAndPlay(result.text);
      }
    } catch (e: unknown) {
      if (e instanceof AgentApiError && e.status === 404) {
        await AsyncStorage.removeItem(SESSION_KEY);
        setSessionId(null);
        setApproval(null);
        setError('Agentセッションが終了しました。もう一度送信してください');
      } else {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(false);
    }
  }

  async function send() {
    const value = text.trim();
    if (!value || busy) return;
    setText('');
    await sendText(value);
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
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
    >
      <View style={[styles.header, { borderBottomColor: colors.separator }]}>
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
        renderItem={({ item }) => (
          <View
            style={[
              styles.bubble,
              item.role === 'user' ? styles.userBubble : styles.assistantBubble,
              {
                backgroundColor:
                  item.role === 'user' ? BRAND_COLOR : colors.separator,
              },
            ]}
          >
            <Text
              style={{
                color: item.role === 'user' ? COLORS.white : colors.black,
              }}
            >
              {item.text}
            </Text>
          </View>
        )}
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
      <View style={[styles.composer, { borderTopColor: colors.separator }]}>
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
            { color: colors.black, borderColor: colors.separator },
          ]}
          value={text}
          onChangeText={setText}
          placeholder="メッセージ"
          placeholderTextColor={colors.gray}
          editable={!busy}
          onSubmitEditing={send}
          returnKeyType="send"
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
    paddingTop: 48,
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
  composer: { flexDirection: 'row', gap: 8, padding: 12, borderTopWidth: 1 },
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
