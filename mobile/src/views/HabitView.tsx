// HabitView — list of habit cards with add button
// Habits are selectable, context menu (left) changes with selection

import { useCallback, useEffect, useState } from 'react';
import {
  FlatList,
  Pressable,
  RefreshControl,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useRouter } from 'expo-router';
import { Ionicons } from '@expo/vector-icons';
import { IconButton } from 'react-native-paper';
import type { TakusuClient } from '@/src/api/client';
import { showError, logError } from '@/src/api/errors';
import type { HabitRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { ContextMenu } from '@/src/components/ContextMenu';
import { undoRedo } from '@/src/api/undoRedo';

interface HabitViewProps {
  client: TakusuClient | null;
}

export function HabitView({ client }: HabitViewProps) {
  const router = useRouter();
  const colors = useColors();
  const [habits, setHabits] = useState<HabitRow[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    if (!client) return;
    setRefreshing(true);
    try {
      setHabits(await client.listHabits());
    } catch (e) {
      showError(e, 'ハビット一覧の取得に失敗');
    } finally {
      setRefreshing(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function deleteSelected() {
    if (!client) return;
    const toDelete = habits.filter((h) => selected.has(h.id));
    const deleted: HabitRow[] = [];
    let failed = 0;
    for (const h of toDelete) {
      try {
        await client.deleteHabit(h.id);
        deleted.push(h);
      } catch (e) {
        failed++;
        logError(`ハビット削除 (${h.id})`, e);
      }
    }
    if (failed > 0) {
      showError(`${failed}件の削除に失敗しました`, 'ハビットの削除');
    }
    if (deleted.length === 0) return;
    // Track the ids assigned by the server when undo recreates the habits,
    // so redo deletes the recreated (not the stale original) ids.
    // Push a single grouped undo entry so one undo restores all habits.
    const currentIds: string[] = [...deleted.map((h) => h.id)];
    // Track which items have been recreated so a retry after partial failure
    // doesn't create duplicates.
    const createdIdx = new Set<number>();
    undoRedo.push({
      description:
        deleted.length === 1
          ? `delete habit: ${deleted[0].title}`
          : `delete ${deleted.length} habits`,
      undo: async () => {
        for (let i = 0; i < deleted.length; i++) {
          if (createdIdx.has(i)) continue;
          const h = deleted[i];
          const recreated = await client.createHabit({
            title: h.title,
            description: h.description,
            recurrence: h.recurrence,
            start_time: h.start_time,
            end_time: h.end_time,
            avg_minutes: h.avg_minutes,
            sigma_minutes: h.sigma_minutes,
            parallelizable: h.parallelizable,
            allows_parallel: h.allows_parallel,
            abandonability: h.abandonability,
          });
          // CreateHabit does not accept `active`; restore it via update.
          if (!h.active) {
            await client.updateHabit(recreated.id, { active: h.active });
          }
          currentIds[i] = recreated.id;
          createdIdx.add(i);
        }
        await refresh();
      },
      redo: async () => {
        createdIdx.clear();
        for (const id of currentIds) {
          await client.deleteHabit(id);
        }
        await refresh();
      },
    });
    setSelected(new Set());
    await refresh();
  }

  function toggleSelection(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View style={styles.topBar}>
        <ContextMenu
          hasSelection={selected.size > 0}
          onSettings={() => router.push('/settings')}
          onUndo={() =>
            undoRedo.undo().then(refresh).catch((e) => showError(e, 'アンドゥに失敗'))
          }
          onRedo={() =>
            undoRedo.redo().then(refresh).catch((e) => showError(e, 'リドゥに失敗'))
          }
          onSelectAll={() => setSelected(new Set(habits.map((h) => h.id)))}
          onClearSelection={() => setSelected(new Set())}
          onDeleteSelected={deleteSelected}
        />
        <Text style={[styles.title, { color: colors.black }]}>ハビット</Text>
        <View style={{ flex: 1 }} />
        <IconButton
          icon="plus"
          iconColor={COLORS.white}
          size={24}
          containerColor={BRAND_COLOR}
          onPress={() => router.push('/habit/add')}
        />
      </View>

      <FlatList
        data={habits}
        keyExtractor={(h) => h.id}
        renderItem={({ item: h }) => (
          <Pressable
            style={[
              styles.habitCard,
              { backgroundColor: '#F8F5FC' },
              selected.has(h.id) && styles.habitCardSelected,
            ]}
            onPress={() => {
              if (selected.size > 0) {
                toggleSelection(h.id);
              } else {
                router.push(`/habit/${h.id}`);
              }
            }}
            onLongPress={() => toggleSelection(h.id)}
          >
            <View style={styles.habitHeader}>
              <Text style={[styles.habitTitle, { color: colors.black }]}>
                {h.title}
              </Text>
              {selected.has(h.id) && (
                <Ionicons name="checkmark-circle" size={20} color={BRAND_COLOR} />
              )}
            </View>
            <Text style={[styles.habitRecurrence, { color: colors.gray }]}>
              周期: {h.recurrence}
            </Text>
            <Text style={[styles.habitCost, { color: colors.gray }]}>
              {h.avg_minutes}m ±{h.sigma_minutes}
            </Text>
            <Text style={[styles.habitAbandon, { color: colors.gray }]}>
              abandon: {h.abandonability.toFixed(2)}
            </Text>
          </Pressable>
        )}
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={refresh} />
        }
        contentContainerStyle={styles.listContent}
      />
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
    paddingHorizontal: 4,
    paddingTop: 48,
    paddingBottom: 4,
  },
  title: {
    fontSize: 18,
    fontWeight: '600',
    marginLeft: 4,
  },
  listContent: {
    padding: 12,
    paddingBottom: 100,
    gap: 8,
  },
  habitCard: {
    borderRadius: 12,
    padding: 16,
    gap: 4,
  },
  habitCardSelected: {
    borderWidth: 2,
    borderColor: BRAND_COLOR,
  },
  habitHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  habitTitle: {
    fontSize: 16,
    fontWeight: '600',
    flex: 1,
  },
  habitRecurrence: {
    fontSize: 13,
  },
  habitCost: {
    fontSize: 13,
  },
  habitAbandon: {
    fontSize: 13,
  },
});
