import React, { useMemo, useRef, useState } from 'react';

import type {
  SosDependencyGraphEdge,
  SosDependencyGraphNode,
  SosDependencyGraphResponse,
} from '@/api/sosValidation';
import {
  buildVisibleKindState,
  getDependencyGraphEdgeKey,
  type SosGraphKind,
} from '@/components/sos/sosDependencyGraphUtils';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

interface PositionedNode {
  node: SosDependencyGraphNode;
  x: number;
  y: number;
  width: number;
  height: number;
}

interface PositionedEdge {
  edge: SosDependencyGraphEdge;
  key: string;
  path: string;
  labelX: number;
  labelY: number;
}

interface SosDependencyGraphViewProps {
  graph: SosDependencyGraphResponse;
  selectedNodeId?: string | null;
  onSelectNode?: (node: SosDependencyGraphNode) => void;
  selectedEdgeKey?: string | null;
  onSelectEdge?: (edge: SosDependencyGraphEdge) => void;
  visibleKinds?: Record<SosGraphKind, boolean>;
  onVisibleKindsChange?: (visibleKinds: Record<SosGraphKind, boolean>) => void;
}

const LANE_ORDER: SosGraphKind[] = ['system', 'interface', 'contract'];
const LANE_LABELS: Record<SosGraphKind, string> = {
  system: 'Systems',
  interface: 'Interfaces',
  contract: 'Contracts',
};
const EDGE_LABELS: Record<string, string> = {
  exposes: 'exposes',
  governs_provider: 'provider',
  governs_consumer: 'consumer',
  integrates_with: 'integration',
};
const NODE_STYLE: Record<SosGraphKind, { fill: string; stroke: string; badge: string }> = {
  system: {
    fill: '#dbeafe',
    stroke: '#2563eb',
    badge: 'bg-blue-50 text-blue-800 border-blue-200',
  },
  interface: {
    fill: '#d1fae5',
    stroke: '#0f766e',
    badge: 'bg-teal-50 text-teal-800 border-teal-200',
  },
  contract: {
    fill: '#fef3c7',
    stroke: '#d97706',
    badge: 'bg-amber-50 text-amber-900 border-amber-200',
  },
};
const EDGE_STYLE: Record<string, { stroke: string; dashArray?: string }> = {
  exposes: { stroke: '#2563eb' },
  governs_provider: { stroke: '#d97706' },
  governs_consumer: { stroke: '#0f766e' },
  integrates_with: { stroke: '#7c3aed', dashArray: '8 6' },
};
const MIN_ZOOM = 0.65;
const MAX_ZOOM = 1.85;

