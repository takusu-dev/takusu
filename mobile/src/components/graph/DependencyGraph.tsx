// DependencyGraph — force-directed DAG visualization
// Uses @shopify/react-native-skia + d3-force
// Shared by GraphView (full-screen, editable) and TaskDetailView (embedded, read-only)

import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Platform,
  type LayoutChangeEvent,
  StyleSheet,
  View,
} from 'react-native';
import {
  Canvas,
  Circle,
  DashPathEffect,
  Group,
  Paragraph,
  Path,
  Skia,
  TextAlign,
  useCanvasRef,
} from '@shopify/react-native-skia';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';
import Reanimated, {
  runOnJS,
  useSharedValue,
  useDerivedValue,
} from 'react-native-reanimated';
import * as d3 from 'd3-force';
import { COLORS, BRAND_COLOR, useColors } from '@/src/theme';

// ── Types ──

export interface GraphNode {
  id: string;
  label: string;
  color: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  /** Redundant edge (direct dep already implied by a longer path) — #387 */
  redundant?: boolean;
}

export interface DependencyGraphProps {
  /** Nodes grouped by id → node data */
  nodes: GraphNode[];
  edges: GraphEdge[];
  /** Highlight a specific task (e.g. the one being viewed in detail) */
  highlightTaskId?: string;
  /** Enable edge addition/removal (GraphView only) */
  editMode?: boolean;
  /** Font size for node labels (#379: GraphView uses larger text) */
  fontSize?: number;
  onTapNode?: (taskId: string) => void;
  /** Cut multiple edges at once — used by line-cut (#382) */
  onCutEdges?: (edges: { source: string; target: string }[]) => void;
  onAddEdge?: (sourceId: string, targetId: string) => void;
  /** Fixed height for embedded use (TaskDetailView); flex:1 when omitted */
  height?: number;
}

// ── Constants ──

const NODE_RADIUS = 28;
const DEFAULT_FONT_SIZE = 15;
const MAX_LABEL_CHARS = 40;
const LABEL_WIDTH = 140;
const LABEL_OFFSET = NODE_RADIUS + 6;
const HIT_RADIUS = NODE_RADIUS + 4;
const LABEL_PAD_X = 6;
const LABEL_PAD_Y = 3;
/** Redundant edges (direct dep already implied by a transitive path) — #387 */
const REDUNDANT_EDGE_COLOR = '#e85d04';

// ── Helpers ──

/** Check if two line segments intersect (#382: cut line vs edges) */
function segmentsIntersect(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  x3: number,
  y3: number,
  x4: number,
  y4: number,
): boolean {
  'worklet';
  const d1x = x2 - x1,
    d1y = y2 - y1;
  const d2x = x4 - x3,
    d2y = y4 - y3;
  const denom = d1x * d2y - d1y * d2x;
  if (Math.abs(denom) < 1e-10) return false; // parallel
  const t = ((x3 - x1) * d2y - (y3 - y1) * d2x) / denom;
  const u = ((x3 - x1) * d1y - (y3 - y1) * d1x) / denom;
  return t >= 0 && t <= 1 && u >= 0 && u <= 1;
}

/** Compute arrowhead triangle points for edge from (ax,ay) to (bx,by) */
function arrowHead(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  size: number,
): string {
  const dx = bx - ax;
  const dy = by - ay;
  const len = Math.hypot(dx, dy);
  if (len === 0) return '';
  const ux = dx / len;
  const uy = dy / len;
  const tipX = bx - ux * NODE_RADIUS;
  const tipY = by - uy * NODE_RADIUS;
  const leftX = tipX - ux * size + uy * (size * 0.5);
  const leftY = tipY - uy * size - ux * (size * 0.5);
  const rightX = tipX - ux * size - uy * (size * 0.5);
  const rightY = tipY - uy * size + ux * (size * 0.5);
  return `M ${tipX} ${tipY} L ${leftX} ${leftY} L ${rightX} ${rightY} Z`;
}

