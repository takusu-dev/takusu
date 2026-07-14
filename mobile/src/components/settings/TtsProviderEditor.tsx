import { useState } from 'react';
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
import { type TtsProviderSettings } from '@/src/api/settingsStore';

const TTS_PROVIDER_TYPES: TtsProviderSettings['provider'][] = ['cartesia'];

const TTS_PROVIDER_DEFAULTS: Record<TtsProviderSettings['provider'], string> = {
  cartesia: 'Cartesia',
};

interface Props {
  provider: TtsProviderSettings;
  apiKey: string;
  onChangeProvider: (provider: TtsProviderSettings) => void;
  onChangeApiKey: (apiKey: string) => void;
  onSave: () => void;
  onCancel: () => void;
  onDelete?: () => void;
  saving?: boolean;
}

export function TtsProviderEditor({
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
  const [sampleRate, setSampleRate] = useState(String(provider.sampleRate));

  function updateSampleRate(text: string) {
    setSampleRate(text);
    const parsed = parseInt(text, 10);
    if (Number.isFinite(parsed) && parsed > 0) {
      onChangeProvider({ ...provider, sampleRate: parsed });
    }
  }

  function handleSave() {
    if (!provider.voiceId.trim()) {
      Alert.alert('入力不足', 'Voice IDを入力してください');
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
      <Pressable
        onPress={() => {
          Alert.alert('TTS Provider', '', [
            { text: 'キャンセル', style: 'cancel' },
            ...TTS_PROVIDER_TYPES.map((type) => ({
              text: type,
              onPress: () =>
                onChangeProvider({
                  ...provider,
                  provider: type,
                  name: TTS_PROVIDER_DEFAULTS[type],
                }),
            })),
          ]);
        }}
        style={[styles.dropdown, { borderColor: colors.separator }]}
      >
        <Text style={{ color: colors.black }}>{provider.provider}</Text>
        <Text style={{ color: colors.gray }}>▼</Text>
      </Pressable>
      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={provider.voiceId}
        onChangeText={(voiceId) => onChangeProvider({ ...provider, voiceId })}
        autoCapitalize="none"
        placeholder="Voice ID"
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
        placeholder="Cartesia API key"
      />
      <View style={styles.row}>
        <TextInput
          style={[
            styles.input,
            styles.language,
            { color: colors.black, borderColor: colors.separator },
          ]}
          value={provider.language}
          onChangeText={(language) =>
            onChangeProvider({ ...provider, language })
          }
          autoCapitalize="none"
          placeholder="言語"
        />
        <TextInput
          style={[
            styles.input,
            styles.sampleRate,
            { color: colors.black, borderColor: colors.separator },
          ]}
          value={sampleRate}
          onChangeText={updateSampleRate}
          keyboardType="numeric"
          placeholder="サンプルレート"
        />
      </View>
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
  dropdown: {
    minHeight: 44,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
  },
  row: { flexDirection: 'row', gap: 8 },
  language: { flex: 1 },
  sampleRate: { flex: 1.5 },
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