export function SosDependencyGraphView({
  graph,
  selectedNodeId,
  onSelectNode,
  selectedEdgeKey,
  onSelectEdge,
  visibleKinds,
  onVisibleKindsChange,
}: SosDependencyGraphViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [zoom, setZoom] = useState(1);
  const [internalVisibleKinds, setInternalVisibleKinds] = useState<Record<SosGraphKind, boolean>>(
    buildVisibleKindState()
  );
  const resolvedVisibleKinds = visibleKinds ?? internalVisibleKinds;

  const visibleLaneOrder = useMemo(
    () => LANE_ORDER.filter((kind) => resolvedVisibleKinds[kind]),
    [resolvedVisibleKinds]
  );

  const { positionedNodes, positionedEdges, viewWidth, viewHeight, legendKinds } = useMemo(() => {
    const nodeWidth = 196;
    const nodeHeight = 62;
    const leftMargin = 96;
    const topMargin = 88;
    const rowGap = 176;
    const columnGap = 228;

    const grouped = {
      system: sortNodes(
        graph.nodes.filter(
          (node) => coerceKind(node.kind) === 'system' && resolvedVisibleKinds.system
        )
      ),
      interface: sortNodes(
        graph.nodes.filter(
          (node) => coerceKind(node.kind) === 'interface' && resolvedVisibleKinds.interface
        )
      ),
      contract: sortNodes(
        graph.nodes.filter(
          (node) => coerceKind(node.kind) === 'contract' && resolvedVisibleKinds.contract
        )
      ),
    };

    const activeLaneOrder = LANE_ORDER.filter((kind) => resolvedVisibleKinds[kind]);
    const maxColumns = Math.max(
      grouped.system.length,
      grouped.interface.length,
      grouped.contract.length,
      1
    );
    const viewWidth = Math.max(960, leftMargin * 2 + nodeWidth + (maxColumns - 1) * columnGap);
    const viewHeight =
      topMargin * 2 + nodeHeight + Math.max(activeLaneOrder.length - 1, 0) * rowGap;

    const positions = new Map<string, PositionedNode>();

    activeLaneOrder.forEach((kind, laneIndex) => {
      grouped[kind].forEach((node, columnIndex) => {
        positions.set(node.id, {
          node,
          x: leftMargin + columnIndex * columnGap,
          y: topMargin + laneIndex * rowGap,
          width: nodeWidth,
          height: nodeHeight,
        });
      });
    });

    const visibleNodeIds = new Set(positions.keys());
    const positionedEdges = graph.edges
      .filter((edge) => visibleNodeIds.has(edge.from) && visibleNodeIds.has(edge.to))
      .map((edge) => {
        const source = positions.get(edge.from);
        const target = positions.get(edge.to);
        if (!source || !target) {
          return null;
        }

        return {
          edge,
          key: getDependencyGraphEdgeKey(edge),
          path: buildEdgePath(source, target, edge),
          labelX: (centerX(source) + centerX(target)) / 2,
          labelY: buildLabelY(source, target, edge),
        } satisfies PositionedEdge;
      })
      .filter((entry): entry is PositionedEdge => Boolean(entry));

    return {
      positionedNodes: Array.from(positions.values()),
      positionedEdges,
      viewWidth,
      viewHeight,
      legendKinds: Object.keys(EDGE_STYLE),
    };
  }, [graph, resolvedVisibleKinds]);

  const selectedNeighborIds = useMemo(() => {
    if (!selectedNodeId) {
      return new Set<string>();
    }

    const neighbors = new Set<string>();
    positionedEdges.forEach(({ edge }) => {
      if (edge.from === selectedNodeId) {
        neighbors.add(edge.to);
      }
      if (edge.to === selectedNodeId) {
        neighbors.add(edge.from);
      }
    });
    return neighbors;
  }, [positionedEdges, selectedNodeId]);

  const selectedEdgeEndpointIds = useMemo(() => {
    if (!selectedEdgeKey) {
      return new Set<string>();
    }

    const selectedEdge = positionedEdges.find((entry) => entry.key === selectedEdgeKey)?.edge;
    if (!selectedEdge) {
      return new Set<string>();
    }

    return new Set([selectedEdge.from, selectedEdge.to]);
  }, [positionedEdges, selectedEdgeKey]);

  const handleToggleKind = (kind: SosGraphKind) => {
    const activeCount = Object.values(resolvedVisibleKinds).filter(Boolean).length;
    if (resolvedVisibleKinds[kind] && activeCount === 1) {
      return;
    }

    const nextVisibleKinds = {
      ...resolvedVisibleKinds,
      [kind]: !resolvedVisibleKinds[kind],
    };

    if (onVisibleKindsChange) {
      onVisibleKindsChange(nextVisibleKinds);
      return;
    }

    setInternalVisibleKinds(nextVisibleKinds);
  };

  const handleFitView = () => {
    setZoom(1);
    containerRef.current?.scrollTo({ left: 0, top: 0, behavior: 'smooth' });
  };

  const adjustZoom = (delta: number) => {
    setZoom((current) => clamp(roundZoom(current + delta), MIN_ZOOM, MAX_ZOOM));
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
          {LANE_ORDER.map((kind) => {
            const isVisible = resolvedVisibleKinds[kind];
            return (
              <button
                key={kind}
                type="button"
                onClick={() => handleToggleKind(kind)}
                className={cn(
                  'inline-flex items-center gap-2 rounded-full border px-2.5 py-1 transition-colors',
                  isVisible
                    ? NODE_STYLE[kind].badge
                    : 'border-border bg-background text-muted-foreground'
                )}
              >
                <span
                  className="h-2.5 w-2.5 rounded-full"
                  style={{
                    backgroundColor: isVisible ? NODE_STYLE[kind].stroke : 'rgba(100,116,139,0.5)',
                  }}
                />
                {LANE_LABELS[kind]}
              </button>
            );
          })}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => adjustZoom(-0.15)}
            disabled={zoom <= MIN_ZOOM}
          >
            -
          </Button>
          <Badge variant="outline">{Math.round(zoom * 100)}%</Badge>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => adjustZoom(0.15)}
            disabled={zoom >= MAX_ZOOM}
          >
            +
          </Button>
          <Button type="button" variant="outline" size="sm" onClick={handleFitView}>
            Fit View
          </Button>
        </div>
      </div>

      <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
        {LANE_ORDER.map((kind) => (
          <span
            key={kind}
            className={cn(
              'inline-flex items-center gap-2 rounded-full border px-2.5 py-1',
              NODE_STYLE[kind].badge
            )}
          >
            <span
              className="h-2.5 w-2.5 rounded-full"
              style={{ backgroundColor: NODE_STYLE[kind].stroke }}
            />
            {LANE_LABELS[kind]}
          </span>
        ))}
        {legendKinds.map((kind) => (
          <span key={kind} className="inline-flex items-center gap-2 rounded-full border px-2.5 py-1">
            <span
              className="h-0.5 w-5 rounded-full"
              style={{
                backgroundColor: EDGE_STYLE[kind].stroke,
                borderTop: EDGE_STYLE[kind].dashArray
                  ? `2px dashed ${EDGE_STYLE[kind].stroke}`
                  : undefined,
              }}
            />
            {EDGE_LABELS[kind] ?? kind}
          </span>
        ))}
      </div>

      <div
        ref={containerRef}
        className="overflow-auto rounded-sm border border-border bg-[radial-gradient(circle_at_top,#f8fafc,transparent_42%),linear-gradient(180deg,#ffffff,rgba(241,245,249,0.9))]"
      >
        {positionedNodes.length === 0 || visibleLaneOrder.length === 0 ? (
          <div className="flex min-h-[280px] items-center justify-center p-6 text-sm text-muted-foreground">
            All graph kinds are hidden. Re-enable at least one lane to inspect the topology.
          </div>
        ) : (
          <svg
            viewBox={`0 0 ${viewWidth} ${viewHeight}`}
            style={{
              width: `${Math.round(viewWidth * zoom)}px`,
              height: `${Math.round(viewHeight * zoom)}px`,
              minWidth: `${Math.round(viewWidth * zoom)}px`,
            }}
            role="img"
            aria-label="Systems-of-systems dependency graph"
          >
            <defs>
              <pattern id="sos-grid" width="28" height="28" patternUnits="userSpaceOnUse">
                <path
                  d="M 28 0 L 0 0 0 28"
                  fill="none"
                  stroke="rgba(148,163,184,0.18)"
                  strokeWidth="1"
                />
              </pattern>
            </defs>

            <rect x="0" y="0" width={viewWidth} height={viewHeight} fill="url(#sos-grid)" />

            {visibleLaneOrder.map((kind, index) => {
              const laneY = 52 + index * 176;
              return (
                <g key={kind}>
                  <line
                    x1="56"
                    y1={laneY}
                    x2={viewWidth - 56}
                    y2={laneY}
                    stroke="rgba(148,163,184,0.32)"
                    strokeDasharray="8 10"
                  />
                  <text x="56" y={laneY - 14} fill="#475569" fontSize="14" fontWeight="700">
                    {LANE_LABELS[kind]}
                  </text>
                </g>
              );
            })}

            {positionedEdges.map(({ edge, key, path, labelX, labelY }) => {
              const style = EDGE_STYLE[edge.kind] ?? { stroke: '#64748b' };
              const isSelected = selectedEdgeKey === key;
              const isLinkedToSelectedNode =
                Boolean(selectedNodeId) &&
                (edge.from === selectedNodeId || edge.to === selectedNodeId);
              const isLinkedToSelectedEdge =
                selectedEdgeEndpointIds.has(edge.from) || selectedEdgeEndpointIds.has(edge.to);
              const opacity = selectedEdgeKey
                ? isSelected
                  ? 1
                  : isLinkedToSelectedEdge
                    ? 0.28
                    : 0.08
                : selectedNodeId
                  ? isLinkedToSelectedNode
                    ? 0.98
                    : 0.16
                  : 0.98;

              return (
                <g key={key} opacity={opacity} className="cursor-pointer">
                  <path
                    d={path}
                    fill="none"
                    stroke="transparent"
                    strokeWidth="14"
                    onClick={() => onSelectEdge?.(edge)}
                  />
                  <path
                    d={path}
                    fill="none"
                    stroke={style.stroke}
                    strokeWidth={isSelected ? 3.75 : edge.kind === 'integrates_with' ? 2.5 : 2.25}
                    strokeDasharray={style.dashArray}
                    strokeLinecap="round"
                    onClick={() => onSelectEdge?.(edge)}
                  />
                  <g transform={`translate(${labelX}, ${labelY})`} onClick={() => onSelectEdge?.(edge)}>
                    <rect
                      x={-44}
                      y={-10}
                      width="88"
                      height="20"
                      rx="10"
                      fill={isSelected ? 'rgba(248,250,252,0.98)' : 'rgba(255,255,255,0.92)'}
                      stroke={isSelected ? style.stroke : 'rgba(148,163,184,0.45)'}
                    />
                    <text
                      textAnchor="middle"
                      dominantBaseline="middle"
                      fontSize="10"
                      fontWeight="700"
                      fill="#334155"
                    >
                      {EDGE_LABELS[edge.kind] ?? edge.kind}
                    </text>
                  </g>
                </g>
              );
            })}

            {positionedNodes.map((position) => {
              const { node } = position;
              const kind = coerceKind(node.kind);
              const style = NODE_STYLE[kind];
              const isSelected = selectedNodeId === node.id;
              const isConnected = selectedNeighborIds.has(node.id);
              const isSelectedEdgeEndpoint = selectedEdgeEndpointIds.has(node.id);
              const isDimmedByEdge = Boolean(selectedEdgeKey) && !isSelected && !isSelectedEdgeEndpoint;
              const isDimmedByNode =
                !selectedEdgeKey && Boolean(selectedNodeId) && !isSelected && !isConnected;
              const isDimmed = isDimmedByEdge || isDimmedByNode;

              return (
                <g
                  key={node.id}
                  transform={`translate(${position.x}, ${position.y})`}
                  opacity={isDimmed ? 0.24 : 1}
                  className="cursor-pointer"
                  onClick={() => onSelectNode?.(node)}
                >
                  <rect
                    width={position.width}
                    height={position.height}
                    rx="18"
                    fill={style.fill}
                    stroke={isSelected ? '#0f172a' : style.stroke}
                    strokeWidth={isSelected ? 3 : 2}
                  />
                  <rect
                    x="14"
                    y="12"
                    width="60"
                    height="18"
                    rx="9"
                    fill="rgba(255,255,255,0.82)"
                    stroke={style.stroke}
                    strokeWidth="1"
                  />
                  <text
                    x="44"
                    y="24.5"
                    textAnchor="middle"
                    fontSize="10"
                    fontWeight="700"
                    fill="#0f172a"
                  >
                    {kind.toUpperCase()}
                  </text>
                  <text x="14" y="43" fontSize="13" fontWeight="700" fill="#0f172a">
                    {truncate(node.label, 26)}
                  </text>
                  <text x="14" y="57" fontSize="10.5" fill="#475569">
                    {truncate(node.id, 32)}
                  </text>
                </g>
              );
            })}
          </svg>
        )}
      </div>

      <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
        <Badge variant="outline">{graph.nodes.length} nodes total</Badge>
        <Badge variant="outline">{positionedNodes.length} visible</Badge>
        <Badge variant="outline">{graph.edges.length} edges total</Badge>
        <Badge variant="outline">{positionedEdges.length} visible</Badge>
        <span>Click a node to focus a neighborhood, or click an edge to inspect the contract path.</span>
      </div>
    </div>
  );
}