/** Hit test that treats the node circle and its label as a single target (#422). */
function hitTestNode(
  node: GraphNode,
  x: number,
  y: number,
  fontSize: number,
  labelHeights: Map<string, number>,
): boolean {
  const dx = x - node.x;
  const dy = y - node.y;
  if (Math.hypot(dx, dy) < HIT_RADIUS) return true;
  const halfW = LABEL_WIDTH / 2 + LABEL_PAD_X;
  const top = node.y + LABEL_OFFSET - LABEL_PAD_Y;
  const labelHeight = labelHeights.get(node.id) ?? fontSize + LABEL_PAD_Y * 2;
  const bottom = top + labelHeight;
  return x >= node.x - halfW && x <= node.x + halfW && y >= top && y <= bottom;
}

// ── Component ──

export function DependencyGraph({
  nodes: inputNodes,
  edges: inputEdges,
  highlightTaskId,
  editMode = false,
  fontSize = DEFAULT_FONT_SIZE,
  onTapNode,
  onCutEdges,
  onAddEdge,
  height,
}: DependencyGraphProps) {
  const colors = useColors();
  const canvasRef = useCanvasRef();
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });
  const [simNodes, setSimNodes] = useState<GraphNode[]>(inputNodes);

  // Pan/zoom transforms — applied to the Skia Group, not the outer View
  const translateX = useSharedValue(0);
  const translateY = useSharedValue(0);
  const scale = useSharedValue(1);

  // Group transform derived from shared values
  const groupTransform = useDerivedValue(() => [
    { translateX: translateX.value },
    { translateY: translateY.value },
    { scale: scale.value },
  ]);

  // Drag state for edit mode edge addition.
  // #219: use Reanimated shared values so the drag line updates smoothly
  // on the UI thread without waiting for React re-renders.
  // #294: dragSourceId must be a SharedValue, not useRef — onStart/onUpdate/
  // onEnd are separate worklets with separate closure copies, so a useRef
  // mutation in onStart is invisible to onUpdate/onEnd. SharedValues are
  // accessible from all worklets on the UI thread.
  const dragSourceId = useSharedValue<string | null>(null);
  const dragActive = useSharedValue(0);
  const dragSx = useSharedValue(0);
  const dragSy = useSharedValue(0);
  const dragEx = useSharedValue(0);
  const dragEy = useSharedValue(0);

  // Cut line state (#382): long-press on empty space → drag → cut crossing edges
  const cutActive = useSharedValue(0);
  const cutSx = useSharedValue(0);
  const cutSy = useSharedValue(0);
  const cutEx = useSharedValue(0);
  const cutEy = useSharedValue(0);

  // Node drag state (#383): pan on node → drag node
  const draggingNodeId = useSharedValue<string | null>(null);

  // Crossing edges during cut line drag — React state for rendering
  const [crossingEdges, setCrossingEdges] = useState<Set<string>>(new Set());

  // Content-derived key: triggers simulation restart when node/edge identity
  // changes even if the counts stay the same (e.g., after editing deps).
  const graphKey = useMemo(() => {
    const nodeIds = inputNodes
      .map((n) => n.id)
      .sort()
      .join(',');
    const edgeKeys = inputEdges
      .map((e) => `${e.source}→${e.target}`)
      .sort()
      .join(',');
    return `${nodeIds}|${edgeKeys}`;
  }, [inputNodes, inputEdges]);

  // ── Force simulation ──

  useEffect(() => {
    if (
      inputNodes.length === 0 ||
      canvasSize.width === 0 ||
      canvasSize.height === 0
    ) {
      setSimNodes([]);
      return;
    }

    // Build links from edges
    const nodeMap = new Map(inputNodes.map((n) => [n.id, n]));
    const links = inputEdges
      .filter((e) => nodeMap.has(e.source) && nodeMap.has(e.target))
      .map((e) => ({ source: e.source, target: e.target }));

    // Clone nodes for simulation (d3 mutates positions)
    const simNodesLocal = inputNodes.map((n) => ({
      ...n,
      x: n.x !== 0 ? n.x : Math.random() * 200,
      y: n.y !== 0 ? n.y : Math.random() * 200,
    }));

    const sim = d3
      .forceSimulation<GraphNode>(simNodesLocal)
      .alphaDecay(0.02)
      .alphaMin(0.001)
      .velocityDecay(0.3)
      .force(
        'link',
        d3
          .forceLink<GraphNode, { source: string; target: string }>(links)
          .id((d) => d.id)
          .distance(160),
      )
      .force('charge', d3.forceManyBody().strength(-150))
      .force(
        'center',
        d3.forceCenter(canvasSize.width / 2, canvasSize.height / 2),
      )
      .force('collide', d3.forceCollide(NODE_RADIUS + 8))
      .stop(); // Stop d3's internal timer — we drive ticks manually below

    // Run all ticks synchronously in a single batch.
    // Previously this used setInterval at 16ms for 300 iterations (5s of
    // rapid re-renders), which caused Skia to crash when the user tapped
    // during the animation. Running all ticks at once produces the final
    // layout in one pass — one setSimNodes call, no animation, no crash.
    const maxTicks = 300;
    for (let i = 0; i < maxTicks; i++) {
      sim.tick();
      if (sim.alpha() < sim.alphaMin()) break;
    }
    sim.stop();

    const finalNodes = simNodesLocal.map((n) => ({ ...n, x: n.x, y: n.y }));
    setSimNodes(finalNodes);

    // ── Auto-fit: zoom/translate so all nodes are visible (#218, #384) ──
    // Apply for both full-screen GraphView and embedded (TaskDetailView).
    // Skip in edit mode so the user's pan/zoom is preserved across edge
    // additions/removals (which trigger refresh → graphKey change → re-run).
    if (finalNodes.length > 0 && !editMode) {
      const xs = finalNodes.map((n) => n.x);
      const ys = finalNodes.map((n) => n.y);
      // Account for label height below nodes in the bounding box
      const minX = Math.min(...xs) - NODE_RADIUS - 4;
      const maxX = Math.max(...xs) + NODE_RADIUS + 4;
      const minY = Math.min(...ys) - NODE_RADIUS - 4;
      const maxY = Math.max(...ys) + NODE_RADIUS + LABEL_OFFSET + 24;
      const graphW = maxX - minX;
      const graphH = maxY - minY;
      const cw = canvasSize.width;
      const ch = canvasSize.height;
      if (graphW > 0 && graphH > 0) {
        const padding = height ? 16 : 40;
        const fitScale = Math.min(
          (cw - padding * 2) / graphW,
          (ch - padding * 2) / graphH,
          1, // don't zoom in beyond 1x
        );
        const cx = (minX + maxX) / 2;
        const cy = (minY + maxY) / 2;
        scale.value = fitScale;
        translateX.value = cw / 2 - cx * fitScale;
        translateY.value = ch / 2 - cy * fitScale;
      }
    }

    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graphKey, canvasSize.width, canvasSize.height]);

  // ── Coordinate transform (screen → world) ──
  // The Canvas fills the parent at its natural size.  The Group inside the
  // Canvas applies pan/zoom.  Gesture coordinates arrive in screen space
  // (relative to the GestureDetector view), so we must undo the Group's
  // transform to get world coordinates.

  const toWorld = useCallback(
    (sx: number, sy: number) => {
      'worklet';
      return {
        x: (sx - translateX.value) / scale.value,
        y: (sy - translateY.value) / scale.value,
      };
    },
    [translateX, translateY, scale],
  );

  // ── Node position update (#383) ──
  const commitNodePosition = useCallback((id: string, x: number, y: number) => {
    setSimNodes((prev) => prev.map((n) => (n.id === id ? { ...n, x, y } : n)));
  }, []);

  const updateCrossingEdges = useCallback(
    (sx: number, sy: number, ex: number, ey: number) => {
      const nodeMap = new Map(simNodes.map((n) => [n.id, n]));
      const crossing = new Set<string>();
      for (const edge of inputEdges) {
        const s = nodeMap.get(edge.source);
        const t = nodeMap.get(edge.target);
        if (!s || !t) continue;
        if (segmentsIntersect(sx, sy, ex, ey, s.x, s.y, t.x, t.y)) {
          crossing.add(`${edge.source}→${edge.target}`);
        }
      }
      setCrossingEdges(crossing);
    },
    [simNodes, inputEdges],
  );

  // Collect crossing edges for cut line — called via runOnJS from onEnd (#382)
  const collectAndCutCrossingEdges = useCallback(
    (sx: number, sy: number, ex: number, ey: number) => {
      if (!onCutEdges) {
        setCrossingEdges(new Set());
        return;
      }
      const nodeMap = new Map(simNodes.map((n) => [n.id, n]));
      const toCut: { source: string; target: string }[] = [];
      for (const edge of inputEdges) {
        const s = nodeMap.get(edge.source);
        const t = nodeMap.get(edge.target);
        if (!s || !t) continue;
        if (segmentsIntersect(sx, sy, ex, ey, s.x, s.y, t.x, t.y)) {
          toCut.push({ source: edge.source, target: edge.target });
        }
      }
      if (toCut.length > 0) onCutEdges(toCut);
      setCrossingEdges(new Set());
    },
    [simNodes, inputEdges, onCutEdges],
  );

  // Clear crossing edges — wrapper so the Set is created on the JS thread,
  // not passed through the worklet bridge (which may not preserve Set) (#382)
  const clearCrossingEdges = useCallback(() => setCrossingEdges(new Set()), []);

  // ── Gesture: Pan (node drag + canvas pan) (#383) ──
  // When pan starts on a node, drag the node. Otherwise, pan the canvas
  // (non-edit, non-embedded only).

  const panGesture = Gesture.Pan()
    .enabled(!height)
    .onStart((e) => {
      const world = toWorld(e.x, e.y);
      // Check if touching a node (or its label) → start node drag (#383, #422)
      for (const node of simNodes) {
        if (hitTestNode(node, world.x, world.y, fontSize, labelHeights)) {
          draggingNodeId.value = node.id;
          return;
        }
      }
      draggingNodeId.value = null;
    })
    .onChange((e) => {
      if (draggingNodeId.value) {
        // Drag node — update position via runOnJS for rendering
        const world = toWorld(e.x, e.y);
        runOnJS(commitNodePosition)(draggingNodeId.value, world.x, world.y);
      } else if (!editMode) {
        translateX.value = translateX.value + e.changeX;
        translateY.value = translateY.value + e.changeY;
      }
    })
    .onEnd(() => {
      if (draggingNodeId.value) {
        draggingNodeId.value = null;
      }
    });

  // ── Gesture: Pinch ──

  const pinchGesture = Gesture.Pinch()
    .enabled(!editMode && !height)
    .onChange((e) => {
      scale.value = Math.max(0.3, Math.min(3, scale.value * e.scaleChange));
    });

  // ── Gesture: Tap (node tap only — edge cutting moved to line-cut #382) ──

  const tapGesture = Gesture.Tap().onEnd((e) => {
    const world = toWorld(e.x, e.y);

    // Check node hits (including the label area) (#422)
    for (const node of simNodes) {
      if (hitTestNode(node, world.x, world.y, fontSize, labelHeights)) {
        if (onTapNode) runOnJS(onTapNode)(node.id);
        return;
      }
    }
  });

  // ── Gesture: Long-press → edge drag or cut line (edit mode) ──
  // Long-press on a node → drag to another node → add edge (existing)
  // Long-press on empty space → drag → draw cut line → cut crossing edges (#382)

  const longPressDrag = Gesture.Pan()
    .activateAfterLongPress(150)
    .onStart((e) => {
      if (!editMode) return;
      const world = toWorld(e.x, e.y);
      // Check if starting on a node (or its label) → edge addition mode
      for (const node of simNodes) {
        if (hitTestNode(node, world.x, world.y, fontSize, labelHeights)) {
          if (onAddEdge) {
            dragSourceId.value = node.id;
            dragSx.value = node.x;
            dragSy.value = node.y;
            dragEx.value = node.x;
            dragEy.value = node.y;
            dragActive.value = 1;
          }
          return;
        }
      }
      // Not on a node → cut line mode (#382)
      if (onCutEdges) {
        cutSx.value = world.x;
        cutSy.value = world.y;
        cutEx.value = world.x;
        cutEy.value = world.y;
        cutActive.value = 1;
      }
    })
    .onUpdate((e) => {
      if (!editMode) return;
      const world = toWorld(e.x, e.y);
      if (dragSourceId.value) {
        // Edge addition drag
        dragEx.value = world.x;
        dragEy.value = world.y;
      } else if (cutActive.value === 1) {
        // Cut line drag (#382)
        cutEx.value = world.x;
        cutEy.value = world.y;
        runOnJS(updateCrossingEdges)(
          cutSx.value,
          cutSy.value,
          world.x,
          world.y,
        );
      }
    })
    .onEnd((e) => {
      if (!editMode) return;
      if (dragSourceId.value && onAddEdge) {
        // Edge addition — check if dropped on a node (including its label)
        const world = toWorld(e.x, e.y);
        for (const node of simNodes) {
          if (node.id === dragSourceId.value) continue;
          if (hitTestNode(node, world.x, world.y, fontSize, labelHeights)) {
            runOnJS(onAddEdge)(dragSourceId.value, node.id);
            break;
          }
        }
      } else if (cutActive.value === 1) {
        // Cut line — collect crossing edges and cut them via runOnJS (#382)
        runOnJS(collectAndCutCrossingEdges)(
          cutSx.value,
          cutSy.value,
          cutEx.value,
          cutEy.value,
        );
      }
    })
    .onFinalize(() => {
      // Reset all drag/cut state on both end and cancel (#382)
      dragSourceId.value = null;
      dragActive.value = 0;
      cutActive.value = 0;
      runOnJS(clearCrossingEdges)();
    });

  const composed = Gesture.Simultaneous(
    pinchGesture,
    Gesture.Exclusive(
      longPressDrag,
      Gesture.Simultaneous(panGesture, tapGesture),
    ),
  );

  // Animated style for the outer View is no longer needed for pan/zoom —
  // the Group inside Canvas handles transforms.  But we still need a
  // Reanimated.View as the gesture target (GestureDetector requires one).

  // ── Edge path strings ──

  const nodeMap = useMemo(
    () => new Map(simNodes.map((n) => [n.id, n])),
    [simNodes],
  );

  // Map of input node visual properties (color, label) — these may change
  // without triggering a re-simulation (e.g. status change → color change).
  // simNodes is used only for x/y positions; visual props come from here.
  const inputNodeMap = useMemo(
    () => new Map(inputNodes.map((n) => [n.id, n])),
    [inputNodes],
  );

  // Exact label hit heights for the node's label pill (#422).
  const labelHeights = useMemo(() => {
    const map = new Map<string, number>();
    for (const node of inputNodes) {
      const text = truncate(node.label, MAX_LABEL_CHARS);
      map.set(node.id, getLabelHeight(text, fontSize) + LABEL_PAD_Y * 2);
    }
    return map;
  }, [inputNodes, fontSize]);

  const edgePaths = useMemo(() => {
    const paths: {
      source: string;
      target: string;
      d: string;
      redundant: boolean;
      key: string;
    }[] = [];
    for (const edge of inputEdges) {
      const s = nodeMap.get(edge.source);
      const t = nodeMap.get(edge.target);
      if (!s || !t) continue;
      const key = `${edge.source}→${edge.target}`;
      paths.push({
        source: edge.source,
        target: edge.target,
        d: `M ${s.x} ${s.y} L ${t.x} ${t.y}`,
        redundant: !!edge.redundant,
        key,
      });
    }
    return paths;
  }, [inputEdges, nodeMap]);

  const arrowPaths = useMemo(() => {
    const paths: { d: string; redundant: boolean; key: string }[] = [];
    for (const edge of inputEdges) {
      const s = nodeMap.get(edge.source);
      const t = nodeMap.get(edge.target);
      if (!s || !t) continue;
      const ah = arrowHead(s.x, s.y, t.x, t.y, 10);
      if (ah)
        paths.push({
          d: ah,
          redundant: !!edge.redundant,
          key: `${edge.source}→${edge.target}`,
        });
    }
    return paths;
  }, [inputEdges, nodeMap]);

  // Drag line path — derived from shared values for smooth UI-thread updates (#219)
  const dragPath = useDerivedValue(() => {
    if (dragActive.value === 0) return '';
    return `M ${dragSx.value} ${dragSy.value} L ${dragEx.value} ${dragEy.value}`;
  });

  // Cut line path (#382) — dashed line for edge cutting
  const cutPath = useDerivedValue(() => {
    if (cutActive.value === 0) return '';
    return `M ${cutSx.value} ${cutSy.value} L ${cutEx.value} ${cutEy.value}`;
  });

  // ── Canvas size tracking ──

  const handleLayout = useCallback((e: LayoutChangeEvent) => {
    const { width, height: h } = e.nativeEvent.layout;
    setCanvasSize({ width, height: h });
  }, []);

  // ── Render ──

  if (inputNodes.length === 0) {
    return (
      <View
        style={[styles.empty, { backgroundColor: colors.white }]}
        onLayout={handleLayout}
      >
        <Reanimated.Text style={{ color: colors.gray }}>
          依存関係がありません
        </Reanimated.Text>
      </View>
    );
  }

  // Edge color helper: crossing cut line (#382) → red, redundant (#387) → orange,
  // normal → gray
  function edgeColor(key: string, redundant: boolean): string {
    if (crossingEdges.has(key)) return COLORS.red;
    if (redundant) return REDUNDANT_EDGE_COLOR;
    return colors.grayLight ?? '#aaa';
  }

  return (
    <GestureDetector gesture={composed}>
      <Reanimated.View
        style={[
          height ? { height } : styles.flex,
          { backgroundColor: colors.white },
        ]}
        onLayout={handleLayout}
      >
        <Canvas ref={canvasRef} style={height ? { height } : styles.flex}>
          {/* Pan/zoom applied to the Skia Group so the Canvas background
              stays fixed and the parent container doesn't clip. */}
          <Group transform={groupTransform}>
            {/* Edges — redundant edges drawn in a warning color (#387) */}
            {edgePaths.map((ep) => (
              <Path
                key={`e-${ep.key}`}
                path={ep.d}
                color={edgeColor(ep.key, ep.redundant)}
                style="stroke"
                strokeWidth={
                  crossingEdges.has(ep.key) ? 4 : ep.redundant ? 3 : 2
                }
              />
            ))}

            {/* Arrowheads */}
            {arrowPaths.map((ap) => (
              <Path
                key={`a-${ap.key}`}
                path={ap.d}
                color={edgeColor(ap.key, ap.redundant)}
                style="fill"
              />
            ))}

            {/* Drag line — always rendered, hidden via opacity when inactive (#219) */}
            <Path
              path={dragPath}
              color={BRAND_COLOR}
              style="stroke"
              strokeWidth={2}
              opacity={dragActive}
            />

            {/* Cut line (#382) — dashed red line for edge cutting */}
            <Path
              path={cutPath}
              color={COLORS.red}
              style="stroke"
              strokeWidth={2}
              opacity={cutActive}
            >
              <DashPathEffect intervals={[8, 6]} />
            </Path>

            {/* Nodes — positions from simNodes, visual props from inputNodes */}
            {simNodes.map((node) => {
              // Look up current visual properties from inputNodes (may have
              // changed without triggering a re-simulation).
              const inputNode = inputNodeMap.get(node.id);
              const isDone = inputNode?.color === '#aaa';
              const isHighlight = node.id === highlightTaskId;
              const bgColor = isDone
                ? '#ccc'
                : isHighlight
                  ? COLORS.red
                  : (inputNode?.color ?? BRAND_COLOR);
              // Label is outside the node on a white background, so use
              // dark text for readability (#294)
              const textColor = isDone ? '#999' : '#333';
              const label = inputNode?.label ?? node.label;

              return (
                <Group key={node.id}>
                  <Circle
                    cx={node.x}
                    cy={node.y}
                    r={NODE_RADIUS}
                    color={bgColor}
                  />
                  <NodeLabel
                    x={node.x}
                    y={node.y + LABEL_OFFSET}
                    text={truncate(label, MAX_LABEL_CHARS)}
                    color={textColor}
                    fontSize={fontSize}
                  />
                  {/* Highlight border */}
                  {isHighlight && (
                    <Circle
                      cx={node.x}
                      cy={node.y}
                      r={NODE_RADIUS + 2}
                      color={COLORS.red}
                      style="stroke"
                      strokeWidth={2}
                    />
                  )}
                </Group>
              );
            })}
          </Group>
        </Canvas>
      </Reanimated.View>
    </GestureDetector>
  );
}

