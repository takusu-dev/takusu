// GraphView — task dependency DAG visualization
// Uses WebView + Cytoscape.js with dagre layout
// Shows transitive dependencies of incomplete tasks (completed nodes are gray)
// Edit mode: tap edge to cut, drag node-to-node to add dependency
// Non-edit mode: pan/zoom enabled

import { useCallback, useEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { WebView, type WebViewMessageEvent } from 'react-native-webview';
import type { TakusuClient } from '@/src/api/client';
import type { TaskRow } from '@/src/api/types';
import { parseDepends } from '@/src/api/types';
import { COLORS, BRAND_COLOR } from '@/src/theme';

interface GraphViewProps {
  client: TakusuClient | null;
  onBack: () => void;
}

// HTML content for the WebView with Cytoscape.js + dagre
const GRAPH_HTML = `
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no">
  <style>
    body { margin: 0; padding: 0; overflow: hidden; background: #fff; }
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
              'border-color': '#7261A3',
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
              'arrow-color': '#7261A3',
              'line-color': '#7261A3',
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
    });

    // Expose for ReactNativeWebView
    window.initGraph = initGraph;
    window.setEditMode = setEditMode;
  </script>
</body>
</html>
`;

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

export function GraphView({ client, onBack }: GraphViewProps) {
  const webViewRef = useRef<WebView>(null);
  const [editMode, setEditMode] = useState(false);
  const [tasks, setTasks] = useState<TaskRow[]>([]);

  const refresh = useCallback(async () => {
    if (!client) return;
    const allTasks = await client.listTasks();
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
      // Navigate to task detail — handled by parent
      onBack();
    } else if (msg.type === 'cutEdge') {
      // Remove dependency: target no longer depends on source
      const targetTask = tasks.find((t) => t.id === msg.target);
      if (targetTask) {
        const deps = parseDepends(targetTask.depends).filter(
          (d) => d !== msg.source,
        );
        client.updateTask(msg.target, { depends: deps }).then(refresh);
      }
    } else if (msg.type === 'addEdge') {
      // Add dependency: target depends on source
      const targetTask = tasks.find((t) => t.id === msg.target);
      if (targetTask) {
        const deps = parseDepends(targetTask.depends);
        if (!deps.includes(msg.source)) {
          deps.push(msg.source);
          client.updateTask(msg.target, { depends: deps }).then(refresh);
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
    <View style={styles.container}>
      <View style={styles.topBar}>
        <Pressable style={styles.backButton} onPress={onBack}>
          <Text style={styles.backButtonText}>‹</Text>
        </Pressable>
        <View style={{ flex: 1 }} />
        <Pressable
          style={[styles.editButton, editMode && styles.editButtonActive]}
          onPress={toggleEditMode}
        >
          <Text style={[styles.editButtonText, editMode && styles.editButtonTextActive]}>
            {editMode ? '編集中' : '編集'}
          </Text>
        </Pressable>
      </View>

      <WebView
        ref={webViewRef}
        source={{ html: GRAPH_HTML }}
        onMessage={onMessage}
        style={styles.webview}
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
  editButton: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: BRAND_COLOR,
  },
  editButtonActive: {
    backgroundColor: BRAND_COLOR,
  },
  editButtonText: {
    fontSize: 14,
    color: BRAND_COLOR,
  },
  editButtonTextActive: {
    color: COLORS.white,
  },
  webview: {
    flex: 1,
  },
});
