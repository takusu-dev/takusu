import { useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useColors, BRAND_COLOR, COLORS } from '@/src/theme';
import {
  TTS_PROVIDER_LABELS,
  type TtsProvider,
  type TtsProviderSettings,
} from '@/src/api/settingsStore';
import { showError } from '@/src/api/errors';

const TTS_PROVIDER_TYPES: TtsProvider[] = ['cartesia', 'android'];

interface Props {
  provider: TtsProviderSettings;
  apiKey: string;
  onChangeProvider: (provider: TtsProviderSettings) => void;
  onChangeApiKey: (apiKey: string) => void;
  onSave: (provider: TtsProviderSettings) => void;
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
  const [isExpanded, setIsExpanded] = useState(false);
  const [sampleRate, setSampleRate] = useState(String(provider.sampleRate));
  const [speed, setSpeed] = useState(
    provider.speed !== undefined ? String(provider.speed) : '',
  );

  const isAndroid = provider.provider === 'android';

  function updateSampleRate(text: string) {
    setSampleRate(text);
    const parsed = parseInt(text, 10);
    if (Number.isFinite(parsed) && parsed > 0) {
      onChangeProvider({ ...provider, sampleRate: parsed });
    }
  }

  function updateSpeed(text: string) {
    setSpeed(text);
    if (text.trim() === '') {
      onChangeProvider({ ...provider, speed: undefined });
      return;
    }
    const parsed = parseFloat(text);
    if (Number.isFinite(parsed) && parsed > 0) {
      onChangeProvider({ ...provider, speed: parsed });
    }
  }

  function selectProvider(type: TtsProvider) {
    setIsExpanded(false);
    onChangeApiKey('');
    onChangeProvider({
      ...provider,
      provider: type,
      name: TTS_PROVIDER_LABELS[type],
      voiceId: '',
    });
  }

  function handleSave() {
    if (!isAndroid && !provider.voiceId.trim()) {
      void showError('Voice IDを入力してください', '入力不足');
      return;
    }

    let nextProvider = provider;

    if (!isAndroid) {
      const parsedSampleRate = parseInt(sampleRate, 10);
      if (
        sampleRate.trim() === '' ||
        !Number.isFinite(parsedSampleRate) ||
        parsedSampleRate <= 0
      ) {
        void showError(
          'サンプルレートは正の整数を入力してください',
          '入力不足',
        );
        return;
      }
      nextProvider = { ...nextProvider, sampleRate: parsedSampleRate };
    }

    if (speed.trim() !== '') {
      const parsedSpeed = parseFloat(speed);
      if (!Number.isFinite(parsedSpeed) || parsedSpeed <= 0) {
        void showError('速度は正の数値を入力してください', '入力不足');
        return;
      }
      nextProvider = { ...nextProvider, speed: parsedSpeed };
    } else {
      nextProvider = { ...nextProvider, speed: undefined };
    }

    onSave(nextProvider);
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

      <View>
        <Pressable
          onPress={() => setIsExpanded((prev) => !prev)}
          style={[styles.dropdown, { borderColor: colors.separator }]}
        >
          <Text style={{ color: colors.black }}>
            {TTS_PROVIDER_LABELS[provider.provider]}
          </Text>
          <Ionicons
            name={isExpanded ? 'chevron-up' : 'chevron-down'}
            size={16}
            color={colors.gray}
          />
        </Pressable>

        {isExpanded && (
          <View
            style={[
              styles.dropdownList,
              {
                backgroundColor: colors.surface,
                borderColor: colors.separator,
              },
            ]}
          >
            {TTS_PROVIDER_TYPES.map((type) => (
              <Pressable
                key={type}
                onPress={() => selectProvider(type)}
                style={[
                  styles.dropdownItem,
                  provider.provider === type && {
                    backgroundColor: colors.surfaceTint,
                  },
                ]}
              >
                <Ionicons
                  name={
                    provider.provider === type
                      ? 'checkmark-circle'
                      : 'ellipse-outline'
                  }
                  size={20}
                  color={
                    provider.provider === type ? BRAND_COLOR : colors.black
                  }
                />
                <Text style={{ color: colors.black }}>
                  {TTS_PROVIDER_LABELS[type]}
                </Text>
              </Pressable>
            ))}
          </View>
        )}
      </View>

      <TextInput
        style={[
          styles.input,
          { color: colors.black, borderColor: colors.separator },
        ]}
        value={provider.voiceId}
        onChangeText={(voiceId) => onChangeProvider({ ...provider, voiceId })}
        autoCapitalize="none"
        placeholder={isAndroid ? 'Voice（任意、空欄で既定の声）' : 'Voice ID'}
      />

      {!isAndroid && (
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
      )}

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
        {!isAndroid && (
          <TextInput
            style={[
              styles.input,
              styles.sampleRate,
              { color: colors.black, borderColor: colors.separator },
            ]}
            value={sampleRate}
            onChangeText={updateSampleRate}
            onBlur={() => {
              const parsed = parseInt(sampleRate, 10);
              if (
                sampleRate.trim() === '' ||
                !Number.isFinite(parsed) ||
                parsed <= 0
              ) {
                setSampleRate(String(provider.sampleRate));
              }
            }}
            keyboardType="numeric"
            placeholder="サンプルレート"
          />
        )}
        <TextInput
          style={[
            styles.input,
            styles.speed,
            { color: colors.black, borderColor: colors.separator },
          ]}
          value={speed}
          onChangeText={updateSpeed}
          onBlur={() => {
            if (speed.trim() === '') return;
            const parsed = parseFloat(speed);
            if (!Number.isFinite(parsed) || parsed <= 0) {
              setSpeed(
                provider.speed !== undefined ? String(provider.speed) : '',
              );
            }
          }}
          keyboardType="numeric"
          placeholder="速度"
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
  dropdownList: {
    marginTop: 4,
    borderWidth: 1,
    borderRadius: 8,
    overflow: 'hidden',
  },
  dropdownItem: {
    minHeight: 44,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingHorizontal: 12,
  },
  row: { flexDirection: 'row', gap: 8 },
  language: { flex: 1 },
  sampleRate: { flex: 1.5 },
  speed: { flex: 1 },
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
