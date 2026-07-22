import { useMemo, useState } from 'react';
import {
  Pressable,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useColors, BRAND_COLOR } from '@/src/theme';
import type { PermissionsMap } from '@/src/api/settingsStore';

// Keep this list in sync with the operations produced by Rust tools:
// - crates/takusu-agent/src/tools/takusu.rs (MutationKind)
// - crates/takusu-agent/src/tools/progress.rs (start/pause/progress/complete/split)
// - crates/takusu-agent/src/tools/memory.rs (create/update/delete)
// - crates/takusu-agent/src/tools/skills.rs (create/update/delete)
const ALL_KEY = '*:*';

interface PermissionItem {
  key: string;
  title: string;
  danger?: boolean;
}

interface PermissionCategory {
  key: string;
  title: string;
  permissions: PermissionItem[];
}

const CATEGORIES: PermissionCategory[] = [
  {
    key: 'task',
    title: 'タスク',
    permissions: [
      { key: 'task:create', title: 'タスク作成' },
      { key: 'task:update', title: 'タスク更新' },
      { key: 'task:delete', title: 'タスク削除', danger: true },
      { key: 'task:move', title: 'タスク移動' },
      { key: 'task:start', title: 'タスク開始' },
      { key: 'task:pause', title: 'タスク一時停止' },
      { key: 'task:progress', title: 'タスク進捗' },
      { key: 'task:complete', title: 'タスク完了' },
      { key: 'task:split', title: 'タスク分割' },
    ],
  },
  {
    key: 'habit',
    title: '習慣',
    permissions: [
      { key: 'habit:create', title: '習慣作成' },
      { key: 'habit:update', title: '習慣更新' },
      { key: 'habit:delete', title: '習慣削除', danger: true },
    ],
  },
  {
    key: 'schedule',
    title: 'スケジュール',
    permissions: [
      { key: 'schedule:generate', title: 'スケジュール生成' },
      { key: 'schedule:reschedule', title: 'スケジュール再生成' },
    ],
  },
  {
    key: 'memory',
    title: '記憶',
    permissions: [
      { key: 'memory:create', title: '記憶作成' },
      { key: 'memory:update', title: '記憶更新' },
      { key: 'memory:delete', title: '記憶削除', danger: true },
    ],
  },
  {
    key: 'skill',
    title: 'スキル',
    permissions: [
      { key: 'skill:create', title: 'スキル作成' },
      { key: 'skill:update', title: 'スキル更新' },
      { key: 'skill:delete', title: 'スキル削除', danger: true },
    ],
  },
];

interface Props {
  permissions?: PermissionsMap;
  onChange: (permissions: PermissionsMap) => void;
}

function resolvePermission(key: string, permissions?: PermissionsMap): boolean {
  const map = permissions ?? {};
  const exact = map[key];
  if (exact !== undefined) return exact;
  const parts = key.split(':');
  if (parts.length === 2) {
    const [target, operation] = parts;
    const targetWildcard = map[`${target}:*`];
    if (targetWildcard !== undefined) return targetWildcard;
    const opWildcard = map[`*:${operation}`];
    if (opWildcard !== undefined) return opWildcard;
  }
  const all = map[ALL_KEY];
  if (all !== undefined) return all;
  return false;
}

function categoryEnabledCount(
  cat: PermissionCategory,
  permissions?: PermissionsMap,
): number {
  return cat.permissions.filter((p) => resolvePermission(p.key, permissions))
    .length;
}

function isCategoryOn(
  cat: PermissionCategory,
  permissions?: PermissionsMap,
): boolean {
  return cat.permissions.every((p) => resolvePermission(p.key, permissions));
}

