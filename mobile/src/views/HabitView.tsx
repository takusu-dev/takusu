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
import { Ionicons } from '@expo/vector-icons';
import { Button, IconButton } from 'react-native-paper';
import type { TakusuClient } from '@/src/api/client';
import type { HabitRow } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';

interface HabitViewProps {
  client: TakusuClient | null;
  onBack: () => void;
}

export function HabitView({ client, onBack }: HabitViewProps) {
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
        <IconButton
          icon="chevron-left"
          iconColor={BRAND_COLOR}
          size={28}
          onPress={onBack}
        />
        <Text style={[styles.title, { color: colors.black }]}>ハビット</Text>
        <View style={{ flex: 1 }} />
        {selected.size > 0 ? (
          <View style={styles.contextMenu}>
            <IconButton
              icon="select-all"
              iconColor={BRAND_COLOR}
              size={22}
              onPress={() => setSelected(new Set(habits.map((h) => h.id)))}
            />
            <IconButton
              icon="select-off"
              iconColor={BRAND_COLOR}
              size={22}
              onPress={() => setSelected(new Set())}
            />
            <Button
              mode="contained"
              onPress={deleteSelected}
              buttonColor={COLORS.red}
              textColor={COLORS.white}
              compact
            >
              削除
            </Button>
          </View>
        ) : (
          <IconButton
            icon="plus"
            iconColor={COLORS.white}
            size={24}
            containerColor={BRAND_COLOR}
            onPress={() => router.push('/habit/add')}
          />
        )}
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
  contextMenu: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
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
