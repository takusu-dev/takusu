// SettingsView — categorized settings
// general: dark/white theme
// worker: endpoint, key (with server restart)
// google calendar: config + OAuth + manual sync
// info: license, version (build number)
//
// Split into two screens (issue #127): a category list and a per-category
// detail screen. The horizontal tab bar overflowed on small screens, making
// some categories (notably "情報") unreachable.

import { useCallback, useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useRouter } from 'expo-router';
import * as Application from 'expo-application';
import * as FileSystem from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import * as Clipboard from 'expo-clipboard';
import Constants from 'expo-constants';
import {
  useServer,
  saveWorkersUrl,
  saveWorkersToken,
} from '@/src/api/ServerProvider';
import type { GoogleCalSettings, SettingsRow } from '@/src/api/types';
import { useColors, BRAND_COLOR } from '@/src/theme';
import {
  formatTime,
  minutesToTime,
  timeToMinutes,
} from '@/src/notifications/settings';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import { haptic } from '@/src/components/haptics';
import TakusuServerModule from '../../modules/takusu-server/src/TakusuServerModule';
import { AgentSettingsView } from '@/src/views/AgentSettingsView';
import { SkillsSettingsView } from '@/src/views/SkillsSettingsView';

export type SettingsCategory =
  | 'general'
  | 'sleep'
  | 'workload'
  | 'notifications'
  | 'agent'
  | 'skills'
  | 'worker'
  | 'google'
  | 'info';

const CATEGORY_LABELS: Record<SettingsCategory, string> = {
  general: '一般',
  sleep: '睡眠',
  workload: '作業負荷',
  notifications: '通知',
  agent: 'Agent',
  skills: 'スキル',
  worker: 'Worker',
  google: 'Google Calendar',
  info: '情報',
};

const CATEGORY_ORDER: SettingsCategory[] = [
  'general',
  'sleep',
  'workload',
  'notifications',
  'agent',
  'skills',
  'worker',
  'google',
  'info',
];

// Convert stored minutes back to hours for the workload inputs.
// `0` or `null` means "use the default", so the input is left empty.
function formatMinutesToHours(minutes: number | null | undefined): string {
  if (!minutes || minutes <= 0) return '';
  return String(parseFloat((minutes / 60).toFixed(2)));
}

// Parse an hours string into minutes. Empty string or "0" resolves to 0
// (the default sentinel). Returns null for invalid/negative input.
function parseHoursToMinutes(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === '') return 0;
  const n = parseFloat(trimmed);
  if (!isFinite(n) || n < 0) return null;
  return Math.round(n * 60);
}

// ── Category list screen ──
// Replaces the horizontal tab bar that overflowed on small screens (issue #127).
export function SettingsCategoryView() {
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View
        style={[
          styles.topBar,
          { borderBottomColor: colors.separator, paddingTop: 8 + insets.top },
        ]}
      >
        <Pressable
          style={styles.backButton}
          onPress={() => {
            haptic.light();
            router.back();
          }}
        >
          <Text style={[styles.backButtonText, { color: BRAND_COLOR }]}>‹</Text>
        </Pressable>
        <Text style={[styles.title, { color: colors.black }]}>設定</Text>
      </View>

      <ScrollView
        contentContainerStyle={[
          styles.content,
          { paddingBottom: 16 + insets.bottom },
        ]}
      >
        {CATEGORY_ORDER.map((key) => (
          <Pressable
            key={key}
            style={[
              styles.categoryRow,
              { borderBottomColor: colors.separator },
            ]}
            onPress={() => {
              haptic.select();
              router.push(`/settings/${key}`);
            }}
          >
            <Text style={[styles.categoryLabel, { color: colors.black }]}>
              {CATEGORY_LABELS[key]}
            </Text>
            <Ionicons name="chevron-forward" size={20} color={colors.gray} />
          </Pressable>
        ))}
      </ScrollView>
    </View>
  );
}

