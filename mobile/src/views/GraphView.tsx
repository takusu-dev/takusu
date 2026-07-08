// GraphView — task dependency DAG visualization
// Uses @shopify/react-native-skia + d3-force via DependencyGraph component
// Shows transitive dependencies of incomplete tasks (completed nodes are gray)
// Edit mode: tap edge to cut, long-press-drag node-to-node to add dependency
// Non-edit mode: pan/zoom enabled

import { useCallback, useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { Button, IconButton } from 'react-native-paper';
import type { TakusuClient } from '@/src/api/client';
import { showError } from '@/src/api/errors';
import type { TaskRow, RedundantDependency } from '@/src/api/types';
import { parseDepends } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { haptic } from '@/src/components/haptics';
import {
  DependencyGraph,
  type GraphNode,
  type GraphEdge,
} from '@/src/components/graph/DependencyGraph';

interface GraphViewProps {
  client: TakusuClient | null;
  onBack: () => void;
  onTaskPress?: (taskId: string) => void;
}

export function GraphView({ client, onBack, onTaskPress }: GraphViewProps) {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const [editMode, setEditMode] = useState(false);
  const [tasks, setTasks] = useState<TaskRow[]>([]);
  const [graphNodes, setGraphNodes] = useState<GraphNode[]>([]);
  const [graphEdges, setGraphEdges] = useState<GraphEdge[]>([]);

  const refresh = useCallback(async () => {
    if (!client) return;
    let allTasks: TaskRow[];
    let redundant: RedundantDependency[];
    try {
      [allTasks, redundant] = await Promise.all([
        client.listTasks(),
        client
          .analyzeTaskDependencies()
          .then((r) => r.redundant)
          .catch(() => []),
      ]);
    } catch (e) {
      showError(e, 'タスク一覧の取得に失敗');
      return;
    }
    setTasks(allTasks);

    // Build a set of redundant edge keys for quick lookup (#387).
    // Rust API returns from=dependent, to=dependency, but GraphView edges
    // use source=dependency, target=dependent, so we flip the key direction.
    const redundantSet = new Set(redundant.map((r) => `${r.to}→${r.from}`));

    // Build transitive dependency graph from incomplete tasks
    const incomplete = allTasks.filter(
      (t) => t.status !== 'completed' && t.status !== 'skipped',
    );
    const taskMap = new Map(allTasks.map((t) => [t.id, t]));
    const visited = new Set<string>();
    const nodes: GraphNode[] = [];
    const edges: GraphEdge[] = [];

    function visit(id: string) {
      if (visited.has(id)) return;
      visited.add(id);
      const task = taskMap.get(id);
      if (!task) return;
      const isDone = task.status === 'completed' || task.status === 'skipped';
      nodes.push({
        id: task.id,
        label: task.title,
        color: isDone ? '#aaa' : BRAND_COLOR,
        x: 0,
        y: 0,
        vx: 0,
        vy: 0,
      });
      const deps = parseDepends(task.depends);
      for (const depId of deps) {
        const key = `${depId}→${task.id}`;
        edges.push({
          source: depId,
          target: task.id,
          redundant: redundantSet.has(key),
        });
        visit(depId);
      }
    }

    for (const t of incomplete) visit(t.id);

    setGraphNodes(nodes);
    setGraphEdges(edges);
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  function handleTapNode(taskId: string) {
    haptic.light();
    if (onTaskPress) {
      onTaskPress(taskId);
    } else {
      onBack();
    }
  }

  function handleCutEdge(source: string, target: string) {
    if (!client) return;
    haptic.medium();
    const targetTask = tasks.find((t) => t.id === target);
    if (targetTask) {
      const deps = parseDepends(targetTask.depends).filter((d) => d !== source);
      client
        .updateTask(target, { depends: deps })
        .then(refresh)
        .catch((e) => showError(e, '依存関係の削除に失敗'));
    }
  }

  function handleAddEdge(source: string, target: string) {
    if (!client) return;
    haptic.medium();
    const targetTask = tasks.find((t) => t.id === target);
    if (targetTask) {
      const deps = parseDepends(targetTask.depends);
      if (!deps.includes(source)) {
        deps.push(source);
        client
          .updateTask(target, { depends: deps })
          .then(refresh)
          .catch((e) => showError(e, '依存関係の追加に失敗'));
      }
    }
  }

  function toggleEditMode() {
    haptic.medium();
    setEditMode((v) => !v);
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View style={[styles.topBar, { paddingTop: 4 + insets.top }]}>
        <View style={styles.topBarLeft}>
          <IconButton
            icon="chevron-left"
            iconColor={BRAND_COLOR}
            size={28}
            onPress={() => {
              haptic.light();
              onBack();
            }}
          />
        </View>
        <View style={styles.topBarCenter}>
          <Button
            mode={editMode ? 'contained' : 'outlined'}
            onPress={toggleEditMode}
            textColor={editMode ? COLORS.white : BRAND_COLOR}
            buttonColor={editMode ? BRAND_COLOR : undefined}
            style={styles.editButton}
            labelStyle={styles.editButtonLabel}
            contentStyle={styles.editButtonContent}
          >
            {editMode ? '編集中' : '編集'}
          </Button>
        </View>
        <View style={styles.topBarRight} />
      </View>

      <DependencyGraph
        nodes={graphNodes}
        edges={graphEdges}
        editMode={editMode}
        onTapNode={handleTapNode}
        onCutEdge={handleCutEdge}
        onAddEdge={handleAddEdge}
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
    paddingBottom: 4,
  },
  topBarLeft: {
    width: 48,
    alignItems: 'flex-start',
  },
  topBarCenter: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  topBarRight: {
    width: 48,
  },
  editButton: {
    borderRadius: 4,
    minWidth: 96,
  },
  editButtonLabel: {
    fontSize: 18,
    fontWeight: '600',
  },
  editButtonContent: {
    paddingVertical: 6,
    paddingHorizontal: 8,
  },
});
