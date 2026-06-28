// SettingsView — categorized settings
// general: dark/white theme, sync mode
// worker: endpoint, key
// google calendar: config
// info: license, version (build number)

import { useState } from 'react';
import {
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import { useRouter } from 'expo-router';
import { useServer } from '@/src/api/ServerProvider';
import { COLORS, BRAND_COLOR } from '@/src/theme';

type SettingsCategory = 'general' | 'worker' | 'google' | 'info';

export function SettingsView() {
  const router = useRouter();
  const { client } = useServer();
  const [category, setCategory] = useState<SettingsCategory>('general');
  const [darkMode, setDarkMode] = useState(false);
  const [syncMode, setSyncMode] = useState<'simultaneous' | 'two-step'>(
    'simultaneous',
  );
  const [workerUrl, setWorkerUrl] = useState('');
  const [workerKey, setWorkerKey] = useState('');

  const categories: { key: SettingsCategory; label: string }[] = [
    { key: 'general', label: '一般' },
    { key: 'worker', label: 'Worker' },
    { key: 'google', label: 'Google Calendar' },
    { key: 'info', label: '情報' },
  ];

  return (
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={() => router.back()}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <Text style={styles.title}>設定</Text>
      </View>

      <View style={styles.body}>
        {/* Category tabs */}
        <View style={styles.tabs}>
          {categories.map((c) => (
            <Pressable
              key={c.key}
              style={[styles.tab, category === c.key && styles.tabActive]}
              onPress={() => setCategory(c.key)}
            >
              <Text
                style={[
                  styles.tabText,
                  category === c.key && styles.tabTextActive,
                ]}
              >
                {c.label}
              </Text>
            </Pressable>
          ))}
        </View>

        <ScrollView contentContainerStyle={styles.content}>
          {category === 'general' && (
            <>
              <View style={styles.settingRow}>
                <Text style={styles.settingLabel}>ダークモード</Text>
                <Switch
                  value={darkMode}
                  onValueChange={setDarkMode}
                  trackColor={{ true: BRAND_COLOR }}
                />
              </View>
              <View style={styles.settingRow}>
                <Text style={styles.settingLabel}>同期モード</Text>
                <View style={styles.segmentedControl}>
                  <Pressable
                    style={[
                      styles.segment,
                      syncMode === 'simultaneous' && styles.segmentActive,
                    ]}
                    onPress={() => setSyncMode('simultaneous')}
                  >
                    <Text
                      style={[
                        styles.segmentText,
                        syncMode === 'simultaneous' && styles.segmentTextActive,
                      ]}
                    >
                      同時
                    </Text>
                  </Pressable>
                  <Pressable
                    style={[
                      styles.segment,
                      syncMode === 'two-step' && styles.segmentActive,
                    ]}
                    onPress={() => setSyncMode('two-step')}
                  >
                    <Text
                      style={[
                        styles.segmentText,
                        syncMode === 'two-step' && styles.segmentTextActive,
                      ]}
                    >
                      2段階
                    </Text>
                  </Pressable>
                </View>
              </View>
            </>
          )}

          {category === 'worker' && (
            <>
              <View style={styles.field}>
                <Text style={styles.label}>エンドポイント</Text>
                <TextInput
                  style={styles.input}
                  value={workerUrl}
                  onChangeText={setWorkerUrl}
                  placeholder="https://your-worker.workers.dev"
                />
              </View>
              <View style={styles.field}>
                <Text style={styles.label}>キー</Text>
                <TextInput
                  style={styles.input}
                  value={workerKey}
                  onChangeText={setWorkerKey}
                  placeholder="tsk_..."
                  secureTextEntry
                />
              </View>
            </>
          )}

          {category === 'google' && (
            <View style={styles.field}>
              <Text style={styles.label}>Google Calendar</Text>
              <Text style={styles.value}>
                Google Calendar連携の設定はここから行います。
              </Text>
              <Pressable style={styles.actionButton}>
                <Text style={styles.actionButtonText}>OAuth認証を開始</Text>
              </Pressable>
            </View>
          )}

          {category === 'info' && (
            <>
              <View style={styles.field}>
                <Text style={styles.label}>バージョン</Text>
                <Text style={styles.value}>0.1.0 (build 1)</Text>
              </View>
              <View style={styles.field}>
                <Text style={styles.label}>ライセンス</Text>
                <Text style={styles.value}>MIT</Text>
              </View>
            </>
          )}
        </ScrollView>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: COLORS.white,
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingTop: 48,
    paddingBottom: 8,
  },
  backButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  backButtonText: {
    fontSize: 28,
    color: BRAND_COLOR,
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
    color: COLORS.black,
    marginLeft: 8,
  },
  body: {
    flex: 1,
  },
  tabs: {
    flexDirection: 'row',
    paddingHorizontal: 8,
    gap: 4,
    borderBottomWidth: 1,
    borderBottomColor: COLORS.separator,
  },
  tab: {
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  tabActive: {
    borderBottomWidth: 2,
    borderBottomColor: BRAND_COLOR,
  },
  tabText: {
    fontSize: 14,
    color: COLORS.gray,
  },
  tabTextActive: {
    color: BRAND_COLOR,
    fontWeight: '600',
  },
  content: {
    padding: 16,
    gap: 16,
  },
  settingRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 8,
  },
  settingLabel: {
    fontSize: 16,
    color: COLORS.black,
  },
  segmentedControl: {
    flexDirection: 'row',
    borderRadius: 8,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: COLORS.separator,
  },
  segment: {
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  segmentActive: {
    backgroundColor: BRAND_COLOR,
  },
  segmentText: {
    fontSize: 13,
    color: COLORS.gray,
  },
  segmentTextActive: {
    color: COLORS.white,
  },
  field: {
    gap: 4,
  },
  label: {
    fontSize: 13,
    color: COLORS.gray,
    fontWeight: '500',
  },
  value: {
    fontSize: 16,
    color: COLORS.black,
  },
  input: {
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    fontSize: 16,
  },
  actionButton: {
    marginTop: 12,
    paddingHorizontal: 16,
    paddingVertical: 10,
    backgroundColor: BRAND_COLOR,
    borderRadius: 8,
    alignItems: 'center',
  },
  actionButtonText: {
    color: COLORS.white,
    fontSize: 14,
    fontWeight: '600',
  },
});
