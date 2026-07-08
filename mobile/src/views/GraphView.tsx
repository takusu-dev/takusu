// GraphView — task dependency DAG visualization
// Uses @shopify/react-native-skia + d3-force via DependencyGraph component
// Shows transitive dependencies of incomplete tasks (completed nodes are gray)
// Edit mode: long-press-drag node-to-node to add dependency,
//            long-press-drag on empty space to cut crossing edges (#382)
// Non-edit mode: pan/zoom enabled, node drag enabled (#383)

import { useCallback, useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { Button, IconButton } from 'react-native-paper';
import { useFocusEffect } from 'expo-router';
import type { TakusuClient } from '@/src/api/client';
import { showError } from '@/src/api/errors';
import type { TaskRow } from '@/src/api/types';
import { parseDepends } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { haptic } from '@/src/components/haptics';
import {
  DependencyGraph,
  type GraphNode,
  type GraphEdge,
} from '@/src/components/graph/DependencyGraph';

// GraphView uses a larger font size than the embedded TaskDetailView graph (#379)
const GRAPHVIEW_FONT_SIZE = 18;

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
  const [redundantEdgeKeys, setRedundantEdgeKeys] = useState<Set<string>>(
    new Set(),
  );

  const refresh = useCallback(async () => {
    if (!client) return;
    let allTasks: TaskRow[];
    try {
      allTasks = await client.listTasks();
    } catch (e) {
      showError(e, 'タスク一覧の取得に失敗');
      return;
    }
    setTasks(allTasks);

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
        edges.push({ source: depId, target: task.id });
        visit(depId);
      }
    }

    for (const t of incomplete) visit(t.id);

    setGraphNodes(nodes);
    setGraphEdges(edges);

    // Fetch redundant dependency analysis (#387)
    try {
      const analysis = await client.analyzeTaskDependencies();
      const redundantKeys = new Set<string>();
      for (const r of analysis.redundant) {
        redundantKeys.add(`${r.from}→${r.to}`);
      }
      setRedundantEdgeKeys(redundantKeys);
    } catch {
      // Non-critical — graph still works without redundant highlighting
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Refresh on focus (#386): when returning from TaskDetailView after
  // editing edges, GraphView needs to re-fetch to show the latest state.
  useFocusEffect(
    useCallback(() => {
      refresh();
    }, [refresh]),
  );

  function handleTapNode(taskId: string) {
    haptic.light();
    if (onTaskPress) {
      onTaskPress(taskId);
    } else {
      onBack();
    }
  }

  function handleCutEdges(edges: { source: string; target: string }[]) {
    if (!client || edges.length === 0) return;
    haptic.medium();
    // Group by target task to batch updates
    const byTarget = new Map<string, string[]>();
    for (const { source, target } of edges) {
      const arr = byTarget.get(target) ?? [];
      arr.push(source);
      byTarget.set(target, arr);
    }
    const updates: Promise<unknown>[] = [];
    for (const [target, sources] of byTarget) {
      const targetTask = tasks.find((t) => t.id === target);
      if (targetTask) {
        const deps = parseDepends(targetTask.depends).filter(
          (d) => !sources.includes(d),
        );
        updates.push(client.updateTask(target, { depends: deps }));
      }
    }
    Promise.all(updates)
      .then(refresh)
      .catch((e) => showError(e, '依存関係の削除に失敗'));
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
        fontSize={GRAPHVIEW_FONT_SIZE}
        redundantEdges={redundantEdgeKeys}
        onTapNode={handleTapNode}
        onCutEdges={handleCutEdges}
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
