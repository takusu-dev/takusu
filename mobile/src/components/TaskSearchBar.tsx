// Query-aware task search bar with server-driven completion and token chips.
// Mirrors the design in doc/mock/task-search-ui-mock.html.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  FlatList,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { useColors } from '@/src/theme';
import type { Completion } from '@/src/api/types';
import { TakusuClient } from '@/src/api/client';

interface Token {
  raw: string;
  type: 'op' | 'paren' | 'term';
  start: number;
  end: number;
}

const DEFAULT_QUALIFIERS = [
  'status',
  'title',
  'desc',
  'description',
  'start',
  'end',
  'scheduled-start',
  'scheduled-end',
  'from',
  'until',
  'habit',
  'is',
  'has',
];

const DEFAULT_COMPLETIONS: Completion[] = DEFAULT_QUALIFIERS.map((q) => ({
  label: `${q}:`,
  value: `${q}:`,
}));

function isOp(raw: string): boolean {
  const upper = raw.toUpperCase();
  return upper === 'OR' || upper === 'AND' || upper === 'NOT';
}

function tokenize(query: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < query.length) {
    const c = query[i];
    if (/\s/.test(c)) {
      i++;
      continue;
    }
    if (c === '(' || c === ')') {
      tokens.push({ type: 'paren', raw: c, start: i, end: i + 1 });
      i++;
      continue;
    }
    if (c === '"') {
      const start = i;
      i++;
      while (i < query.length && query[i] !== '"') {
        i++;
      }
      if (i < query.length) {
        i++;
      }
      tokens.push({ type: 'term', raw: query.slice(start, i), start, end: i });
      continue;
    }
    const start = i;
    while (
      i < query.length &&
      !/\s/.test(query[i]) &&
      query[i] !== '(' &&
      query[i] !== ')'
    ) {
      i++;
    }
    const raw = query.slice(start, i);
    if (!raw) continue;

    if (raw === '-') {
      tokens.push({ type: 'op', raw, start, end: i });
      continue;
    }

    if (raw.startsWith('-') && raw.length > 1) {
      tokens.push({ type: 'op', raw: '-', start, end: start + 1 });
      const rest = raw.slice(1);
      tokens.push({
        type: isOp(rest) ? 'op' : 'term',
        raw: rest,
        start: start + 1,
        end: i,
      });
      continue;
    }

    tokens.push({
      type: isOp(raw) ? 'op' : 'term',
      raw,
      start,
      end: i,
    });
  }
  return tokens;
}

function removeTokenAt(query: string, start: number, end: number): string {
  const before = query.slice(0, start).trimEnd();
  const after = query.slice(end).trimStart();
  if (before.length === 0) return after;
  if (after.length === 0) return before;
  return `${before} ${after}`;
}

interface TaskSearchBarProps {
  value: string;
  onChangeText: (text: string) => void;
  client: TakusuClient | null;
}

