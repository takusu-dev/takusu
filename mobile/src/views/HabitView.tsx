// HabitView — list of habit cards with add button
// Habits are selectable, context menu changes with selection

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
import type { TakusuClient } from '@/src/api/client';
import type { HabitRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR } from '@/src/theme';

interface HabitViewProps {
  client: TakusuClient | null;
  onBack: () => void;
}

export function HabitView({ client, onBack }: HabitViewProps) {
  const router = useRouter();
  const [habits, setHabits] = useState<HabitRow[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    if (!client) return;
    setRefreshing(true);
    try {
      setHabits(await client.listHabits());
    } finally {
      setRefreshing(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function deleteSelected() {
    if (!client) return;
    for (const id of selected) {
      await client.deleteHabit(id);
    }
    setSelected(new Set());
    await refresh();
  }

  return (
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={onBack}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <Text style={styles.title}>ハビット</Text>
        <View style={{ flex: 1 }} />
        {selected.size > 0 && (
          <Pressable style={styles.deleteButton} onPress={deleteSelected}>
            <Text style={styles.deleteButtonText}>削除</Text>
          </Pressable>
        )}
      </View>

      <FlatList
        data={habits}
        keyExtractor={(h) => h.id}
        renderItem={({ item: h }) => (
          <Pressable
            style={[
              styles.habitCard,
              selected.has(h.id) && styles.habitCardSelected,
            ]}
            onPress={() => {
              if (selected.size > 0) {
                setSelected((prev) => {
                  const next = new Set(prev);
                  if (next.has(h.id)) next.delete(h.id);
                  else next.add(h.id);
                  return next;
                });
              } else {
                router.push(`/habit/${h.id}`);
              }
            }}
            onLongPress={() =>
              setSelected((prev) => {
                const next = new Set(prev);
                if (next.has(h.id)) next.delete(h.id);
                else next.add(h.id);
                return next;
              })
            }
          >
            <Text style={styles.habitTitle}>{h.title}</Text>
            <Text style={styles.habitRecurrence}>周期: {h.recurrence}</Text>
            <Text style={styles.habitCost}>
              {h.avg_minutes}m ±{h.sigma_minutes}
            </Text>
            <Text style={styles.habitAbandon}>
              abandon: {h.abandonability.toFixed(2)}
            </Text>
          </Pressable>
        )}
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={refresh} />
        }
        contentContainerStyle={styles.listContent}
      />

      <Pressable
        style={styles.addButton}
        onPress={() => router.push('/habit/add')}
      >
        <Text style={styles.addButtonText}>+</Text>
      </Pressable>
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
  deleteButton: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    backgroundColor: COLORS.red,
    borderRadius: 8,
  },
  deleteButtonText: {
    color: COLORS.white,
    fontSize: 14,
  },
  listContent: {
    padding: 12,
    paddingBottom: 100,
    gap: 8,
  },
  habitCard: {
    backgroundColor: '#F8F5FC',
    borderRadius: 12,
    padding: 16,
    gap: 4,
  },
  habitCardSelected: {
    borderWidth: 2,
    borderColor: BRAND_COLOR,
  },
  habitTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: COLORS.black,
  },
  habitRecurrence: {
    fontSize: 13,
    color: COLORS.gray,
  },
  habitCost: {
    fontSize: 13,
    color: COLORS.gray,
  },
  habitAbandon: {
    fontSize: 13,
    color: COLORS.gray,
  },
  addButton: {
    position: 'absolute',
    bottom: 24,
    right: 24,
    width: 56,
    height: 56,
    borderRadius: 28,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
    elevation: 4,
  },
  addButtonText: {
    fontSize: 28,
    color: COLORS.white,
    fontWeight: '300',
  },
});
