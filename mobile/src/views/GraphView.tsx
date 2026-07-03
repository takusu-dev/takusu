// GraphView — task dependency DAG visualization
// Uses WebView + Cytoscape.js with dagre layout
// Shows transitive dependencies of incomplete tasks (completed nodes are gray)
// Edit mode: tap edge to cut, drag node-to-node to add dependency
// Non-edit mode: pan/zoom enabled

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { WebView, type WebViewMessageEvent } from 'react-native-webview';
import { Ionicons } from '@expo/vector-icons';
import { Button, IconButton } from 'react-native-paper';
import type { TakusuClient } from '@/src/api/client';
import { showError } from '@/src/api/errors';
import type { TaskRow } from '@/src/api/types';
import { parseDepends } from '@/src/api/types';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

interface GraphViewProps {
  client: TakusuClient | null;
  onBack: () => void;
  onTaskPress?: (taskId: string) => void;
}

// HTML content for the WebView with Cytoscape.js + dagre.
// Colors are applied at runtime via setTheme() to avoid reloading the
// WebView when the theme changes (which causes a white flash — Issue #37).
function buildGraphHtml(): string {
  return `
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no">
  <style>
    body { margin: 0; padding: 0; overflow: hidden; background: transparent; }
    #cy { width: 100vw; height: 100vh; }
  </style>
  <script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.30.4/cytoscape.min.js"></script>
  <script src="https://cdnjs.cloudflare.com/ajax/libs/dagre/0.8.5/dagre.min.js"></script>
  <script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape-dagre/2.5.0/cytoscape-dagre.min.js"></script>
</head>
<body>
  <div id="cy"></div>
  <script>
    cytoscape.use(cytoscapeDagre);

    let cy;
    let editMode = false;
    let currentBrand = '${BRAND_COLOR}';

    function applyTheme(brand) {
      currentBrand = brand;
      // Body background is transparent — the native WebView background (set
      // via React style) provides the themed color with no flash.
      if (cy) {
        cy.style()
          .selector('node')
          .style('border-color', brand)
          .update();
        cy.style()
          .selector('edge')
          .style('arrow-color', brand)
          .style('line-color', brand)
          .update();
      }
    }

    function initGraph(data) {
      if (cy) cy.destroy();
      cy = cytoscape({
        container: document.getElementById('cy'),
        elements: data.elements,
        style: [
          {
            selector: 'node',
            style: {
              'label': 'data(label)',
              'text-valign': 'center',
              'text-halign': 'center',
              'text-wrap': 'wrap',
              'text-max-width': '120px',
              'font-size': '12px',
              'background-color': 'data(color)',
              'width': 'data(width)',
              'height': 'data(height)',
              'border-width': 2,
              'border-color': currentBrand,
              'color': '#fff',
              'text-outline-width': 2,
              'text-outline-color': '#444',
            }
          },
          {
            selector: 'node[status="completed"], node[status="skipped"]',
            style: {
              'background-color': '#aaa',
              'border-color': '#888',
              'color': '#ddd',
            }
          },
          {
            selector: 'edge',
            style: {
              'curve-style': 'bezier',
              'target-arrow-shape': 'triangle',
              'arrow-color': currentBrand,
              'line-color': currentBrand,
              'width': 2,
            }
          },
          {
            selector: '.selected',
            style: {
              'border-width': 4,
              'border-color': '#E07070',
            }
          }
        ],
        layout: {
          name: 'dagre',
          rankDir: 'TB',
          nodeSep: 40,
          rankSep: 60,
          animate: true,
        },
        userZoomingEnabled: true,
        userPanningEnabled: true,
        boxSelectionEnabled: false,
      });

      cy.on('tap', 'node', function(evt) {
        const node = evt.target;
        const id = node.data('id');
        ReactNativeWebView.postMessage(JSON.stringify({ type: 'tapNode', id }));
      });

      cy.on('tap', 'edge', function(evt) {
        if (editMode) {
          const edge = evt.target;
          const source = edge.data('source');
          const target = edge.data('target');
          ReactNativeWebView.postMessage(JSON.stringify({ type: 'cutEdge', source, target }));
        }
      });

      // Edge drawing in edit mode
      let edgeSource = null;
      cy.on('tapstart', 'node', function(evt) {
        if (editMode) {
          edgeSource = evt.target.data('id');
        }
      });
      cy.on('tapend', function(evt) {
        if (editMode && edgeSource && evt.target !== cy) {
          const targetNode = evt.target;
          if (targetNode.isNode && targetNode.isNode() && targetNode.data('id') !== edgeSource) {
            const target = targetNode.data('id');
            ReactNativeWebView.postMessage(JSON.stringify({ type: 'addEdge', source: edgeSource, target }));
          }
        }
        edgeSource = null;
      });
    }

    function setEditMode(enabled) {
      editMode = enabled;
      if (cy) {
        cy.userZoomingEnabled(!enabled);
        cy.userPanningEnabled(!enabled);
      }
    }

    document.addEventListener('message', function(e) {
      const msg = JSON.parse(e.data);
      if (msg.type === 'init') initGraph(msg.data);
      if (msg.type === 'setEditMode') setEditMode(msg.enabled);
      if (msg.type === 'setTheme') applyTheme(msg.brand);
    });

    // Expose for ReactNativeWebView
    window.initGraph = initGraph;
    window.setEditMode = setEditMode;
    window.applyTheme = applyTheme;
  </script>
</body>
</html>
`;
}