export function TaskSearchBar({
  value,
  onChangeText,
  client,
}: TaskSearchBarProps) {
  const colors = useColors();
  const [completions, setCompletions] = useState<Completion[]>([]);
  const [focused, setFocused] = useState(false);
  const blurTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clear pending timers when the component unmounts.
  useEffect(() => {
    return () => {
      if (blurTimeoutRef.current) {
        clearTimeout(blurTimeoutRef.current);
      }
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }
    if (!client || value.trim().length === 0) {
      setCompletions(value.trim().length === 0 ? DEFAULT_COMPLETIONS : []);
      return;
    }
    debounceRef.current = setTimeout(() => {
      client
        .completeTaskQuery(value)
        .then(setCompletions)
        .catch(() => setCompletions([]));
    }, 150);
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [value, client]);

  const chips = useMemo(() => tokenize(value), [value]);

  const handleSelect = useCallback(
    (item: Completion) => {
      onChangeText(item.value);
      if (blurTimeoutRef.current) {
        clearTimeout(blurTimeoutRef.current);
      }
      setFocused(true);
    },
    [onChangeText],
  );

  const handleRemove = useCallback(
    (start: number, end: number) => {
      onChangeText(removeTokenAt(value, start, end));
    },
    [onChangeText, value],
  );

  const handleClear = useCallback(() => {
    onChangeText('');
  }, [onChangeText]);

  return (
    <View style={styles.container}>
      <View
        style={[
          styles.inputRow,
          {
            borderColor: colors.separator,
            backgroundColor: colors.surface,
          },
        ]}
      >
        <TextInput
          style={[styles.input, { color: colors.black }]}
          value={value}
          onChangeText={onChangeText}
          onFocus={() => {
            if (blurTimeoutRef.current) {
              clearTimeout(blurTimeoutRef.current);
            }
            setFocused(true);
          }}
          onBlur={() => {
            blurTimeoutRef.current = setTimeout(() => setFocused(false), 150);
          }}
          placeholder="検索..."
          placeholderTextColor={colors.grayLight}
          autoCapitalize="none"
          autoCorrect={false}
        />
        {value.length > 0 && (
          <Pressable onPress={handleClear} style={styles.clear}>
            <Ionicons name="close-circle" size={18} color={colors.grayLight} />
          </Pressable>
        )}
      </View>
      {chips.length > 0 && (
        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          style={styles.chipsRow}
          contentContainerStyle={styles.chipsContent}
        >
          {chips.map((t, index) => (
            <Pressable
              key={`${t.type}-${t.start}-${index}`}
              onPress={() => handleRemove(t.start, t.end)}
              style={[
                styles.chip,
                {
                  backgroundColor: colors.surfaceTint,
                  borderColor: colors.separator,
                },
              ]}
            >
              <Text style={[styles.chipText, { color: colors.black }]}>
                {t.raw}
              </Text>
              <Ionicons name="close" size={12} color={colors.grayLight} />
            </Pressable>
          ))}
        </ScrollView>
      )}
      {focused && completions.length > 0 && (
        <View
          style={[
            styles.dropdown,
            {
              backgroundColor: colors.surface,
              borderColor: colors.separator,
            },
          ]}
        >
          <FlatList
            data={completions}
            keyboardShouldPersistTaps="handled"
            keyExtractor={(item, index) => `${item.value}-${index}`}
            renderItem={({ item }) => (
              <Pressable
                onPress={() => handleSelect(item)}
                style={styles.completionItem}
              >
                <Text style={[styles.completionText, { color: colors.black }]}>
                  {item.label}
                </Text>
              </Pressable>
            )}
            ItemSeparatorComponent={() => (
              <View
                style={[
                  styles.separator,
                  { backgroundColor: colors.separator },
                ]}
              />
            )}
          />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    position: 'relative',
  },
  inputRow: {
    flexDirection: 'row',
    alignItems: 'center',
    height: 40,
    borderRadius: 12,
    borderWidth: 1,
    paddingHorizontal: 12,
  },
  input: {
    flex: 1,
    fontSize: 15,
    paddingVertical: 0,
  },
  clear: {
    padding: 4,
  },
  chipsRow: {
    marginTop: 6,
    maxHeight: 34,
  },
  chipsContent: {
    gap: 6,
    paddingRight: 8,
  },
  chip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
    borderWidth: 1,
  },
  chipText: {
    fontSize: 12,
  },
  dropdown: {
    position: 'absolute',
    top: 46,
    left: 0,
    right: 0,
    maxHeight: 200,
    borderRadius: 12,
    borderWidth: 1,
    zIndex: 100,
    elevation: 5,
    overflow: 'hidden',
  },
  completionItem: {
    paddingHorizontal: 12,
    paddingVertical: 10,
  },
  completionText: {
    fontSize: 14,
  },
  separator: {
    height: 1,
  },
});
