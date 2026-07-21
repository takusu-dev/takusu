import { Pressable, StyleSheet, Switch, Text, View } from 'react-native';
import { useColors, BRAND_COLOR } from '@/src/theme';
import type { PermissionsMap } from '@/src/api/settingsStore';

// Keep this list in sync with the operations produced by Rust tools:
// - crates/takusu-agent/src/tools/takusu.rs (MutationKind)
// - crates/takusu-agent/src/tools/progress.rs (start/pause/progress/complete/split)
// - crates/takusu-agent/src/tools/memory.rs (create/update/delete)
// - crates/takusu-agent/src/tools/skills.rs (create/update/delete)
const PERMISSION_KEYS = [
  '*:*',
  'task:create',
  'task:update',
  'task:delete',
  'task:move',
  'task:start',
  'task:pause',
  'task:progress',
  'task:complete',
  'task:split',
  'habit:create',
  'habit:update',
  'habit:delete',
  'schedule:generate',
  'schedule:reschedule',
  'memory:create',
  'memory:update',
  'memory:delete',
  'skill:create',
  'skill:update',
  'skill:delete',
];

interface Props {
  permissions?: PermissionsMap;
  onChange: (permissions: PermissionsMap) => void;
}

function labelForKey(key: string): string {
  switch (key) {
    case '*:*':
      return 'すべて自動承認';
    case 'task:create':
      return 'タスク作成';
    case 'task:update':
      return 'タスク更新';
    case 'task:delete':
      return 'タスク削除';
    case 'task:move':
      return 'タスク移動';
    case 'task:start':
      return 'タスク開始';
    case 'task:pause':
      return 'タスク一時停止';
    case 'task:progress':
      return 'タスク進捗';
    case 'task:complete':
      return 'タスク完了';
    case 'task:split':
      return 'タスク分割';
    case 'habit:create':
      return '習慣作成';
    case 'habit:update':
      return '習慣更新';
    case 'habit:delete':
      return '習慣削除';
    case 'schedule:generate':
      return 'スケジュール生成';
    case 'schedule:reschedule':
      return 'スケジュール再生成';
    case 'memory:create':
      return '記憶作成';
    case 'memory:update':
      return '記憶更新';
    case 'memory:delete':
      return '記憶削除';
    case 'skill:create':
      return 'スキル作成';
    case 'skill:update':
      return 'スキル更新';
    case 'skill:delete':
      return 'スキル削除';
    default:
      return key;
  }
}

export function PermissionsEditor({ permissions, onChange }: Props) {
  const colors = useColors();

  function toggle(key: string) {
    const next = { ...(permissions ?? {}) };
    next[key] = !next[key];
    onChange(next);
  }

  return (
    <View style={styles.container}>
      {PERMISSION_KEYS.map((key) => {
        const enabled = !!permissions?.[key];
        return (
          <Pressable
            key={key}
            onPress={() => toggle(key)}
            style={[styles.row, { borderColor: colors.separator }]}
          >
            <Text style={[styles.label, { color: colors.black }]}>
              {labelForKey(key)}
            </Text>
            <Switch
              value={enabled}
              onValueChange={() => toggle(key)}
              trackColor={{ false: colors.grayLight, true: BRAND_COLOR }}
            />
          </Pressable>
        );
      })}
    </View>
  );
}

const styles = StyleSheet.create({
  container: { gap: 2 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: 6,
    paddingHorizontal: 4,
    borderBottomWidth: 1,
  },
  label: { fontSize: 14 },
});
