// DependencyGraph — force-directed DAG visualization
// Uses @shopify/react-native-skia + d3-force
// Shared by GraphView (full-screen, editable) and TaskDetailView (embedded, read-only)

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Platform,
  type LayoutChangeEvent,
  StyleSheet,
  View,
} from 'react-native';
import {
  Canvas,
  Circle,
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
}

export interface DependencyGraphProps {
  /** Nodes grouped by id → node data */
  nodes: GraphNode[];
  edges: GraphEdge[];
  /** Highlight a specific task (e.g. the one being viewed in detail) */
  highlightTaskId?: string;
  /** Enable edge addition/removal (GraphView only) */
  editMode?: boolean;
  onTapNode?: (taskId: string) => void;
  onCutEdge?: (sourceId: string, targetId: string) => void;
  onAddEdge?: (sourceId: string, targetId: string) => void;
  /** Fixed height for embedded use (TaskDetailView); flex:1 when omitted */
  height?: number;
}

// ── Constants ──

const NODE_RADIUS = 35;
const FONT_SIZE = 13;
const MAX_LABEL_CHARS = 5;
const LABEL_WIDTH = NODE_RADIUS * 1.8;
const HIT_RADIUS = NODE_RADIUS;
const EDGE_HIT_WIDTH = 12;

// ── Helpers ──