interface GraphNode {
  id: string;
  label: string;
  status: string;
  color: string;
  width: number;
  height: number;
}

interface GraphEdge {
  source: string;
  target: string;
}

export function GraphView({ client, onBack, onTaskPress }: GraphViewProps) {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const webViewRef = useRef<WebView>(null);
  const [editMode, setEditMode] = useState(false);
  const [tasks, setTasks] = useState<TaskRow[]>([]);

  // Build the HTML once — it does not depend on theme colors so the WebView
  // is not reloaded when the theme changes (avoids the flash from Issue #37).
  // Colors are pushed in via applyTheme() instead.
  const graphHtml = useMemo(() => buildGraphHtml(), []);

  // Apply theme colors to the WebView whenever they change, without
  // reloading the page. Only the cytoscape edge/border colors need updating —
  // the background is handled by the native WebView style (transparent body).
  useEffect(() => {
    const brand = JSON.stringify(BRAND_COLOR);
    webViewRef.current?.injectJavaScript(
      `window.applyTheme(${brand}); true;`,
    );
  }, [colors.white]);

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
        status: task.status,
        color: isDone ? '#aaa' : BRAND_COLOR,
        width: Math.max(60, Math.min(120, task.title.length * 8)),
        height: 40,
      });
      const deps = parseDepends(task.depends);
      for (const depId of deps) {
        edges.push({ source: depId, target: task.id });
        visit(depId);
      }
    }

    for (const t of incomplete) visit(t.id);

    const elements = {
      nodes: nodes.map((n) => ({ data: n })),
      edges: edges.map((e, i) => ({
        data: { id: `e-${i}`, source: e.source, target: e.target },
      })),
    };

    webViewRef.current?.injectJavaScript(
      `window.initGraph(${JSON.stringify({ elements })});`,
    );
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  function onMessage(event: WebViewMessageEvent) {
    if (!client) return;
    const msg = JSON.parse(event.nativeEvent.data);

    if (msg.type === 'tapNode') {
      if (onTaskPress) {
        onTaskPress(msg.id);
      } else {
        onBack();
      }
    } else if (msg.type === 'cutEdge') {
      const targetTask = tasks.find((t) => t.id === msg.target);
      if (targetTask) {
        const deps = parseDepends(targetTask.depends).filter(
          (d) => d !== msg.source,
        );
        client
          .updateTask(msg.target, { depends: deps })
          .then(refresh)
          .catch((e) => showError(e, '依存関係の削除に失敗'));
      }
    } else if (msg.type === 'addEdge') {
      const targetTask = tasks.find((t) => t.id === msg.target);
      if (targetTask) {
        const deps = parseDepends(targetTask.depends);
        if (!deps.includes(msg.source)) {
          deps.push(msg.source);
          client
            .updateTask(msg.target, { depends: deps })
            .then(refresh)
            .catch((e) => showError(e, '依存関係の追加に失敗'));
        }
      }
    }
  }

  function toggleEditMode() {
    const newMode = !editMode;
    setEditMode(newMode);
    webViewRef.current?.injectJavaScript(
      `window.setEditMode(${newMode});`,
    );
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      <View style={[styles.topBar, { paddingTop: 4 + insets.top }]}>
        <View style={styles.topBarLeft}>
          <IconButton
            icon="chevron-left"
            iconColor={BRAND_COLOR}
            size={28}
            onPress={onBack}
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

      <WebView
        ref={webViewRef}
        source={{ html: graphHtml }}
        onMessage={onMessage}
        // Match the WebView's native background to the theme so there is no
        // white flash before the HTML body background is applied.
        style={[styles.webview, { backgroundColor: colors.white }]}
        originWhitelist={['*']}
        javaScriptEnabled
        domStorageEnabled
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
  webview: {
    flex: 1,
  },
});
