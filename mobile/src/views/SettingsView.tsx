// SettingsView — categorized settings
// general: dark/white theme
// worker: endpoint, key (with server restart)
// google calendar: config + OAuth + manual sync
// info: license, version (build number)

import { useCallback, useEffect, useRef, useState } from 'react';
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
import { useRouter } from 'expo-router';
import * as WebBrowser from 'expo-web-browser';
import * as Linking from 'expo-linking';
import * as Application from 'expo-application';
import * as FileSystem from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import * as Clipboard from 'expo-clipboard';
import Constants from 'expo-constants';
import { useServer, saveWorkersUrl, saveWorkersToken } from '@/src/api/ServerProvider';
import { setOAuthCallbackListener } from '@/src/api/oauthCallback';
import type { GoogleCalSettings } from '@/src/api/types';
import { useColors, BRAND_COLOR } from '@/src/theme';
import type { NotificationSettings } from '@/src/notifications/settings';
import { formatTime, minutesToTime, timeToMinutes } from '@/src/notifications/settings';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { DateTimePickerModal } from '@/src/components/DateTimePickerModal';
import { haptic } from '@/src/components/haptics';
import TakusuServerModule from '../../modules/takusu-server/src/TakusuServerModule';

type SettingsCategory = 'general' | 'notifications' | 'worker' | 'google' | 'info';

const OAUTH_REDIRECT_URI = Linking.createURL('oauth/callback');

