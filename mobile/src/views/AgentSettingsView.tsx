import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useColors, BRAND_COLOR, COLORS } from '@/src/theme';
import { useServer } from '@/src/api/ServerProvider';
import TakusuServerModule from '../../modules/takusu-server/src/TakusuServerModule';
import {
  deleteAgentApiKey,
  loadAgentApiKey,
  loadSettings,
  saveAgentApiKey,
  saveAgentProviders,
  type LlmProviderSettings,
  type TtsProviderSettings,
} from '@/src/api/settingsStore';

const DEFAULT_LLM: LlmProviderSettings = {
  id: 'llm-default',
  name: 'OpenAI',
  provider: 'openai',
  baseUrl: 'https://api.openai.com/v1',
  selectedModel: '',
  cachedModels: [],
};

const DEFAULT_TTS: TtsProviderSettings = {
  id: 'tts-default',
  name: 'Cartesia',
  provider: 'cartesia',
  voiceId: '',
  language: 'ja',
  sampleRate: 44100,
};

export function AgentSettingsView() {
  const colors = useColors();
  const { client } = useServer();
  const [llm, setLlm] = useState<LlmProviderSettings>(DEFAULT_LLM);
  const [tts, setTts] = useState<TtsProviderSettings>(DEFAULT_TTS);
  const [llmKey, setLlmKey] = useState('');
  const [ttsKey, setTtsKey] = useState('');
  const [modelsLoading, setModelsLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    loadSettings().then(async (settings) => {
      if (cancelled) return;
      const savedLlm = settings.llmProviders[0] ?? DEFAULT_LLM;
      const savedTts = settings.ttsProviders[0] ?? DEFAULT_TTS;
      setLlm(savedLlm);
      setTts(savedTts);
      const [llmSecret, ttsSecret] = await Promise.all([
        loadAgentApiKey('llm', savedLlm.id),
        loadAgentApiKey('tts', savedTts.id),
      ]);
      if (!cancelled) {
        setLlmKey(llmSecret);
        setTtsKey(ttsSecret);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  async function fetchModels() {
    if (!llm.baseUrl.trim() || !llmKey.trim()) {
      Alert.alert('入力不足', 'base URLとAPI keyを入力してください');
      return;
    }
    setModelsLoading(true);
    try {
      const response = await fetch(
        `${llm.baseUrl.replace(/\/+$/, '')}/models`,
        {
          headers: { Authorization: `Bearer ${llmKey.trim()}` },
        },
      );
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const body = (await response.json()) as { data?: Array<{ id?: string }> };
      const models = [
        ...new Set(
          (body.data ?? [])
            .map((item) => item.id)
            .filter((id): id is string => Boolean(id)),
        ),
      ].sort();
      setLlm((current) => ({
        ...current,
        cachedModels: models,
        selectedModel: current.selectedModel || models[0] || '',
        modelsFetchedAt: new Date().toISOString(),
      }));
      if (models.length === 0)
        Alert.alert('モデルなし', 'モデルIDを手入力してください');
    } catch (e) {
      Alert.alert('取得失敗', e instanceof Error ? e.message : String(e));
    } finally {
      setModelsLoading(false);
    }
  }

  async function save() {
    if (!llm.selectedModel.trim()) {
      Alert.alert('入力不足', 'LLMモデルを選択または入力してください');
      return;
    }
    setSaving(true);
    try {
      await saveAgentProviders([llm], llm.id, [tts], tts.id);
      await Promise.all([
        saveAgentApiKey('llm', llm.id, llmKey.trim()),
        saveAgentApiKey('tts', tts.id, ttsKey.trim()),
      ]);
      Alert.alert(
        '保存しました',
        'Agent設定を保存しました。サーバー再起動後に反映されます',
      );
    } catch (e) {
      Alert.alert('保存失敗', e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  function startModelDownload(modelId: string) {
    try {
      TakusuServerModule.startModelDownload(modelId);
      Alert.alert(
        'ダウンロード開始',
        'バックグラウンドで音声モデルを準備します。通知で進捗を確認できます',
      );
    } catch (e) {
      Alert.alert('開始失敗', e instanceof Error ? e.message : String(e));
    }
  }

  async function remove() {
    await Promise.all([
      deleteAgentApiKey('llm', llm.id),
      deleteAgentApiKey('tts', tts.id),
      saveAgentProviders([], null, [], null),
    ]);
    setLlm(DEFAULT_LLM);
    setTts(DEFAULT_TTS);
    setLlmKey('');
    setTtsKey('');
  }

  return (
    <ScrollView contentContainerStyle={styles.content}>
      <Text style={[styles.heading, { color: colors.black }]}>
        LLM Provider
      </Text>
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={llm.name}
        onChangeText={(name) => setLlm({ ...llm, name })}
        placeholder="表示名"
      />
      <View style={styles.providerRow}>
        {(['openai', 'openrouter', 'custom'] as const).map((provider) => (
          <Pressable
            key={provider}
            onPress={() => setLlm({ ...llm, provider })}
            style={[
              styles.chip,
              {
                backgroundColor:
                  llm.provider === provider ? BRAND_COLOR : colors.separator,
              },
            ]}
          >
            <Text
              style={{
                color: llm.provider === provider ? COLORS.white : colors.black,
              }}
            >
              {provider}
            </Text>
          </Pressable>
        ))}
      </View>
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={llm.baseUrl}
        onChangeText={(baseUrl) => setLlm({ ...llm, baseUrl })}
        autoCapitalize="none"
        placeholder="Base URL"
      />
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={llmKey}
        onChangeText={setLlmKey}
        autoCapitalize="none"
        secureTextEntry
        placeholder="API key"
      />
      <Pressable
        onPress={fetchModels}
        style={styles.secondary}
        disabled={modelsLoading}
      >
        {modelsLoading ? <ActivityIndicator /> : <Text>モデルを取得</Text>}
      </Pressable>
      {llm.cachedModels.map((model) => (
        <Pressable
          key={model}
          onPress={() => setLlm({ ...llm, selectedModel: model })}
          style={[styles.modelRow, { borderColor: colors.separator }]}
        >
          <Text style={{ color: colors.black }}>
            {llm.selectedModel === model ? '● ' : '○ '}
            {model}
          </Text>
        </Pressable>
      ))}
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={llm.selectedModel}
        onChangeText={(selectedModel) => setLlm({ ...llm, selectedModel })}
        autoCapitalize="none"
        placeholder="モデルID（手入力可）"
      />

      <Text style={[styles.heading, { color: colors.black }]}>音声モデル</Text>
      <Pressable
        onPress={() => startModelDownload('hush')}
        style={styles.secondary}
      >
        <Text>Hushノイズ除去を準備</Text>
      </Pressable>
      <Pressable
        onPress={() => startModelDownload('sherpa-sense-voice-int8')}
        style={styles.secondary}
      >
        <Text>SenseVoice音声認識を準備</Text>
      </Pressable>

      <Text style={[styles.heading, { color: colors.black }]}>
        TTS Provider
      </Text>
      <View style={styles.providerRow}>
        <View style={[styles.chip, { backgroundColor: BRAND_COLOR }]}>
          <Text style={{ color: COLORS.white }}>cartesia</Text>
        </View>
      </View>
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={tts.voiceId}
        onChangeText={(voiceId) => setTts({ ...tts, voiceId })}
        autoCapitalize="none"
        placeholder="Voice ID"
      />
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={ttsKey}
        onChangeText={setTtsKey}
        autoCapitalize="none"
        secureTextEntry
        placeholder="Cartesia API key"
      />
      <Pressable onPress={save} style={styles.save} disabled={saving}>
        {saving ? (
          <ActivityIndicator color={COLORS.white} />
        ) : (
          <Text style={styles.saveText}>保存</Text>
        )}
      </Pressable>
      <Pressable onPress={remove} style={styles.remove}>
        <Text style={styles.removeText}>Provider設定を削除</Text>
      </Pressable>
      {!client && (
        <Text style={{ color: colors.gray }}>
          Planner serverに接続していません
        </Text>
      )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: 16, gap: 10 },
  heading: { fontSize: 18, fontWeight: '700', marginTop: 12 },
  input: {
    minHeight: 44,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
  },
  providerRow: { flexDirection: 'row', gap: 8 },
  chip: { paddingVertical: 9, paddingHorizontal: 12, borderRadius: 8 },
  secondary: {
    minHeight: 44,
    borderWidth: 1,
    borderColor: BRAND_COLOR,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  modelRow: { padding: 10, borderWidth: 1, borderRadius: 8 },
  save: {
    minHeight: 48,
    borderRadius: 8,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
    marginTop: 12,
  },
  saveText: { color: COLORS.white, fontWeight: '700' },
  remove: { alignItems: 'center', padding: 12 },
  removeText: { color: '#B33A3A' },
});
