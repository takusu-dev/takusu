import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useColors, BRAND_COLOR } from '@/src/theme';
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
import { LlmProviderEditor } from '@/src/components/settings/LlmProviderEditor';
import { TtsProviderEditor } from '@/src/components/settings/TtsProviderEditor';

function newId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}

const MODEL_SIZES: Record<string, string> = {
  hush: '約8 MB',
  'sherpa-sense-voice-int8': '約160 MB',
};

function normalizeLlmProvider(p: LlmProviderSettings): LlmProviderSettings {
  return {
    id: p.id,
    name: p.name,
    baseUrl: p.baseUrl,
    selectedModel: p.selectedModel,
    cachedModels: p.cachedModels,
    modelsFetchedAt: p.modelsFetchedAt,
    cost: p.cost,
  };
}

function newLlmProvider(): LlmProviderSettings {
  return {
    id: newId('llm'),
    name: 'Custom',
    baseUrl: '',
    selectedModel: '',
    cachedModels: [],
  };
}

function newTtsProvider(): TtsProviderSettings {
  return {
    id: newId('tts'),
    name: 'Cartesia',
    provider: 'cartesia',
    voiceId: '',
    language: 'ja',
    sampleRate: 44100,
  };
}

export function AgentSettingsView() {
  const colors = useColors();
  const { client } = useServer();

  const [llmProviders, setLlmProviders] = useState<LlmProviderSettings[]>([]);
  const [activeLlm, setActiveLlm] = useState<string | null>(null);
  const [ttsProviders, setTtsProviders] = useState<TtsProviderSettings[]>([]);
  const [activeTts, setActiveTts] = useState<string | null>(null);

  const [editingLlm, setEditingLlm] = useState<LlmProviderSettings | null>(
    null,
  );
  const [editingLlmKey, setEditingLlmKey] = useState('');
  const [editingTts, setEditingTts] = useState<TtsProviderSettings | null>(
    null,
  );
  const [editingTtsKey, setEditingTtsKey] = useState('');

  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    loadSettings()
      .then((settings) => {
        if (cancelled) return;
        setLlmProviders(settings.llmProviders.map(normalizeLlmProvider));
        setActiveLlm(settings.activeLlmProvider || null);
        setTtsProviders(settings.ttsProviders);
        setActiveTts(settings.activeTtsProvider || null);
      })
      .catch((e) => {
        Alert.alert('読み込み失敗', e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const editingLlmId = editingLlm?.id;
  useEffect(() => {
    let cancelled = false;
    if (!editingLlmId) {
      setEditingLlmKey('');
      return;
    }
    loadAgentApiKey('llm', editingLlmId).then((key) => {
      if (!cancelled) setEditingLlmKey(key);
    });
    return () => {
      cancelled = true;
    };
  }, [editingLlmId]);

  const editingTtsId = editingTts?.id;
  useEffect(() => {
    let cancelled = false;
    if (!editingTtsId) {
      setEditingTtsKey('');
      return;
    }
    loadAgentApiKey('tts', editingTtsId).then((key) => {
      if (!cancelled) setEditingTtsKey(key);
    });
    return () => {
      cancelled = true;
    };
  }, [editingTtsId]);

  async function setActiveLlmAndSave(id: string | null) {
    try {
      await saveAgentProviders(llmProviders, id, ttsProviders, activeTts);
      setActiveLlm(id);
    } catch (e) {
      Alert.alert('保存失敗', e instanceof Error ? e.message : String(e));
    }
  }

  async function setActiveTtsAndSave(id: string | null) {
    try {
      await saveAgentProviders(llmProviders, activeLlm, ttsProviders, id);
      setActiveTts(id);
    } catch (e) {
      Alert.alert('保存失敗', e instanceof Error ? e.message : String(e));
    }
  }

  async function saveLlm(provider: LlmProviderSettings, key: string) {
    setSaving(true);
    try {
      const existing = llmProviders.find((p) => p.id === provider.id);
      const updated = existing
        ? llmProviders.map((p) => (p.id === provider.id ? provider : p))
        : [...llmProviders, provider];
      const newActive = activeLlm ?? provider.id;
      await saveAgentApiKey('llm', provider.id, key);
      await saveAgentProviders(updated, newActive, ttsProviders, activeTts);
      setLlmProviders(updated);
      setActiveLlm(newActive);
      setEditingLlm(null);
      setEditingLlmKey('');
      Alert.alert(
        '保存しました',
        'LLM Providerを保存しました。サーバー再起動後に反映されます',
      );
    } catch (e) {
      Alert.alert('保存失敗', e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  async function saveTts(provider: TtsProviderSettings, key: string) {
    setSaving(true);
    try {
      const existing = ttsProviders.find((p) => p.id === provider.id);
      const updated = existing
        ? ttsProviders.map((p) => (p.id === provider.id ? provider : p))
        : [...ttsProviders, provider];
      const newActive = activeTts ?? provider.id;
      await saveAgentApiKey('tts', provider.id, key);
      await saveAgentProviders(llmProviders, activeLlm, updated, newActive);
      setTtsProviders(updated);
      setActiveTts(newActive);
      setEditingTts(null);
      setEditingTtsKey('');
      Alert.alert(
        '保存しました',
        'TTS Providerを保存しました。サーバー再起動後に反映されます',
      );
    } catch (e) {
      Alert.alert('保存失敗', e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  function deleteLlm(id: string) {
    Alert.alert('削除', 'このLLM Providerを削除しますか？', [
      { text: 'キャンセル', style: 'cancel' },
      {
        text: '削除',
        style: 'destructive',
        onPress: async () => {
          setSaving(true);
          try {
            const updated = llmProviders.filter((p) => p.id !== id);
            const newActive =
              activeLlm === id ? (updated[0]?.id ?? null) : activeLlm;
            await deleteAgentApiKey('llm', id);
            await saveAgentProviders(
              updated,
              newActive,
              ttsProviders,
              activeTts,
            );
            setLlmProviders(updated);
            if (newActive !== activeLlm) setActiveLlm(newActive);
            if (editingLlm?.id === id) setEditingLlm(null);
          } catch (e) {
            Alert.alert('削除失敗', e instanceof Error ? e.message : String(e));
          } finally {
            setSaving(false);
          }
        },
      },
    ]);
  }

  function deleteTts(id: string) {
    Alert.alert('削除', 'このTTS Providerを削除しますか？', [
      { text: 'キャンセル', style: 'cancel' },
      {
        text: '削除',
        style: 'destructive',
        onPress: async () => {
          setSaving(true);
          try {
            const updated = ttsProviders.filter((p) => p.id !== id);
            const newActive =
              activeTts === id ? (updated[0]?.id ?? null) : activeTts;
            await deleteAgentApiKey('tts', id);
            await saveAgentProviders(
              llmProviders,
              activeLlm,
              updated,
              newActive,
            );
            setTtsProviders(updated);
            if (newActive !== activeTts) setActiveTts(newActive);
            if (editingTts?.id === id) setEditingTts(null);
          } catch (e) {
            Alert.alert('削除失敗', e instanceof Error ? e.message : String(e));
          } finally {
            setSaving(false);
          }
        },
      },
    ]);
  }

  function removeAll() {
    Alert.alert('削除', 'すべてのProvider設定を削除しますか？', [
      { text: 'キャンセル', style: 'cancel' },
      {
        text: '削除',
        style: 'destructive',
        onPress: async () => {
          setSaving(true);
          try {
            await Promise.all(
              llmProviders.map((p) => deleteAgentApiKey('llm', p.id)),
            );
            await Promise.all(
              ttsProviders.map((p) => deleteAgentApiKey('tts', p.id)),
            );
            await saveAgentProviders([], null, [], null);
            setLlmProviders([]);
            setActiveLlm(null);
            setTtsProviders([]);
            setActiveTts(null);
            setEditingLlm(null);
            setEditingLlmKey('');
            setEditingTts(null);
            setEditingTtsKey('');
            Alert.alert(
              '削除しました',
              'Provider設定を削除しました。サーバー再起動後に反映されます',
            );
          } catch (e) {
            Alert.alert('削除失敗', e instanceof Error ? e.message : String(e));
          } finally {
            setSaving(false);
          }
        },
      },
    ]);
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

  function promptModelDownload(modelId: string) {
    const size = MODEL_SIZES[modelId];
    const message = size
      ? `${size}のデータをダウンロードします。よろしいですか？`
      : 'データをダウンロードします。よろしいですか？';
    Alert.alert('ダウンロード確認', message, [
      { text: 'いいえ', style: 'cancel' },
      { text: 'はい', onPress: () => startModelDownload(modelId) },
    ]);
  }

  if (loading) {
    return (
      <View style={[styles.loading, { backgroundColor: colors.white }]}>
        <ActivityIndicator />
      </View>
    );
  }

  return (
    <ScrollView contentContainerStyle={styles.content}>
      <Text style={[styles.heading, { color: colors.black }]}>
        LLM Provider
      </Text>
      {llmProviders.length === 0 && (
        <Text style={{ color: colors.gray }}>Providerを追加してください</Text>
      )}
      {llmProviders.map((provider) => (
        <View
          key={provider.id}
          style={[styles.row, { borderColor: colors.separator }]}
        >
          <Pressable
            onPress={() => setActiveLlmAndSave(provider.id)}
            style={styles.radio}
            disabled={saving}
          >
            <Text
              style={{
                color: activeLlm === provider.id ? BRAND_COLOR : colors.black,
              }}
            >
              {activeLlm === provider.id ? '●' : '○'}
            </Text>
          </Pressable>
          <View style={styles.rowText}>
            <Text style={{ color: colors.black, fontWeight: '600' }}>
              {provider.name}
            </Text>
            <Text style={{ color: colors.gray, fontSize: 12 }}>
              {provider.selectedModel || '未設定'}
              {provider.cost ? ` · ${provider.cost}` : ''}
            </Text>
          </View>
          <Pressable
            onPress={() => setEditingLlm({ ...provider })}
            style={[styles.editButton, { borderColor: colors.separator }]}
          >
            <Text style={{ color: colors.black }}>編集</Text>
          </Pressable>
        </View>
      ))}
      <Pressable
        onPress={() => setEditingLlm(newLlmProvider())}
        style={[styles.addButton, { borderColor: BRAND_COLOR }]}
      >
        <Text style={{ color: BRAND_COLOR }}>+ LLM Providerを追加</Text>
      </Pressable>
      {editingLlm && (
        <LlmProviderEditor
          provider={editingLlm}
          apiKey={editingLlmKey}
          onChangeProvider={setEditingLlm}
          onChangeApiKey={setEditingLlmKey}
          onSave={() => saveLlm(editingLlm, editingLlmKey)}
          onCancel={() => setEditingLlm(null)}
          onDelete={
            llmProviders.some((p) => p.id === editingLlm.id)
              ? () => deleteLlm(editingLlm.id)
              : undefined
          }
          saving={saving}
        />
      )}

      <Text style={[styles.heading, { color: colors.black }]}>音声モデル</Text>
      <Pressable
        onPress={() => promptModelDownload('hush')}
        style={styles.secondary}
      >
        <Text style={{ color: colors.black }}>Hushノイズ除去を準備</Text>
      </Pressable>
      <Pressable
        onPress={() => promptModelDownload('sherpa-sense-voice-int8')}
        style={styles.secondary}
      >
        <Text style={{ color: colors.black }}>SenseVoice音声認識を準備</Text>
      </Pressable>

      <Text style={[styles.heading, { color: colors.black }]}>
        TTS Provider
      </Text>
      {ttsProviders.length === 0 && (
        <Text style={{ color: colors.gray }}>Providerを追加してください</Text>
      )}
      {ttsProviders.map((provider) => (
        <View
          key={provider.id}
          style={[styles.row, { borderColor: colors.separator }]}
        >
          <Pressable
            onPress={() => setActiveTtsAndSave(provider.id)}
            style={styles.radio}
            disabled={saving}
          >
            <Text
              style={{
                color: activeTts === provider.id ? BRAND_COLOR : colors.black,
              }}
            >
              {activeTts === provider.id ? '●' : '○'}
            </Text>
          </Pressable>
          <View style={styles.rowText}>
            <Text style={{ color: colors.black, fontWeight: '600' }}>
              {provider.name}
            </Text>
            <Text style={{ color: colors.gray, fontSize: 12 }}>
              {provider.provider} · {provider.voiceId || '未設定'}
            </Text>
          </View>
          <Pressable
            onPress={() => setEditingTts({ ...provider })}
            style={[styles.editButton, { borderColor: colors.separator }]}
          >
            <Text style={{ color: colors.black }}>編集</Text>
          </Pressable>
        </View>
      ))}
      <Pressable
        onPress={() => setEditingTts(newTtsProvider())}
        style={[styles.addButton, { borderColor: BRAND_COLOR }]}
      >
        <Text style={{ color: BRAND_COLOR }}>+ TTS Providerを追加</Text>
      </Pressable>
      {editingTts && (
        <TtsProviderEditor
          provider={editingTts}
          apiKey={editingTtsKey}
          onChangeProvider={setEditingTts}
          onChangeApiKey={setEditingTtsKey}
          onSave={() => saveTts(editingTts, editingTtsKey)}
          onCancel={() => setEditingTts(null)}
          onDelete={
            ttsProviders.some((p) => p.id === editingTts.id)
              ? () => deleteTts(editingTts.id)
              : undefined
          }
          saving={saving}
        />
      )}

      <Pressable onPress={removeAll} style={styles.remove}>
        <Text style={styles.removeText}>Provider設定をすべて削除</Text>
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
  loading: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  content: { padding: 16, gap: 10 },
  heading: { fontSize: 18, fontWeight: '700', marginTop: 12 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 10,
    borderWidth: 1,
    borderRadius: 8,
    gap: 8,
  },
  radio: { width: 28, alignItems: 'center', justifyContent: 'center' },
  rowText: { flex: 1 },
  editButton: {
    paddingVertical: 6,
    paddingHorizontal: 12,
    borderWidth: 1,
    borderRadius: 8,
  },
  addButton: {
    minHeight: 44,
    borderWidth: 1,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  secondary: {
    minHeight: 44,
    borderWidth: 1,
    borderColor: BRAND_COLOR,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  remove: { alignItems: 'center', padding: 12, marginTop: 8 },
  removeText: { color: '#B33A3A' },
});