export function SettingsView() {
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
  const [category, setCategory] = useState<SettingsCategory>('general');
  const [notifPickerField, setNotifPickerField] = useState<
    'morningBriefing' | 'eveningSummary' | 'habitReminder' | null
  >(null);

  // Worker tab state
  const [workerUrl, setWorkerUrl] = useState(savedUrl);
  const [workerKey, setWorkerKey] = useState(savedToken);
  const [workerDirty, setWorkerDirty] = useState(false);

  // Undo steps input — local text state, committed on blur to avoid
  // trimming the undo stack through intermediate values while typing.
  const [undoStepsInput, setUndoStepsInput] = useState(String(undoSteps));

  // Google Calendar state
  const [gcalSettings, setGcalSettings] = useState<GoogleCalSettings | null>(null);
  const [gcalEnabled, setGcalEnabled] = useState(false);
  const [gcalCalendarId, setGcalCalendarId] = useState('');
  const [gcalClientId, setGcalClientId] = useState('');
  const [gcalClientSecret, setGcalClientSecret] = useState('');
  const [gcalLoading, setGcalLoading] = useState(false);
  const [oauthLoading, setOauthLoading] = useState(false);
  const [syncLoading, setSyncLoading] = useState(false);

  // Health check state (info tab)
  const [localHealthLoading, setLocalHealthLoading] = useState(false);
  const [localHealthResult, setLocalHealthResult] = useState<string | null>(null);
  const [workerHealthLoading, setWorkerHealthLoading] = useState(false);
  const [workerHealthResult, setWorkerHealthResult] = useState<string | null>(null);
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

  // OAuth callback listener — registered when google tab is active
  // Guard against double-firing: if startOAuth already handled the code
  // via openAuthSessionAsync result, the deep link listener is skipped.
  const oauthHandledRef = useRef(false);

  useEffect(() => {
    if (category !== 'google' || !client) return;

    setOAuthCallbackListener(async (code: string) => {
      if (oauthHandledRef.current) {
        oauthHandledRef.current = false;
        return;
      }
      try {
        await client.oauthCallback(code, OAUTH_REDIRECT_URI);
        await loadGcalSettings();
        Alert.alert('成功', 'Google Calendar認証が完了しました');
      } catch (e) {
        Alert.alert('エラー', `OAuth認証に失敗しました: ${e instanceof Error ? e.message : String(e)}`);
      } finally {
        setOauthLoading(false);
      }
    });

    return () => {
      setOAuthCallbackListener(null);
    };
  }, [category, client, loadGcalSettings]);

  async function saveWorkerSettings() {
    await saveWorkersUrl(workerUrl);
    await saveWorkersToken(workerKey);
    setWorkerDirty(false);
  }

  async function handleRestartServer() {
    await saveWorkerSettings();
    await restartServer(workerUrl, workerKey);
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
      Alert.alert('エラー', `保存に失敗: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async function startOAuth() {
    if (!client) return;
    setOauthLoading(true);
    oauthHandledRef.current = false;
    try {
      const { url } = await client.getOAuthUrl(OAUTH_REDIRECT_URI);
      const result = await WebBrowser.openAuthSessionAsync(url, OAUTH_REDIRECT_URI);
      // On Android, openAuthSessionAsync may return the redirect URL directly
      if (result.type === 'success' && result.url) {
        const parsed = Linking.parse(result.url);
        const code = parsed.queryParams?.code;
        if (typeof code === 'string' && code) {
          oauthHandledRef.current = true; // suppress deep link listener
          await client.oauthCallback(code, OAUTH_REDIRECT_URI);
          await loadGcalSettings();
          Alert.alert('成功', 'Google Calendar認証が完了しました');
        }
      }
      // If result.type is not 'success', the deep link listener will handle it
    } catch (e) {
      Alert.alert('エラー', `OAuth開始に失敗: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setOauthLoading(false);
    }
  }

  async function triggerSync() {
    if (!client) return;
    setSyncLoading(true);
    try {
      await client.triggerSync();
      Alert.alert('同期完了', 'Google Calendarへ同期しました');
    } catch (e) {
      Alert.alert('エラー', `同期に失敗: ${e instanceof Error ? e.message : String(e)}`);
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
      Alert.alert('エラー', `ログエクスポートに失敗: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLogExportLoading(false);
    }
  }

  async function clearLogs() {
    try {
      await TakusuServerModule.clearLogs();
      Alert.alert('消去しました', 'ログバッファをクリアしました');
    } catch (e) {
      Alert.alert('エラー', `ログクリアに失敗: ${e instanceof Error ? e.message : String(e)}`);
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
      Alert.alert('コピーしました', `${lines.length} 行のログをクリップボードにコピーしました`);
    } catch (e) {
      Alert.alert('エラー', `ログコピーに失敗: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLogCopyLoading(false);
    }
  }

  const categories: { key: SettingsCategory; label: string }[] = [
    { key: 'general', label: '一般' },
    { key: 'notifications', label: '通知' },
    { key: 'worker', label: 'Worker' },
    { key: 'google', label: 'Google Calendar' },
    { key: 'info', label: '情報' },
  ];

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
        <Pressable style={styles.backButton} onPress={() => { haptic.light(); router.back(); }}>
          <Text style={[styles.backButtonText, { color: BRAND_COLOR }]}>‹</Text>
        </Pressable>
        <Text style={[styles.title, { color: colors.black }]}>設定</Text>
      </View>

      <View style={styles.body}>
        {/* Category tabs */}
        <View style={[styles.tabs, { borderBottomColor: colors.separator }]}>
          {categories.map((c) => (
            <Pressable
              key={c.key}
              style={[styles.tab, category === c.key && { borderBottomColor: BRAND_COLOR }]}
              onPress={() => { if (category !== c.key) haptic.select(); setCategory(c.key); }}
            >
              <Text
                style={[
                  styles.tabText,
                  { color: colors.gray },
                  category === c.key && { color: BRAND_COLOR, fontWeight: '600' },
                ]}
              >
                {c.label}
              </Text>
            </Pressable>
          ))}
        </View>

        <ScrollView
          contentContainerStyle={[styles.content, { paddingBottom: 16 + insets.bottom }]}
        >
          {category === 'general' && (
            <>
              <View style={styles.settingRow}>
                <Text style={[styles.settingLabel, { color: colors.black }]}>
                  ダークモード
                </Text>
                <Switch
                  value={darkMode}
                  onValueChange={(v) => { haptic.select(); setDarkMode(v); }}
                  trackColor={{ true: BRAND_COLOR }}
                />
              </View>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  アンドゥ履歴の上限 (ステップ数)
                </Text>
                <TextInput
                  style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
                  value={undoStepsInput}
                  onChangeText={setUndoStepsInput}
                  onBlur={commitUndoSteps}
                  onSubmitEditing={commitUndoSteps}
                  keyboardType="numeric"
                  placeholder="50"
                  placeholderTextColor={colors.gray}
                />
              </View>
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
                      <Text style={[styles.settingLabel, { color: colors.black }]}>
                        朝のブリーフィング
                      </Text>
                      <Switch
                        value={notifications.morningBriefing}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({ ...notifications, morningBriefing: v });
                        }}
                        trackColor={{ true: BRAND_COLOR }}
                      />
                    </View>
                    {notifications.morningBriefing && (
                      <Pressable
                        style={[styles.timeField, { borderColor: colors.separator }]}
                        onPress={() => { haptic.select(); setNotifPickerField('morningBriefing'); }}
                      >
                        <Text style={[styles.timeText, { color: colors.black }]}>
                          {formatTime(notifications.morningBriefingTime)}
                        </Text>
                      </Pressable>
                    )}
                  </View>

                  {/* Pre-start reminder */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text style={[styles.settingLabel, { color: colors.black }]}>
                        開始直前リマインダー
                      </Text>
                      <Switch
                        value={notifications.preStartReminder}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({ ...notifications, preStartReminder: v });
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
                          style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
                          value={String(notifications.preStartReminderMinutes)}
                          onChangeText={(v) => {
                            const n = parseInt(v, 10);
                            if (!isNaN(n) && n > 0) {
                              setNotifications({
                                ...notifications,
                                preStartReminderMinutes: n,
                              });
                            }
                          }}
                          keyboardType="numeric"
                          placeholder="10"
                          placeholderTextColor={colors.gray}
                        />
                      </View>
                    )}
                  </View>

                  {/* Start overdue */}
                  <View style={styles.settingRow}>
                    <Text style={[styles.settingLabel, { color: colors.black }]}>
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

                  {/* Unscheduled idle */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text style={[styles.settingLabel, { color: colors.black }]}>
                        未スケジュール放置通知
                      </Text>
                      <Switch
                        value={notifications.unscheduledIdle}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({ ...notifications, unscheduledIdle: v });
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
                          style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
                          value={String(notifications.unscheduledIdleHours)}
                          onChangeText={(v) => {
                            const n = parseInt(v, 10);
                            if (!isNaN(n) && n > 0) {
                              setNotifications({
                                ...notifications,
                                unscheduledIdleHours: n,
                              });
                            }
                          }}
                          keyboardType="numeric"
                          placeholder="24"
                          placeholderTextColor={colors.gray}
                        />
                      </View>
                    )}
                  </View>

                  {/* In-progress */}
                  <View style={styles.settingRow}>
                    <Text style={[styles.settingLabel, { color: colors.black }]}>
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

                  {/* Evening summary */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text style={[styles.settingLabel, { color: colors.black }]}>
                        夕方サマリー
                      </Text>
                      <Switch
                        value={notifications.eveningSummary}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({ ...notifications, eveningSummary: v });
                        }}
                        trackColor={{ true: BRAND_COLOR }}
                      />
                    </View>
                    {notifications.eveningSummary && (
                      <Pressable
                        style={[styles.timeField, { borderColor: colors.separator }]}
                        onPress={() => { haptic.select(); setNotifPickerField('eveningSummary'); }}
                      >
                        <Text style={[styles.timeText, { color: colors.black }]}>
                          {formatTime(notifications.eveningSummaryTime)}
                        </Text>
                      </Pressable>
                    )}
                  </View>

                  {/* Habit reminder */}
                  <View style={styles.notifGroup}>
                    <View style={styles.settingRow}>
                      <Text style={[styles.settingLabel, { color: colors.black }]}>
                        Habit未完了リマインダー
                      </Text>
                      <Switch
                        value={notifications.habitReminder}
                        onValueChange={(v) => {
                          haptic.select();
                          setNotifications({ ...notifications, habitReminder: v });
                        }}
                        trackColor={{ true: BRAND_COLOR }}
                      />
                    </View>
                    {notifications.habitReminder && (
                      <Pressable
                        style={[styles.timeField, { borderColor: colors.separator }]}
                        onPress={() => { haptic.select(); setNotifPickerField('habitReminder'); }}
                      >
                        <Text style={[styles.timeText, { color: colors.black }]}>
                          {formatTime(notifications.habitReminderTime)}
                        </Text>
                      </Pressable>
                    )}
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
                  style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
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
                  style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
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
                onPress={() => { haptic.medium(); handleRestartServer(); }}
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
                <Text style={[styles.label, { color: colors.gray }]}>ヘルスチェック</Text>
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => { haptic.light(); checkLocalHealth(); }}
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
                    { color: localHealthResult.startsWith('✓') ? colors.black : colors.red },
                  ]}
                >
                  {localHealthResult}
                </Text>
              )}

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => { haptic.light(); checkWorkerHealth(); }}
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
                    { color: workerHealthResult.startsWith('✓') ? colors.black : colors.red },
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
                <View style={[styles.statusBox, { backgroundColor: colors.grayLight + '20' }]}>
                  <Text style={[styles.label, { color: colors.gray }]}>状態</Text>
                  <Text style={[styles.value, { color: colors.black }]}>
                    有効: {gcalSettings.enabled ? 'はい' : 'いいえ'}
                    {'\n'}client_id: {gcalSettings.client_id ? '設定済み' : '未設定'}
                    {'\n'}client_secret: {gcalSettings.has_client_secret ? '設定済み' : '未設定'}
                    {'\n'}refresh_token: {gcalSettings.has_refresh_token ? '設定済み' : '未設定'}
                  </Text>
                </View>
              )}

              <View style={styles.settingRow}>
                <Text style={[styles.settingLabel, { color: colors.black }]}>
                  有効化
                </Text>
                <Switch
                  value={gcalEnabled}
                  onValueChange={(v) => { haptic.select(); setGcalEnabled(v); }}
                  trackColor={{ true: BRAND_COLOR }}
                />
              </View>

              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>
                  Calendar ID
                </Text>
                <TextInput
                  style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
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
                  style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
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
                  style={[styles.input, { borderColor: colors.separator, color: colors.black }]}
                  value={gcalClientSecret}
                  onChangeText={setGcalClientSecret}
                  placeholder={gcalSettings?.has_client_secret ? '設定済み (入力で上書き)' : 'GOCSPX-...'}
                  placeholderTextColor={colors.gray}
                  secureTextEntry
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => { haptic.medium(); saveGcalSettings(); }}
              >
                <Text style={styles.actionButtonText}>設定を保存</Text>
              </Pressable>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => { haptic.medium(); startOAuth(); }}
                disabled={oauthLoading || !gcalSettings?.has_client_secret}
              >
                {oauthLoading ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>OAuth認証を開始</Text>
                )}
              </Pressable>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => { haptic.medium(); triggerSync(); }}
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
                <Text style={[styles.label, { color: colors.gray }]}>バージョン</Text>
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
                <Text style={[styles.label, { color: colors.gray }]}>ライセンス</Text>
                <Text style={[styles.value, { color: colors.black }]}>MIT</Text>
              </View>

              {/* Log export */}
              <View style={styles.field}>
                <Text style={[styles.label, { color: colors.gray }]}>ログ</Text>
              </View>

              <Pressable
                style={[styles.actionButton, { backgroundColor: BRAND_COLOR }]}
                onPress={() => { haptic.light(); exportLogs(); }}
                disabled={logExportLoading || Platform.OS !== 'android'}
              >
                {logExportLoading ? (
                  <ActivityIndicator color="#FFFFFF" />
                ) : (
                  <Text style={styles.actionButtonText}>
                    {Platform.OS === 'android' ? 'ログをエクスポート' : 'ログ (Androidのみ)'}
                  </Text>
                )}
              </Pressable>

              <Pressable
                style={[styles.actionButton, { backgroundColor: colors.grayLight }]}
                onPress={() => { haptic.light(); copyLogs(); }}
                disabled={logCopyLoading || logExportLoading || Platform.OS !== 'android'}
              >
                {logCopyLoading ? (
                  <ActivityIndicator color={colors.black} />
                ) : (
                  <Text style={[styles.actionButtonText, { color: colors.black }]}>
                    ログをコピー
                  </Text>
                )}
              </Pressable>

              <Pressable
                style={[styles.actionButton, { backgroundColor: colors.grayLight }]}
                onPress={() => { haptic.medium(); clearLogs(); }}
                disabled={Platform.OS !== 'android'}
              >
                <Text style={[styles.actionButtonText, { color: colors.black }]}>
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
            const min =
              notifPickerField === 'morningBriefing'
                ? notifications.morningBriefingTime
                : notifPickerField === 'eveningSummary'
                  ? notifications.eveningSummaryTime
                  : notifications.habitReminderTime;
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
              setNotifications({ ...notifications, morningBriefingTime: minutes });
            } else if (notifPickerField === 'eveningSummary') {
              setNotifications({ ...notifications, eveningSummaryTime: minutes });
            } else if (notifPickerField === 'habitReminder') {
              setNotifications({ ...notifications, habitReminderTime: minutes });
            }
            setNotifPickerField(null);
          }}
          onCancel={() => setNotifPickerField(null)}
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
  tabs: {
    flexDirection: 'row',
    paddingHorizontal: 8,
    gap: 4,
    borderBottomWidth: 1,
  },
  tab: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderBottomWidth: 2,
    borderBottomColor: 'transparent',
  },
  tabText: {
    fontSize: 14,
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