function truncate(s: string, maxLen: number): string {
  return s.length > maxLen ? s.slice(0, maxLen - 1) + '…' : s;
}

// ── Label height measurement ──

/** Measure the rendered height of a node label pill for hit testing (#422). */
function getLabelHeight(text: string, fontSize: number): number {
  const builder = Skia.ParagraphBuilder.Make({ textAlign: TextAlign.Center });
  builder.pushStyle({
    fontFamilies: NODE_LABEL_FONTS,
    fontSize,
    fontStyle: { weight: 500 },
    color: Skia.Color('#000000'),
  });
  builder.addText(text);
  builder.pop();
  const p = builder.build();
  p.layout(LABEL_WIDTH);
  return p.getHeight();
}

// ── NodeLabel — uses Skia Paragraph for CJK font fallback (#251) ──
// matchFont returns a single font with no fallback, so Japanese glyphs
// don't render on Android (Roboto lacks CJK). Paragraph's fontFamilies
// list provides per-character fallback: Latin chars use sans-serif,
// Japanese chars fall through to NotoSansCJK.
// Label is drawn below the node (#294) with a white background pill so
// it stays readable even when zoomed out.
const NODE_LABEL_FONTS = Platform.select<string[]>({
  ios: ['Helvetica', 'Hiragino Sans', 'NotoSansCJK'],
  default: [
    'sans-serif',
    'NotoSansCJK',
    'NotoSansJP',
    'Noto Sans CJK JP',
    'DroidSansJapanese',
  ],
})!;

