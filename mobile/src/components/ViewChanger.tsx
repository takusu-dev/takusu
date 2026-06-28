// ViewChanger — left side bottom, vertical buttons to switch between views
// habit / task / graph

import { Pressable, StyleSheet, Text, View } from 'react-native';
import { COLORS } from '@/src/theme';

export type ViewType = 'task' | 'graph' | 'habit';

interface ViewChangerProps {
  current: ViewType;
  onChange: (view: ViewType) => void;
}

const LABELS: Record<ViewType, string> = {
  task: 'タスク',
  graph: 'グラフ',
  habit: 'ハビット',
};

export function ViewChanger({ current, onChange }: ViewChangerProps) {
  const views: ViewType[] = ['task', 'graph', 'habit'];
  return (
    <View style={styles.container}>
      {views.map((v) => (
        <Pressable
          key={v}
          style={[styles.button, current === v && styles.buttonActive]}
          onPress={() => onChange(v)}
        >
          <Text
            style={[styles.buttonText, current === v && styles.buttonTextActive]}
          >
            {LABELS[v]}
          </Text>
        </Pressable>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    position: 'absolute',
    left: 8,
    bottom: 80,
    gap: 4,
    zIndex: 10,
  },
  button: {
    paddingHorizontal: 12,
    paddingVertical: 8,
    borderRadius: 12,
    backgroundColor: 'rgba(255,255,255,0.9)',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.2,
    shadowRadius: 2,
    elevation: 2,
  },
  buttonActive: {
    backgroundColor: COLORS.brand,
  },
  buttonText: {
    fontSize: 12,
    color: COLORS.brand,
  },
  buttonTextActive: {
    color: COLORS.white,
    fontWeight: '600',
  },
});