/** Distance from point (px,py) to line segment (ax,ay)-(bx,by) */
function distToSegment(
  px: number,
  py: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
): number {
  const dx = bx - ax;
  const dy = by - ay;
  const lenSq = dx * dx + dy * dy;
  if (lenSq === 0) return Math.hypot(px - ax, py - ay);
  let t = ((px - ax) * dx + (py - ay) * dy) / lenSq;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
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

// ── Component ──

export function DependencyGraph({
  nodes: inputNodes,
  edges: inputEdges,
  highlightTaskId,
  editMode = false,
  onTapNode,
  onCutEdge,
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
  const dragSourceRef = useRef<string | null>(null);
  const dragActive = useSharedValue(0);
  const dragSx = useSharedValue(0);
  const dragSy = useSharedValue(0);
  const dragEx = useSharedValue(0);
  const dragEy = useSharedValue(0);

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
      .force('charge', d3.forceManyBody().strength(-400))
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

    // ── Auto-fit: zoom/translate so all nodes are visible (#218) ──
    // Only apply when not in edit mode and no explicit height (full-screen
    // GraphView). Embedded graphs (TaskDetailView) keep scale=1.
    // In edit mode, skip auto-fit so the user's pan/zoom is preserved
    // across edge additions/removals (which trigger refresh → graphKey
    // change → this effect re-runs).
    if (finalNodes.length > 0 && !height && !editMode) {
      const xs = finalNodes.map((n) => n.x);
      const ys = finalNodes.map((n) => n.y);
      const minX = Math.min(...xs) - NODE_RADIUS;
      const maxX = Math.max(...xs) + NODE_RADIUS;
      const minY = Math.min(...ys) - NODE_RADIUS;
      const maxY = Math.max(...ys) + NODE_RADIUS;
      const graphW = maxX - minX;
      const graphH = maxY - minY;
      const cw = canvasSize.width;
      const ch = canvasSize.height;
      if (graphW > 0 && graphH > 0) {
        const padding = 40;
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
      return {
        x: (sx - translateX.value) / scale.value,
        y: (sy - translateY.value) / scale.value,
      };
    },
    [translateX, translateY, scale],
  );

  // ── Gesture: Pan ──
  // Disabled in edit mode to match old WebView behavior (prevents
  // accidental canvas movement while interacting with nodes/edges).
  // Also disabled when embedded (height prop set) so it doesn't block
  // the parent ScrollView's vertical scrolling.

  const panGesture = Gesture.Pan()
    .enabled(!editMode && !height)
    .onChange((e) => {
      translateX.value = translateX.value + e.changeX;
      translateY.value = translateY.value + e.changeY;
    });

  // ── Gesture: Pinch ──

  const pinchGesture = Gesture.Pinch()
    .enabled(!editMode && !height)
    .onChange((e) => {
      scale.value = Math.max(0.3, Math.min(3, scale.value * e.scaleChange));
    });

  // ── Gesture: Tap ──

  const tapGesture = Gesture.Tap().onEnd((e) => {
    const world = toWorld(e.x, e.y);

    // Check node hits
    for (const node of simNodes) {
      const dx = world.x - node.x;
      const dy = world.y - node.y;
      if (Math.hypot(dx, dy) < HIT_RADIUS) {
        if (onTapNode) runOnJS(onTapNode)(node.id);
        return;
      }
    }

    // Check edge hits (only in edit mode)
    if (editMode && onCutEdge) {
      const nodeMap = new Map(simNodes.map((n) => [n.id, n]));
      for (const edge of inputEdges) {
        const s = nodeMap.get(edge.source);
        const t = nodeMap.get(edge.target);
        if (!s || !t) continue;
        const d = distToSegment(world.x, world.y, s.x, s.y, t.x, t.y);
        if (d < EDGE_HIT_WIDTH) {
          runOnJS(onCutEdge)(edge.source, edge.target);
          return;
        }
      }
    }
  });

  // ── Gesture: Long-press → edge drag (edit mode) ──

  const longPressDrag = Gesture.Pan()
    .activateAfterLongPress(200)
    .onStart((e) => {
      if (!editMode || !onAddEdge) return;
      const world = toWorld(e.x, e.y);
      for (const node of simNodes) {
        const dx = world.x - node.x;
        const dy = world.y - node.y;
        if (Math.hypot(dx, dy) < HIT_RADIUS) {
          dragSourceRef.current = node.id;
          dragSx.value = node.x;
          dragSy.value = node.y;
          dragEx.value = node.x;
          dragEy.value = node.y;
          dragActive.value = 1;
          return;
        }
      }
    })
    .onUpdate((e) => {
      if (!dragSourceRef.current) return;
      const world = toWorld(e.x, e.y);
      dragEx.value = world.x;
      dragEy.value = world.y;
    })
    .onEnd((e) => {
      if (!dragSourceRef.current || !onAddEdge) {
        dragSourceRef.current = null;
        dragActive.value = 0;
        return;
      }
      const world = toWorld(e.x, e.y);
      for (const node of simNodes) {
        if (node.id === dragSourceRef.current) continue;
        const dx = world.x - node.x;
        const dy = world.y - node.y;
        if (Math.hypot(dx, dy) < HIT_RADIUS) {
          runOnJS(onAddEdge)(dragSourceRef.current, node.id);
          break;
        }
      }
      dragSourceRef.current = null;
      dragActive.value = 0;
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

  const edgePaths = useMemo(() => {
    const paths: { source: string; target: string; d: string }[] = [];
    for (const edge of inputEdges) {
      const s = nodeMap.get(edge.source);
      const t = nodeMap.get(edge.target);
      if (!s || !t) continue;
      paths.push({
        source: edge.source,
        target: edge.target,
        d: `M ${s.x} ${s.y} L ${t.x} ${t.y}`,
      });
    }
    return paths;
  }, [inputEdges, nodeMap]);

  const arrowPaths = useMemo(() => {
    const paths: string[] = [];
    for (const edge of inputEdges) {
      const s = nodeMap.get(edge.source);
      const t = nodeMap.get(edge.target);
      if (!s || !t) continue;
      const ah = arrowHead(s.x, s.y, t.x, t.y, 10);
      if (ah) paths.push(ah);
    }
    return paths;
  }, [inputEdges, nodeMap]);

  // Drag line path — derived from shared values for smooth UI-thread updates (#219)
  const dragPath = useDerivedValue(() => {
    if (dragActive.value === 0) return '';
    return `M ${dragSx.value} ${dragSy.value} L ${dragEx.value} ${dragEy.value}`;
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
            {/* Edges */}
            {edgePaths.map((ep) => (
              <Path
                key={`e-${ep.source}-${ep.target}`}
                path={ep.d}
                color={colors.grayLight ?? '#aaa'}
                style="stroke"
                strokeWidth={2}
              />
            ))}

            {/* Arrowheads */}
            {arrowPaths.map((d, i) => (
              <Path
                key={`a-${i}`}
                path={d}
                color={colors.grayLight ?? '#aaa'}
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
                  : BRAND_COLOR;
              const textColor = isDone ? '#666' : COLORS.white;
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
                    y={node.y}
                    text={truncate(label, MAX_LABEL_CHARS)}
                    color={textColor}
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

// ── NodeLabel — uses Skia Paragraph for CJK font fallback (#251) ──
// matchFont returns a single font with no fallback, so Japanese glyphs
// don't render on Android (Roboto lacks CJK). Paragraph's fontFamilies
// list provides per-character fallback: Latin chars use sans-serif,
// Japanese chars fall through to NotoSansCJK.
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
}: {
  x: number;
  y: number;
  text: string;
  color: string;
}) {
  const paragraph = useMemo(() => {
    const builder = Skia.ParagraphBuilder.Make({
      textAlign: TextAlign.Center,
    });
    builder.pushStyle({
      fontFamilies: NODE_LABEL_FONTS,
      fontSize: FONT_SIZE,
      fontStyle: { weight: 500 },
      color: Skia.Color(color),
    });
    builder.addText(text);
    builder.pop();
    const p = builder.build();
    p.layout(LABEL_WIDTH);
    return p;
  }, [text, color]);

  const h = paragraph.getHeight();
  return (
    <Paragraph
      paragraph={paragraph}
      x={x - LABEL_WIDTH / 2}
      y={y - h / 2}
      width={LABEL_WIDTH}
    />
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