function NodeLabel({
  x,
  y,
  text,
  color,
  fontSize,
}: {
  x: number;
  y: number;
  text: string;
  color: string;
  fontSize: number;
}) {
  const paragraph = useMemo(() => {
    const builder = Skia.ParagraphBuilder.Make({
      textAlign: TextAlign.Center,
    });
    builder.pushStyle({
      fontFamilies: NODE_LABEL_FONTS,
      fontSize,
      fontStyle: { weight: 500 },
      color: Skia.Color(color),
    });
    builder.addText(text);
    builder.pop();
    const p = builder.build();
    p.layout(LABEL_WIDTH);
    return p;
  }, [text, color, fontSize]);

  const h = paragraph.getHeight();
  const bgRect = Skia.XYWHRect(
    x - LABEL_WIDTH / 2 - LABEL_PAD_X,
    y - LABEL_PAD_Y,
    LABEL_WIDTH + LABEL_PAD_X * 2,
    h + LABEL_PAD_Y * 2,
  );
  const bgPath = Skia.Path.Make();
  bgPath.addRRect(Skia.RRectXY(bgRect, 6, 6));

  return (
    <>
      {/* White background pill for readability when zoomed out */}
      <Path path={bgPath} color="#ffffff" style="fill" opacity={0.85} />
      <Paragraph
        paragraph={paragraph}
        x={x - LABEL_WIDTH / 2}
        y={y}
        width={LABEL_WIDTH}
      />
    </>
  );
}

const styles = StyleSheet.create({
  flex: {
    flex: 1,
  },
  empty: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    padding: 20,
  },
});
