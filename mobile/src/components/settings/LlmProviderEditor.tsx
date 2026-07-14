import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useColors, BRAND_COLOR, COLORS } from '@/src/theme';
import { type LlmProviderSettings } from '@/src/api/settingsStore';

interface Props {
  provider: LlmProviderSettings;
  apiKey: string;
  onChangeProvider: (provider: LlmProviderSettings) => void;
  onChangeApiKey: (apiKey: string) => void;
  onSave: () => void;
  onCancel: () => void;
  onDelete?: () => void;
  saving?: boolean;
}

export function LlmProviderEditor({
  provider,
  apiKey,
  onChangeProvider,
  onChangeApiKey,
  onSave,
  onCancel,
  onDelete,
  saving,
}: Props) {
  const colors = useColors();
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelFilter, setModelFilter] = useState('');

  const filteredModels = useMemo(() => {
    const query = modelFilter.trim().toLowerCase();
    if (!query) return provider.cachedModels;
    return provider.cachedModels.filter((model) =>
      model.toLowerCase().includes(query),
    );
  }, [provider.cachedModels, modelFilter]);

  async function fetchModels() {
    if (!provider.baseUrl.trim() || !apiKey.trim()) {
      Alert.alert('入力不足', 'base URLとAPI keyを入力してください');
      return;
    }
    setModelsLoading(true);
    try {
      const response = await fetch(
        `${provider.baseUrl.replace(/\/+$/, '')}/models`,
        {
          headers: { Authorization: `Bearer ${apiKey.trim()}` },
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
      onChangeProvider({
        ...provider,
        cachedModels: models,
        selectedModel: provider.selectedModel || models[0] || '',
        modelsFetchedAt: new Date().toISOString(),
      });
      if (models.length === 0) {
        Alert.alert('モデルなし', 'モデルIDを手入力してください');
      }
    } catch (e) {
      Alert.alert('取得失敗', e instanceof Error ? e.message : String(e));
    } finally {
      setModelsLoading(false);
    }
  }

  function handleSave() {
    if (!provider.selectedModel.trim()) {
      Alert.alert('入力不足', 'LLMモデルを選択または入力してください');
      return;
    }
    onSave();
  }

  return (
    <View style={[styles.editor, { borderColor: colors.separator }]}>
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={provider.name}
        onChangeText={(name) => onChangeProvider({ ...provider, name })}
        placeholder="表示名"
      />
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={provider.baseUrl}
        onChangeText={(baseUrl) => onChangeProvider({ ...provider, baseUrl })}
        autoCapitalize="none"
        placeholder="Base URL"
      />
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={apiKey}
        onChangeText={onChangeApiKey}
        autoCapitalize="none"
        secureTextEntry
        placeholder="API key"
      />
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={provider.cost ?? ''}
        onChangeText={(cost) => onChangeProvider({ ...provider, cost })}
        autoCapitalize="none"
        placeholder="Cost（例: $0.005 / 1K tokens）"
      />
      <Pressable
        onPress={fetchModels}
        style={styles.secondary}
        disabled={modelsLoading}
      >
        {modelsLoading ? (
          <ActivityIndicator />
        ) : (
          <Text style={{ color: colors.black }}>モデルを取得</Text>
        )}
      </Pressable>
      {provider.cachedModels.length > 0 && (
        <TextInput
          style={[
            styles.input,
            { color: colors.black, borderColor: colors.separator },
          ]}
          value={modelFilter}
          onChangeText={setModelFilter}
          autoCapitalize="none"
          placeholder="モデルを検索"
        />
      )}
      {filteredModels.map((model) => (
        <Pressable
          key={model}
          onPress={() =>
            onChangeProvider({ ...provider, selectedModel: model })
          }
          style={[styles.modelRow, { borderColor: colors.separator }]}
        >
          <Text style={{ color: colors.black }}>
            {provider.selectedModel === model ? '● ' : '○ '}
            {model}
          </Text>
        </Pressable>
      ))}
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={provider.selectedModel}
        onChangeText={(selectedModel) =>
          onChangeProvider({ ...provider, selectedModel })
        }
        autoCapitalize="none"
        placeholder="モデルID（手入力可）"
      />
      <View style={styles.actions}>
        <Pressable onPress={handleSave} style={styles.save} disabled={saving}>
          {saving ? (
            <ActivityIndicator color={COLORS.white} />
          ) : (
            <Text style={styles.saveText}>保存</Text>
          )}
        </Pressable>
        <Pressable onPress={onCancel} style={styles.cancel}>
          <Text style={{ color: colors.black }}>キャンセル</Text>
        </Pressable>
        {onDelete && (
          <Pressable onPress={onDelete} style={styles.remove}>
            <Text style={styles.removeText}>削除</Text>
          </Pressable>
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  editor: {
    padding: 12,
    borderWidth: 1,
    borderRadius: 12,
    gap: 10,
    marginTop: 8,
  },
  input: {
    minHeight: 44,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
  },
  secondary: {
    minHeight: 44,
    borderWidth: 1,
    borderColor: BRAND_COLOR,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  modelRow: { padding: 10, borderWidth: 1, borderRadius: 8 },
  actions: { flexDirection: 'row', gap: 8, marginTop: 4 },
  save: {
    flex: 1,
    minHeight: 44,
    borderRadius: 8,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
  },
  saveText: { color: COLORS.white, fontWeight: '700' },
  cancel: {
    minHeight: 44,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: '#999',
    paddingHorizontal: 16,
    alignItems: 'center',
    justifyContent: 'center',
  },
  remove: {
    minHeight: 44,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: '#B33A3A',
    paddingHorizontal: 16,
    alignItems: 'center',
    justifyContent: 'center',
  },
  removeText: { color: '#B33A3A' },
});