export function PermissionsEditor({ permissions, onChange }: Props) {
  const colors = useColors();
  const [search, setSearch] = useState('');
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  const allOn = resolvePermission(ALL_KEY, permissions);

  const visibleCategories = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) {
      return CATEGORIES.map((cat) => ({
        ...cat,
        visiblePermissions: cat.permissions,
      }));
    }
    return CATEGORIES.map((cat) => ({
      ...cat,
      visiblePermissions: cat.permissions.filter(
        (p) =>
          p.title.toLowerCase().includes(q) || p.key.toLowerCase().includes(q),
      ),
    })).filter(
      (cat) =>
        cat.title.toLowerCase().includes(q) ||
        cat.visiblePermissions.length > 0,
    );
  }, [search]);

  function update(key: string, value: boolean) {
    const next = { ...(permissions ?? {}) };
    next[key] = value;
    onChange(next);
  }

  function toggle(key: string) {
    if (allOn) return;
    update(key, !resolvePermission(key, permissions));
  }

  function toggleMaster() {
    update(ALL_KEY, !allOn);
  }

  function toggleCategory(cat: PermissionCategory) {
    if (allOn) return;
    const catOn = isCategoryOn(cat, permissions);
    const next = { ...(permissions ?? {}) };
    cat.permissions.forEach((p) => {
      next[p.key] = !catOn;
    });
    onChange(next);
  }

  function toggleCollapsed(key: string) {
    setCollapsed((prev) => ({ ...prev, [key]: !prev[key] }));
  }

  return (
    <View style={styles.container}>
      <View
        style={[
          styles.searchBar,
          {
            borderColor: colors.separator,
            backgroundColor: colors.surface,
          },
        ]}
      >
        <Ionicons name="search" size={18} color={colors.gray} />
        <TextInput
          style={[styles.searchInput, { color: colors.black }]}
          value={search}
          onChangeText={setSearch}
          placeholder="権限を検索"
          placeholderTextColor={colors.gray}
          autoCapitalize="none"
        />
      </View>

      <Pressable
        onPress={toggleMaster}
        style={[
          styles.masterRow,
          {
            backgroundColor: colors.surfaceTint,
            borderColor: colors.separator,
          },
        ]}
      >
        <Text style={[styles.masterTitle, { color: colors.black }]}>
          すべて自動承認
        </Text>
        <Switch
          value={allOn}
          onValueChange={toggleMaster}
          accessibilityLabel="すべて自動承認"
          trackColor={{ false: colors.grayLight, true: BRAND_COLOR }}
        />
      </Pressable>

      {visibleCategories.map((cat) => {
        const isCollapsed = !!collapsed[cat.key];
        const catOn = isCategoryOn(cat, permissions);
        const enabledCount = categoryEnabledCount(cat, permissions);

        return (
          <View
            key={cat.key}
            style={[
              styles.category,
              {
                borderColor: colors.separator,
                backgroundColor: colors.surface,
              },
            ]}
          >
            <View
              style={[
                styles.categoryHeader,
                { backgroundColor: colors.surfaceTint },
              ]}
            >
              <Pressable
                onPress={() => toggleCollapsed(cat.key)}
                style={styles.categoryHeaderMain}
              >
                <Text style={[styles.categoryTitle, { color: colors.black }]}>
                  {cat.title}
                </Text>
                <Text style={[styles.categoryMeta, { color: colors.gray }]}>
                  {enabledCount}/{cat.permissions.length}
                </Text>
                <Ionicons
                  name={isCollapsed ? 'chevron-forward' : 'chevron-down'}
                  size={16}
                  color={colors.gray}
                />
              </Pressable>
              <Switch
                value={catOn}
                onValueChange={() => toggleCategory(cat)}
                disabled={allOn}
                accessibilityLabel={cat.title}
                trackColor={{ false: colors.grayLight, true: BRAND_COLOR }}
              />
            </View>

            {!isCollapsed && (
              <View style={styles.categoryBody}>
                {cat.visiblePermissions.map((p) => {
                  const on = resolvePermission(p.key, permissions);
                  return (
                    <Pressable
                      key={p.key}
                      onPress={() => toggle(p.key)}
                      disabled={allOn}
                      style={[
                        styles.permissionRow,
                        {
                          borderColor: p.danger ? colors.red : colors.separator,
                          backgroundColor: colors.surface,
                        },
                      ]}
                    >
                      <Text
                        style={[
                          styles.permissionTitle,
                          { color: colors.black },
                        ]}
                      >
                        {p.title}
                      </Text>
                      <Switch
                        value={on}
                        onValueChange={() => toggle(p.key)}
                        disabled={allOn}
                        accessibilityLabel={p.title}
                        trackColor={{
                          false: colors.grayLight,
                          true: BRAND_COLOR,
                        }}
                      />
                    </Pressable>
                  );
                })}
              </View>
            )}
          </View>
        );
      })}
    </View>
  );
}

const styles = StyleSheet.create({
  container: { gap: 10 },
  searchBar: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    padding: 10,
    borderWidth: 1,
    borderRadius: 10,
  },
  searchInput: {
    flex: 1,
    fontSize: 14,
    padding: 0,
  },
  masterRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: 10,
    borderWidth: 1,
    borderRadius: 12,
    minHeight: 40,
  },
  masterTitle: { fontSize: 15, fontWeight: '700' },
  category: {
    borderWidth: 1,
    borderRadius: 14,
    overflow: 'hidden',
  },
  categoryHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 12,
    minHeight: 40,
  },
  categoryHeaderMain: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  categoryTitle: { flex: 1, fontSize: 15, fontWeight: '700' },
  categoryMeta: { fontSize: 12 },
  categoryBody: {
    paddingBottom: 6,
  },
  permissionRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: 6,
    paddingHorizontal: 10,
    borderWidth: 1,
    borderRadius: 10,
    marginHorizontal: 6,
    marginBottom: 4,
    minHeight: 40,
  },
  permissionTitle: { fontSize: 14, fontWeight: '600' },
});