function sortNodes(nodes: SosDependencyGraphNode[]): SosDependencyGraphNode[] {
  return [...nodes].sort((left, right) => {
    const leftGroup = left.system_id ?? left.system_type ?? '';
    const rightGroup = right.system_id ?? right.system_type ?? '';

    return (
      leftGroup.localeCompare(rightGroup) ||
      left.label.localeCompare(right.label) ||
      left.id.localeCompare(right.id)
    );
  });
}

function coerceKind(raw: string): SosGraphKind {
  if (raw === 'system' || raw === 'interface' || raw === 'contract') {
    return raw;
  }
  return 'interface';
}

function centerX(node: PositionedNode): number {
  return node.x + node.width / 2;
}

function centerY(node: PositionedNode): number {
  return node.y + node.height / 2;
}

function buildLabelY(
  source: PositionedNode,
  target: PositionedNode,
  edge: SosDependencyGraphEdge
): number {
  if (source.node.kind === target.node.kind || edge.kind === 'integrates_with') {
    return Math.min(source.y, target.y) - 26;
  }

  return (centerY(source) + centerY(target)) / 2;
}

function buildEdgePath(
  source: PositionedNode,
  target: PositionedNode,
  edge: SosDependencyGraphEdge
): string {
  if (source.node.kind === target.node.kind || edge.kind === 'integrates_with') {
    const startX = centerX(source);
    const startY = source.y;
    const endX = centerX(target);
    const endY = target.y;
    const arcHeight = Math.max(54, Math.abs(endX - startX) * 0.18);
    const controlY = Math.min(startY, endY) - arcHeight;
    return `M ${startX} ${startY} C ${startX} ${controlY}, ${endX} ${controlY}, ${endX} ${endY}`;
  }

  const sourceKindIndex = LANE_ORDER.indexOf(coerceKind(source.node.kind));
  const targetKindIndex = LANE_ORDER.indexOf(coerceKind(target.node.kind));
  const flowsDown = sourceKindIndex < targetKindIndex;
  const startX = centerX(source);
  const endX = centerX(target);
  const startY = flowsDown ? source.y + source.height : source.y;
  const endY = flowsDown ? target.y : target.y + target.height;
  const verticalDistance = Math.abs(endY - startY);
  const controlOffset = Math.max(48, verticalDistance * 0.52);
  const sourceControlY = flowsDown ? startY + controlOffset : startY - controlOffset;
  const targetControlY = flowsDown ? endY - controlOffset : endY + controlOffset;

  return `M ${startX} ${startY} C ${startX} ${sourceControlY}, ${endX} ${targetControlY}, ${endX} ${endY}`;
}

function truncate(value: string, maxLength: number): string {
  if (value.length <= maxLength) {
    return value;
  }

  return `${value.slice(0, maxLength - 3)}...`;
}

function roundZoom(value: number): number {
  return Math.round(value * 100) / 100;
}

function clamp(value: number, min: number, max: number): number {
  if (value < min) {
    return min;
  }
  if (value > max) {
    return max;
  }
  return value;
}