// ── Per-category detail screen ──
export function SettingsDetailView({
  category,
}: {
  category: SettingsCategory;
}) {
  const router = useRouter();
  const {
    client,
    darkMode,
    setDarkMode,
    undoSteps,
    setUndoSteps,
    workersUrl: savedUrl,
    workersToken: savedToken,
    restartServer,
    restarting,
    notifications,
    setNotifications,
  } = useServer();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const [notifPickerField, setNotifPickerField] = useState<
    'morningBriefing' | null
  >(null);

  // Notification numeric inputs — local text state, committed on blur so
  // the user can clear the field completely while typing (#307).
  const [preStartInput, setPreStartInput] = useState(
    String(notifications.preStartReminderMinutes),
  );
  const [idleHoursInput, setIdleHoursInput] = useState(
    String(notifications.unscheduledIdleHours),
  );

  // Sleep tab state
  const [sleepSettings, setSleepSettings] = useState<SettingsRow | null>(null);
  const [sleepTz, setSleepTz] = useState('');
  const [sleepStart, setSleepStart] = useState('22:00');
  const [sleepEnd, setSleepEnd] = useState('06:00');
  const [sleepLoading, setSleepLoading] = useState(false);
  const [sleepSaving, setSleepSaving] = useState(false);
  const [sleepPickerField, setSleepPickerField] = useState<
    'start' | 'end' | null
  >(null);

  // Workload tab state (#459)
  const [workloadSettings, setWorkloadSettings] = useState<SettingsRow | null>(
    null,
  );
  const [workloadComfortable, setWorkloadComfortable] = useState('');
  const [workloadMaximum, setWorkloadMaximum] = useState('');
  const [workloadLoading, setWorkloadLoading] = useState(false);
  const [workloadSaving, setWorkloadSaving] = useState(false);
  const DEFAULT_COMFORTABLE_HOURS = 8;
  const DEFAULT_MAXIMUM_HOURS = 12;

  // Worker tab state
  const [workerUrl, setWorkerUrl] = useState(savedUrl);
  const [workerKey, setWorkerKey] = useState(savedToken);
  const [workerDirty, setWorkerDirty] = useState(false);

  // Undo steps input — local text state, committed on blur to avoid
  // trimming the undo stack through intermediate values while typing.
  const [undoStepsInput, setUndoStepsInput] = useState(String(undoSteps));

  // Google Calendar state
  const [gcalSettings, setGcalSettings] = useState<GoogleCalSettings | null>(
    null,
  );
  const [gcalEnabled, setGcalEnabled] = useState(false);
  const [gcalCalendarId, setGcalCalendarId] = useState('');
  const [gcalClientId, setGcalClientId] = useState('');
  const [gcalClientSecret, setGcalClientSecret] = useState('');
  const [gcalRefreshToken, setGcalRefreshToken] = useState('');
  const [gcalLoading, setGcalLoading] = useState(false);
  const [syncLoading, setSyncLoading] = useState(false);

  // Health check state (info tab)
  const [localHealthLoading, setLocalHealthLoading] = useState(false);
  const [localHealthResult, setLocalHealthResult] = useState<string | null>(
    null,
  );
  const [workerHealthLoading, setWorkerHealthLoading] = useState(false);
  const [workerHealthResult, setWorkerHealthResult] = useState<string | null>(
    null,
  );
  const [logExportLoading, setLogExportLoading] = useState(false);
  const [logCopyLoading, setLogCopyLoading] = useState(false);

  // Sync worker input with saved values when they change
  useEffect(() => {
    setWorkerUrl(savedUrl);
    setWorkerKey(savedToken);
    setWorkerDirty(false);
  }, [savedUrl, savedToken]);

  // Keep local undo-steps input in sync with the persisted value
  useEffect(() => {
    setUndoStepsInput(String(undoSteps));
  }, [undoSteps]);

  function commitUndoSteps() {
    const n = parseInt(undoStepsInput, 10);
    if (!isNaN(n) && n > 0) {
      setUndoSteps(n);
    } else {
      // Revert to the current persisted value on invalid input
      setUndoStepsInput(String(undoSteps));
    }
  }

  // Keep notification inputs in sync when the persisted value changes
  useEffect(() => {
    setPreStartInput(String(notifications.preStartReminderMinutes));
  }, [notifications.preStartReminderMinutes]);
  useEffect(() => {
    setIdleHoursInput(String(notifications.unscheduledIdleHours));
  }, [notifications.unscheduledIdleHours]);

  function commitPreStart() {
    const n = parseInt(preStartInput, 10);
    if (!isNaN(n) && n > 0) {
      setNotifications({ ...notifications, preStartReminderMinutes: n });
    } else {
      setPreStartInput(String(notifications.preStartReminderMinutes));
    }
  }

  function commitIdleHours() {
    const n = parseInt(idleHoursInput, 10);
    if (!isNaN(n) && n > 0) {
      setNotifications({ ...notifications, unscheduledIdleHours: n });
    } else {
      setIdleHoursInput(String(notifications.unscheduledIdleHours));
    }
  }

  // Load Google Calendar settings when entering google tab
  const loadGcalSettings = useCallback(async () => {
    if (!client) return;
    setGcalLoading(true);
    try {
      const s = await client.getGcalSettings();
      setGcalSettings(s);
      setGcalEnabled(s.enabled);
      setGcalCalendarId(s.calendar_id);
      setGcalClientId(s.client_id);
      setGcalClientSecret('');
    } catch {
      // settings may not exist yet
      setGcalSettings(null);
    } finally {
      setGcalLoading(false);
    }
  }, [client]);

  useEffect(() => {
    if (category === 'google') {
      loadGcalSettings();
    }
  }, [category, loadGcalSettings]);

  // Load sleep/planner settings when entering sleep tab
  const loadSleepSettings = useCallback(async () => {
    if (!client) return;
    setSleepLoading(true);
    try {
      const s = await client.getSettings();
      setSleepSettings(s);
      setSleepTz(s.tz);
      setSleepStart(s.sleep_start);
      setSleepEnd(s.sleep_end);
    } catch {
      // fall back to defaults so the user can still set something
      setSleepSettings(null);
    } finally {
      setSleepLoading(false);
    }
  }, [client]);

  useEffect(() => {
    if (category === 'sleep' || category === 'general') {
      loadSleepSettings();
    }
  }, [category, loadSleepSettings]);

  // Load workload settings when entering workload tab
  const loadWorkloadSettings = useCallback(async () => {
    if (!client) return;
    setWorkloadLoading(true);
    try {
      const s = await client.getSettings();
      setWorkloadSettings(s);
      setWorkloadComfortable(formatMinutesToHours(s.comfortable_minutes));
      setWorkloadMaximum(formatMinutesToHours(s.maximum_minutes));
    } catch {
      setWorkloadSettings(null);
    } finally {
      setWorkloadLoading(false);
    }
  }, [client]);

  useEffect(() => {
    if (category === 'workload') {
      loadWorkloadSettings();
    }
  }, [category, loadWorkloadSettings]);
  async function saveWorkerSettings() {
    await saveWorkersUrl(workerUrl);
    await saveWorkersToken(workerKey);
    setWorkerDirty(false);
  }

  async function handleRestartServer() {
    await saveWorkerSettings();
    await restartServer(workerUrl, workerKey);
  }

  async function saveSleepSettings() {
    if (!client) return;
    // Guard against overwriting server values with defaults when the initial
    // load failed (sleepSettings stays null and the form shows defaults).
    if (!sleepSettings) {
      Alert.alert(
        'エラー',
        '設定の読み込みに失敗しています。タブを開き直してください',
      );
      return;
    }
    setSleepSaving(true);
    try {
      const s = await client.updateSettings({
        sleep_start: sleepStart,
        sleep_end: sleepEnd,
      });
      setSleepSettings(s);
      setSleepStart(s.sleep_start);
      setSleepEnd(s.sleep_end);
      haptic.success();
      Alert.alert('保存しました', '睡眠設定を保存しました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `保存に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setSleepSaving(false);
    }
  }

  async function saveTimezoneSettings() {
    if (!client) return;
    if (!sleepSettings) {
      Alert.alert(
        'エラー',
        '設定の読み込みに失敗しています。タブを開き直してください',
      );
      return;
    }
    setSleepSaving(true);
    try {
      const s = await client.updateSettings({
        tz: sleepTz || undefined,
      });
      setSleepSettings(s);
      setSleepTz(s.tz);
      haptic.success();
      Alert.alert('保存しました', 'タイムゾーンを保存しました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `保存に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setSleepSaving(false);
    }
  }

  async function saveWorkloadSettings() {
    if (!client) return;
    if (!workloadSettings) {
      Alert.alert(
        'エラー',
        '設定の読み込みに失敗しています。タブを開き直してください',
      );
      return;
    }
    const comfortable = parseHoursToMinutes(workloadComfortable);
    const maximum = parseHoursToMinutes(workloadMaximum);
    if (comfortable === null || maximum === null) {
      Alert.alert('エラー', '作業時間は0以上の数値を入力してください');
      return;
    }
    setWorkloadSaving(true);
    try {
      const s = await client.updateSettings({
        comfortable_minutes: comfortable,
        maximum_minutes: maximum,
      });
      setWorkloadSettings(s);
      setWorkloadComfortable(formatMinutesToHours(s.comfortable_minutes));
      setWorkloadMaximum(formatMinutesToHours(s.maximum_minutes));
      haptic.success();
      Alert.alert('保存しました', '作業負荷設定を保存しました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `保存に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setWorkloadSaving(false);
    }
  }

  async function saveGcalSettings() {
    if (!client) return;
    try {
      const s = await client.updateGcalSettings({
        enabled: gcalEnabled,
        calendar_id: gcalCalendarId || undefined,
        client_id: gcalClientId || undefined,
        client_secret: gcalClientSecret || undefined,
      });
      setGcalSettings(s);
      setGcalClientSecret('');
      Alert.alert('保存しました', 'Google Calendar設定を保存しました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `保存に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }

  // Save a refresh token obtained via the CLI OAuth flow (issue #297).
  //
  // Mobile no longer runs OAuth directly — the Android Credential
  // Manager / One Tap flow was fragile across devices (issues #108,
  // #129, #248, #297).  Instead, the user runs OAuth on the CLI:
  //
  //   takusu sync login --client-id <ID> --client-secret <SECRET>
  //   → starts a local callback server on 127.0.0.1 and opens the browser
  //   → receives the authorization code and exchanges it for a refresh token
  //
  // The CLI exchanges the code with Google and stores the refresh
  // token in the shared backend (local SQLite or Workers D1).  Mobile
  // reads it from there.  This field lets the user paste a token
  // obtained by other means as a fallback.
  async function saveRefreshToken() {
    if (!client) return;
    if (!gcalRefreshToken.trim()) {
      Alert.alert('エラー', 'Refresh Tokenを入力してください');
      return;
    }
    try {
      const s = await client.updateGcalSettings({
        refresh_token: gcalRefreshToken.trim(),
      });
      setGcalSettings(s);
      setGcalRefreshToken('');
      Alert.alert('保存しました', 'Refresh Tokenを保存しました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `保存に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }

  async function triggerSync() {
    if (!client) return;
    setSyncLoading(true);
    try {
      await client.triggerSync();
      Alert.alert('同期完了', 'Google Calendarへ同期しました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `同期に失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setSyncLoading(false);
    }
  }

  // ── Health checks (info tab) ──

  async function checkLocalHealth() {
    if (!client) return;
    setLocalHealthLoading(true);
    setLocalHealthResult(null);
    try {
      const text = await client.health();
      setLocalHealthResult(`✓ ${text}`);
    } catch (e) {
      setLocalHealthResult(`✗ ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLocalHealthLoading(false);
    }
  }

  async function checkWorkerHealth() {
    if (!client) return;
    setWorkerHealthLoading(true);
    setWorkerHealthResult(null);
    try {
      const { status } = await client.workerHealthCheck();
      setWorkerHealthResult(`✓ ${status}`);
    } catch (e) {
      setWorkerHealthResult(`✗ ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setWorkerHealthLoading(false);
    }
  }

  // ── Log export (info tab) ──

  async function exportLogs() {
    setLogExportLoading(true);
    try {
      const lines = await TakusuServerModule.getLogs();
      if (lines.length === 0) {
        Alert.alert('ログなし', 'エクスポートするログがありません');
        return;
      }
      const content = lines.join('\n') + '\n';
      const filename = `takusu-logs-${new Date().toISOString().replace(/[:.]/g, '-')}.txt`;
      const file = new FileSystem.File(FileSystem.Paths.cache, filename);
      // write() does not auto-create the file, so create it first if missing.
      if (!file.exists) {
        file.create();
      }
      file.write(content);
      if (await Sharing.isAvailableAsync()) {
        await Sharing.shareAsync(file.uri, {
          mimeType: 'text/plain',
          dialogTitle: 'ログをエクスポート',
        });
      } else {
        Alert.alert('エクスポート完了', `ログを保存しました:\n${file.uri}`);
      }
    } catch (e) {
      Alert.alert(
        'エラー',
        `ログエクスポートに失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setLogExportLoading(false);
    }
  }

  async function clearLogs() {
    try {
      await TakusuServerModule.clearLogs();
      Alert.alert('消去しました', 'ログバッファをクリアしました');
    } catch (e) {
      Alert.alert(
        'エラー',
        `ログクリアに失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  }

  async function copyLogs() {
    setLogCopyLoading(true);
    try {
      const lines = await TakusuServerModule.getLogs();
      if (lines.length === 0) {
        Alert.alert('ログなし', 'コピーするログがありません');
        return;
      }
      const content = lines.join('\n');
      await Clipboard.setStringAsync(content);
      haptic.success();
      Alert.alert(
        'コピーしました',
        `${lines.length} 行のログをクリップボードにコピーしました`,
      );
    } catch (e) {
      Alert.alert(
        'エラー',
        `ログコピーに失敗: ${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setLogCopyLoading(false);
    }
  }

  const appVersion = Application.nativeApplicationVersion ?? 'unknown';
  const buildVersion = Application.nativeBuildVersion ?? 'unknown';
  const gitCommit = Constants.expoConfig?.extra?.gitCommit ?? 'unknown';
  const gitTag = Constants.expoConfig?.extra?.gitTag ?? 'unknown';

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View
        style={[
          styles.topBar,
          { borderBottomColor: colors.separator, paddingTop: 8 + insets.top },
        ]}
      >
        <Pressable
          style={styles.backButton}
          onPress={() => {
            haptic.light();
            router.back();
          }}
        >
          <Text style={[styles.backButtonText, { color: BRAND_COLOR }]}>‹</Text>
        </Pressable>
        <Text style={[styles.title, { color: colors.black }]}>
          {CATEGORY_LABELS[category]}
        </Text>
      </View>

      <View style={styles.body}>
        <ScrollView
          contentContainerStyle={[
            styles.content,
            { paddingBottom: 16 + insets.bottom },
          ]}
        >
          {category === 'agent' && <AgentSettingsView />}
          {category === 'skills' && <SkillsSettingsView />}
          {category === 'general' && (
            <>
              <View style={styles.settingRow}>
                <Text style={[styles.settingLabel, { color: colors.black }]}>
                  ダークモード
                </Text>
                <Switch
                  value={darkMode}
                  onValueChange={(v) => {
                    haptic.select();
                    setDarkMode(v);
                  }}
                  trackColor={{ true: BRAND_COLOR }}
                />
              </View>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  アンドゥ履歴の上限 (ステップ数)
                </Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={undoStepsInput}
                  onChangeText={setUndoStepsInput}
                  onBlur={commitUndoSteps}
                  onSubmitEditing={commitUndoSteps}
                  keyboardType="numeric"
                  placeholder="50"
                  placeholderTextColor={colors.gray}
                />
              </View>

              {sleepLoading ? (
                <ActivityIndicator color={BRAND_COLOR} style={styles.loader} />
              ) : (
                <>
                  {sleepSettings && (
                    <View
                      style={[
                        styles.statusBox,
                        { backgroundColor: colors.grayLight + '20' },
                      ]}
                    >
                      <Text style={[styles.label, { color: colors.gray }]}>
                        現在のタイムゾーン
                      </Text>
                      <Text style={[styles.value, { color: colors.black }]}>
                        {sleepSettings.tz}
                      </Text>
                    </View>
                  )}

                  <View style={styles.field}>
                    <Text style={[styles.label, { color: colors.gray }]}>
                      タイムゾーン
                    </Text>
                    <TextInput
                      style={[
                        styles.input,
                        { borderColor: colors.separator, color: colors.black },
                      ]}
                      value={sleepTz}
                      onChangeText={setSleepTz}
                      placeholder="Asia/Tokyo"
                      placeholderTextColor={colors.gray}
                      autoCapitalize="none"
                      autoCorrect={false}
                    />
                    <Pressable
                      style={[
                        styles.actionButton,
                        { backgroundColor: colors.grayLight },
                      ]}
                      onPress={() => {
                        haptic.light();
                        // Intl is available in Hermes (recent RN) — no native module needed
                        try {
                          const tz =
                            Intl.DateTimeFormat().resolvedOptions().timeZone;
                          if (tz) setSleepTz(tz);
                        } catch {
                          // ignore — device doesn't expose timezone via Intl
                        }
                      }}
                    >
                      <Text
                        style={[
                          styles.actionButtonText,
                          { color: colors.black },
                        ]}
                      >
                        デバイスのタイムゾーンを使用
                      </Text>
                    </Pressable>
                  </View>

                  <Pressable
                    style={[
                      styles.actionButton,
                      { backgroundColor: BRAND_COLOR },
                    ]}
                    onPress={() => {
                      haptic.medium();
                      saveTimezoneSettings();
                    }}
                    disabled={sleepSaving || !client}
                  >
                    {sleepSaving ? (
                      <ActivityIndicator color="#FFFFFF" />
                    ) : (
                      <Text style={styles.actionButtonText}>設定を保存</Text>
                    )}
                  </Pressable>
                </>
              )}
            </>
          )}

          {category === 'sleep' && (
            <>
              {sleepLoading ? (
                <ActivityIndicator color={BRAND_COLOR} style={styles.loader} />
              ) : (
                <>
                  {sleepSettings && (
                    <View
                      style={[
                        styles.statusBox,
                        { backgroundColor: colors.grayLight + '20' },
                      ]}
                    >
                      <Text style={[styles.label, { color: colors.gray }]}>
                        現在の設定
                      </Text>
                      <Text style={[styles.value, { color: colors.black }]}>
                        就寝: {sleepSettings.sleep_start}
                        {'\n'}
                        起床: {sleepSettings.sleep_end}
                      </Text>
                    </View>
                  )}

                  <View style={styles.notifGroup}>
                    <Text style={[styles.label, { color: colors.gray }]}>
                      就寝時刻
                    </Text>
                    <Pressable
                      style={[
                        styles.timeField,
                        { borderColor: colors.separator },
                      ]}
                      onPress={() => {
                        haptic.select();
                        setSleepPickerField('start');
                      }}
                    >
                      <Text style={[styles.timeText, { color: colors.black }]}>
                        {sleepStart}
                      </Text>
                    </Pressable>
                  </View>

                  <View style={styles.notifGroup}>
                    <Text style={[styles.label, { color: colors.gray }]}>
                      起床時刻
                    </Text>
                    <Pressable
                      style={[
                        styles.timeField,
                        { borderColor: colors.separator },
                      ]}
                      onPress={() => {
                        haptic.select();
                        setSleepPickerField('end');
                      }}
                    >
                      <Text style={[styles.timeText, { color: colors.black }]}>
                        {sleepEnd}
                      </Text>
                    </Pressable>
                  </View>

                  <Pressable
                    style={[
                      styles.actionButton,
                      { backgroundColor: BRAND_COLOR },
                    ]}
                    onPress={() => {
                      haptic.medium();
                      saveSleepSettings();
                    }}
                    disabled={sleepSaving || !client}
                  >
                    {sleepSaving ? (
                      <ActivityIndicator color="#FFFFFF" />
                    ) : (
                      <Text style={styles.actionButtonText}>設定を保存</Text>
                    )}
                  </Pressable>
                </>
              )}
            </>
          )}

          {category === 'workload' && (
            <>
              {workloadLoading ? (
                <ActivityIndicator color={BRAND_COLOR} style={styles.loader} />
              ) : (
                <>
                  <View
                    style={[
                      styles.statusBox,
                      { backgroundColor: colors.grayLight + '20' },
                    ]}
                  >
                    <Text style={[styles.label, { color: colors.gray }]}>
                      デフォルト
                    </Text>
                    <Text style={[styles.value, { color: colors.black }]}>
                      快適: {DEFAULT_COMFORTABLE_HOURS}時間 / 最大:{' '}
                      {DEFAULT_MAXIMUM_HOURS}時間
                    </Text>
                  </View>

                  <View style={styles.field}>
                    <Text style={[styles.label, { color: colors.gray }]}>
                      快適な1日の作業時間（時間）
                    </Text>
                    <TextInput
                      style={[
                        styles.input,
                        { borderColor: colors.separator, color: colors.black },
                      ]}
                      value={workloadComfortable}
                      onChangeText={setWorkloadComfortable}
                      keyboardType="numeric"
                      placeholder={String(DEFAULT_COMFORTABLE_HOURS)}
                      placeholderTextColor={colors.gray}
                    />
                  </View>

                  <View style={styles.field}>
                    <Text style={[styles.label, { color: colors.gray }]}>
                      最大の1日の作業時間（時間）
                    </Text>
                    <TextInput
                      style={[
                        styles.input,
                        { borderColor: colors.separator, color: colors.black },
                      ]}
                      value={workloadMaximum}
                      onChangeText={setWorkloadMaximum}
                      keyboardType="numeric"
                      placeholder={String(DEFAULT_MAXIMUM_HOURS)}
                      placeholderTextColor={colors.gray}
                    />
                  </View>

                  <Pressable
                    style={[
                      styles.actionButton,
                      { backgroundColor: BRAND_COLOR },
                    ]}
                    onPress={() => {
                      haptic.medium();
                      saveWorkloadSettings();
                    }}
                    disabled={workloadSaving || !client}
                  >
                    {workloadSaving ? (
                      <ActivityIndicator color="#FFFFFF" />
                    ) : (
                      <Text style={styles.actionButtonText}>設定を保存</Text>
                    )}
                  </Pressable>
                </>
              )}
            </>
          )}

          {category === 'notifications' && (
            <>
              {/* Master toggle */}
              <View style={styles.settingRow}>
                <Text style={[styles.settingLabel, { color: colors.black }]}>
                  通知を有効化
                </Text>
                <Switch
                  value={notifications.enabled}
                  onValueChange={(v) => {
                    haptic.select();
                    setNotifications({ ...notifications, enabled: v });
                  }}
                  trackColor={{ true: BRAND_COLOR }}
                />
              </View>

              {notifications.enabled && (
                <>
                  {/* Morning briefing */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text
                        style={[styles.settingLabel, { color: colors.black }]}
                      >
                        朝のブリーフィング
                      </Text>
                      <Switch
                        value={notifications.morningBriefing}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({
                            ...notifications,
                            morningBriefing: v,
                          });
                        }}
                        trackColor={{ true: BRAND_COLOR }}
                      />
                    </View>
                    {notifications.morningBriefing && (
                      <Pressable
                        style={[
                          styles.timeField,
                          { borderColor: colors.separator },
                        ]}
                        onPress={() => {
                          haptic.select();
                          setNotifPickerField('morningBriefing');
                        }}
                      >
                        <Text
                          style={[styles.timeText, { color: colors.black }]}
                        >
                          {formatTime(notifications.morningBriefingTime)}
                        </Text>
                      </Pressable>
                    )}
                  </View>

                  {/* Pre-start reminder */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text
                        style={[styles.settingLabel, { color: colors.black }]}
                      >
                        開始直前リマインダー
                      </Text>
                      <Switch
                        value={notifications.preStartReminder}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({
                            ...notifications,
                            preStartReminder: v,
                          });
                        }}
                        trackColor={{ true: BRAND_COLOR }}
                      />
                    </View>
                    {notifications.preStartReminder && (
                      <View style={styles.field}>
                        <Text style={[styles.label, { color: colors.gray }]}>
                          何分前から通知するか
                        </Text>
                        <TextInput
                          style={[
                            styles.input,
                            {
                              borderColor: colors.separator,
                              color: colors.black,
                            },
                          ]}
                          value={preStartInput}
                          onChangeText={setPreStartInput}
                          onBlur={commitPreStart}
                          onSubmitEditing={commitPreStart}
                          keyboardType="numeric"
                          placeholder="10"
                          placeholderTextColor={colors.gray}
                        />
                      </View>
                    )}
                  </View>

                  {/* Start overdue */}
                  <View style={styles.settingRow}>
                    <Text
                      style={[styles.settingLabel, { color: colors.black }]}
                    >
                      開始時間到着通知
                    </Text>
                    <Switch
                      value={notifications.startOverdue}
                      onValueChange={(v) => {
                        haptic.select();
                        setNotifications({ ...notifications, startOverdue: v });
                      }}
                      trackColor={{ true: BRAND_COLOR }}
                    />
                  </View>

                  {/* End time */}
                  <View style={styles.settingRow}>
                    <Text
                      style={[styles.settingLabel, { color: colors.black }]}
                    >
                      タスク終了時間通知
                    </Text>
                    <Switch
                      value={notifications.endTime}
                      onValueChange={(v) => {
                        haptic.select();
                        setNotifications({ ...notifications, endTime: v });
                      }}
                      trackColor={{ true: BRAND_COLOR }}
                    />
                  </View>

                  {/* Unscheduled idle */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text
                        style={[styles.settingLabel, { color: colors.black }]}
                      >
                        未スケジュール放置通知
                      </Text>
                      <Switch
                        value={notifications.unscheduledIdle}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({
                            ...notifications,
                            unscheduledIdle: v,
                          });
                        }}
                        trackColor={{ true: BRAND_COLOR }}
                      />
                    </View>
                    {notifications.unscheduledIdle && (
                      <View style={styles.field}>
                        <Text style={[styles.label, { color: colors.gray }]}>
                          何時間放置で通知 (時間)
                        </Text>
                        <TextInput
                          style={[
                            styles.input,
                            {
                              borderColor: colors.separator,
                              color: colors.black,
                            },
                          ]}
                          value={idleHoursInput}
                          onChangeText={setIdleHoursInput}
                          onBlur={commitIdleHours}
                          onSubmitEditing={commitIdleHours}
                          keyboardType="numeric"
                          placeholder="24"
                          placeholderTextColor={colors.gray}
                        />
                      </View>
                    )}
                  </View>

                  {/* In-progress */}
                  <View style={styles.settingRow}>
                    <Text
                      style={[styles.settingLabel, { color: colors.black }]}
                    >
                      タスク実行中通知
                    </Text>
                    <Switch
                      value={notifications.inProgress}
                      onValueChange={(v) => {
                        haptic.select();
                        setNotifications({ ...notifications, inProgress: v });
                      }}
                      trackColor={{ true: BRAND_COLOR }}
                    />
                  </View>
                </>
              )}
            </>
          )}

          {category === 'worker' && (
            <>
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  エンドポイント
                </Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={workerUrl}
                  onChangeText={(v) => {
                    setWorkerUrl(v);
                    setWorkerDirty(true);
                  }}
                  placeholder="https://your-worker.workers.dev"
                  placeholderTextColor={colors.gray}
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>キー</Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={workerKey}
                  onChangeText={(v) => {
                    setWorkerKey(v);
                    setWorkerDirty(true);
                  }}
                  placeholder="tsk_..."
                  placeholderTextColor={colors.gray}
                  secureTextEntry
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>

              {workerDirty && (
                <Text style={[styles.warning, { color: colors.red }]}>
                  ⚠ サーバーを再起動するまで反映されません
                </Text>
              )}

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.medium();
                  handleRestartServer();
                }}
                disabled={restarting}
              >
                {restarting ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>サーバーを再起動</Text>
                )}
              </Pressable>

              {/* Health checks */}
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  ヘルスチェック
                </Text>
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.light();
                  checkLocalHealth();
                }}
                disabled={localHealthLoading || !client}
              >
                {localHealthLoading ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>ローカルサーバー</Text>
                )}
              </Pressable>
              {localHealthResult && (
                <Text
                  style={[
                    styles.healthResult,
                    {
                      color: localHealthResult.startsWith('✓')
                        ? colors.black
                        : colors.red,
                    },
                  ]}
                >
                  {localHealthResult}
                </Text>
              )}

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.light();
                  checkWorkerHealth();
                }}
                disabled={workerHealthLoading || !client}
              >
                {workerHealthLoading ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>Worker</Text>
                )}
              </Pressable>
              {workerHealthResult && (
                <Text
                  style={[
                    styles.healthResult,
                    {
                      color: workerHealthResult.startsWith('✓')
                        ? colors.black
                        : colors.red,
                    },
                  ]}
                >
                  {workerHealthResult}
                </Text>
              )}
            </>
          )}

          {category === 'google' && (
            <>
              {gcalLoading && (
                <ActivityIndicator color={BRAND_COLOR} style={styles.loader} />
              )}

              {gcalSettings && (
                <View
                  style={[
                    styles.statusBox,
                    { backgroundColor: colors.grayLight + '20' },
                  ]}
                >
                  <Text style={[styles.label, { color: colors.gray }]}>
                    状態
                  </Text>
                  <Text style={[styles.value, { color: colors.black }]}>
                    有効: {gcalSettings.enabled ? 'はい' : 'いいえ'}
                    {'\n'}client_id:{' '}
                    {gcalSettings.client_id ? '設定済み' : '未設定'}
                    {'\n'}client_secret:{' '}
                    {gcalSettings.has_client_secret ? '設定済み' : '未設定'}
                    {'\n'}refresh_token:{' '}
                    {gcalSettings.has_refresh_token ? '設定済み' : '未設定'}
                  </Text>
                </View>
              )}

              <View style={styles.settingRow}>
                <Text style={[styles.settingLabel, { color: colors.black }]}>
                  有効化
                </Text>
                <Switch
                  value={gcalEnabled}
                  onValueChange={(v) => {
                    haptic.select();
                    setGcalEnabled(v);
                  }}
                  trackColor={{ true: BRAND_COLOR }}
                />
              </View>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  Calendar ID
                </Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={gcalCalendarId}
                  onChangeText={setGcalCalendarId}
                  placeholder="primary"
                  placeholderTextColor={colors.gray}
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  Client ID
                </Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={gcalClientId}
                  onChangeText={setGcalClientId}
                  placeholder="xxxxx.apps.googleusercontent.com"
                  placeholderTextColor={colors.gray}
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  Client Secret
                </Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={gcalClientSecret}
                  onChangeText={setGcalClientSecret}
                  placeholder={
                    gcalSettings?.has_client_secret
                      ? '設定済み (入力で上書き)'
                      : 'GOCSPX-...'
                  }
                  placeholderTextColor={colors.gray}
                  secureTextEntry
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.medium();
                  saveGcalSettings();
                }}
              >
                <Text style={styles.actionButtonText}>設定を保存</Text>
              </Pressable>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  Refresh Token
                </Text>
                <TextInput
                  style={[
                    styles.input,
                    { borderColor: colors.separator, color: colors.black },
                  ]}
                  value={gcalRefreshToken}
                  onChangeText={setGcalRefreshToken}
                  placeholder={
                    gcalSettings?.has_refresh_token
                      ? '設定済み (入力で上書き)'
                      : 'CLIでOAuth実行後に貼り付け'
                  }
                  placeholderTextColor={colors.gray}
                  secureTextEntry
                  autoCapitalize="none"
                  autoCorrect={false}
                />
                <Text style={[styles.helpText, { color: colors.gray }]}>
                  CLIで `takusu sync login --client-id 〜 --client-secret 〜`
                  を実行して取得したトークンを貼り付けてください
                </Text>
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.medium();
                  saveRefreshToken();
                }}
                disabled={!gcalRefreshToken.trim()}
              >
                <Text style={styles.actionButtonText}>Refresh Tokenを保存</Text>
              </Pressable>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.medium();
                  triggerSync();
                }}
                disabled={syncLoading || !gcalSettings?.has_refresh_token}
              >
                {syncLoading ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>手動同期</Text>
                )}
              </Pressable>
            </>
          )}

          {category === 'info' && (
            <>
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  バージョン
                </Text>
                <Text style={[styles.value, { color: colors.black }]}>
                  {appVersion} (build {buildVersion})
                </Text>
              </View>
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>Git</Text>
                <Text style={[styles.value, { color: colors.black }]}>
                  {gitTag} @ {gitCommit}
                </Text>
              </View>
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  ライセンス
                </Text>
                <Text style={[styles.value, { color: colors.black }]}>
                  MIT{'\n'}
                  Copyright (c) 2025 satler
                </Text>
              </View>

              {/* Log export */}
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>ログ</Text>
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => {
                  haptic.light();
                  exportLogs();
                }}
                disabled={logExportLoading || Platform.OS !== 'android'}
              >
                {logExportLoading ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>
                    {Platform.OS === 'android'
                      ? 'ログをエクスポート'
                      : 'ログ (Androidのみ)'}
                  </Text>
                )}
              </Pressable>

              <Pressable
                style={[
                  styles.actionButton,
                  { backgroundColor: colors.grayLight },
                ]}
                onPress={() => {
                  haptic.light();
                  copyLogs();
                }}
                disabled={
                  logCopyLoading ||
                  logExportLoading ||
                  Platform.OS !== 'android'
                }
              >
                {logCopyLoading ? (
                  <ActivityIndicator color={colors.black} />
                ) : (
                  <Text
                    style={[styles.actionButtonText, { color: colors.black }]}
                  >
                    ログをコピー
                  </Text>
                )}
              </Pressable>

              <Pressable
                style={[
                  styles.actionButton,
                  { backgroundColor: colors.grayLight },
                ]}
                onPress={() => {
                  haptic.medium();
                  clearLogs();
                }}
                disabled={Platform.OS !== 'android'}
              >
                <Text
                  style={[styles.actionButtonText, { color: colors.black }]}
                >
                  ログを消去
                </Text>
              </Pressable>
            </>
          )}
        </ScrollView>
      </View>

      {/* Notification time picker modal */}
      {notifPickerField && (
        <DateTimePickerModal
          visible={true}
          mode="time"
          label="通知時刻"
          value={(() => {
            const min = notifications.morningBriefingTime;
            const { hour, minute } = minutesToTime(min);
            const d = new Date();
            d.setHours(hour, minute, 0, 0);
            return d;
          })()}
          onConfirm={(date) => {
            if (!date) {
              setNotifPickerField(null);
              return;
            }
            const minutes = timeToMinutes(date.getHours(), date.getMinutes());
            if (notifPickerField === 'morningBriefing') {
              setNotifications({
                ...notifications,
                morningBriefingTime: minutes,
              });
            }
            setNotifPickerField(null);
          }}
          onCancel={() => setNotifPickerField(null)}
        />
      )}

      {/* Sleep time picker modal */}
      {sleepPickerField && (
        <DateTimePickerModal
          visible={true}
          mode="time"
          label={sleepPickerField === 'start' ? '就寝時刻' : '起床時刻'}
          value={(() => {
            const s = sleepPickerField === 'start' ? sleepStart : sleepEnd;
            const [h, m] = s.split(':').map((n) => parseInt(n, 10) || 0);
            const d = new Date();
            d.setHours(h, m, 0, 0);
            return d;
          })()}
          onConfirm={(date) => {
            if (!date) {
              setSleepPickerField(null);
              return;
            }
            const hh = date.getHours().toString().padStart(2, '0');
            const mm = date.getMinutes().toString().padStart(2, '0');
            const formatted = `${hh}:${mm}`;
            if (sleepPickerField === 'start') {
              setSleepStart(formatted);
            } else {
              setSleepEnd(formatted);
            }
            setSleepPickerField(null);
          }}
          onCancel={() => setSleepPickerField(null)}
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingBottom: 8,
    borderBottomWidth: StyleSheet.hairlineWidth,
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
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
    marginLeft: 8,
  },
  body: {
    flex: 1,
  },
  content: {
    padding: 16,
    gap: 16,
  },
  categoryRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 16,
    paddingHorizontal: 4,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  categoryLabel: {
    fontSize: 16,
  },
  settingRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 8,
  },
  settingLabel: {
    fontSize: 16,
  },
  field: {
    gap: 4,
  },
  label: {
    fontSize: 13,
    fontWeight: '500',
  },
  value: {
    fontSize: 16,
  },
  input: {
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
    fontSize: 16,
  },
  helpText: {
    fontSize: 12,
    marginTop: 2,
  },
  warning: {
    fontSize: 13,
    fontWeight: '500',
  },
  statusBox: {
    padding: 12,
    borderRadius: 8,
    gap: 4,
  },
  loader: {
    paddingVertical: 16,
  },
  actionButton: {
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderRadius: 8,
    alignItems: 'center',
  },
  actionButtonText: {
    color: '#FFFFFF',
    fontSize: 14,
    fontWeight: '600',
  },
  notifGroup: {
    gap: 8,
  },
  timeField: {
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    alignItems: 'flex-end',
  },
  timeText: {
    fontSize: 16,
    fontWeight: '500',
    fontVariant: ['tabular-nums'],
  },
  healthResult: {
    fontSize: 13,
    fontFamily: 'monospace',
    paddingHorizontal: 4,
  },
});
