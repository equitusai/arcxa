/**
 * LineageGraph Component
 * Beautiful Sankey diagram visualization for data lineage
 * Uses D3 for layout and custom SVG rendering for enterprise polish
 */

import React, { useEffect, useRef, useState, useMemo } from 'react';
import {
  sankey as d3Sankey,
  sankeyLinkHorizontal,
  sankeyLeft,
  SankeyNode,
  SankeyLink,
  SankeyGraph,
} from 'd3-sankey';
import { scaleLinear } from 'd3-scale';
import { cn } from '@/lib/utils';
import { detectLineageAnomalies, getNodeAnomalies, type AnomalyReport } from '@/lib/lineage-anomalies';
import type { LineageGraph, LineageNode, LineageEdge } from '@/hooks/useLineageGraph';

interface LineageGraphProps {
  graph: LineageGraph;
  width?: number;
  height?: number;
  onNodeClick?: (node: LineageNode) => void;
  onEdgeClick?: (edge: LineageEdge) => void;
  selectedNodeId?: string;
  showAnomalies?: boolean; // Phase 2 feature
  simulationGraph?: LineageGraph; // Phase 3: What-If simulation result
  viewMode?: 'current' | 'simulated' | 'diff'; // Phase 3: View mode for simulation
  className?: string;
}

// Premium color scheme inspired by LineageMockup
const NODE_TYPE_COLORS = {
  source: { light: '#3b82f6', dark: '#60a5fa' }, // blue
  quality: { light: '#f59e0b', dark: '#fbbf24' }, // amber
  transform: { light: '#8b5cf6', dark: '#a78bfa' }, // purple
  mapping: { light: '#10b981', dark: '#34d399' }, // green
  destination: { light: '#06b6d4', dark: '#22d3ee' }, // cyan
  intermediate: { light: '#8b5cf6', dark: '#a78bfa' }, // purple (same as transform)
  default: { light: '#9CA1AB', dark: '#9CA1AB' }, // neutral gray
};

function getNodeColor(nodeType: string | undefined, isDark = false): string {
  const type = (nodeType?.toLowerCase() || 'default') as keyof typeof NODE_TYPE_COLORS;
  const colors = NODE_TYPE_COLORS[type] || NODE_TYPE_COLORS.default;
  return isDark ? colors.dark : colors.light;
}

interface D3Node extends SankeyNode<LineageNode, LineageEdge> {
  // D3 adds x0, x1, y0, y1 for positioning
}

interface D3Link extends SankeyLink<LineageNode, LineageEdge> {
  // D3 adds source/target with full node data
}

// Phase 3: Diff computation types
type DiffStatus = 'added' | 'removed' | 'improved' | 'degraded' | 'changed' | 'unchanged';

interface NodeDiff {
  nodeId: string;
  status: DiffStatus;
  oldConfidence?: number;
  newConfidence?: number;
  confidenceDelta?: number;
}

interface EdgeDiff {
  edgeId: string;
  status: DiffStatus;
}

export function LineageGraph({
  graph,
  width = 1200,
  height = 600,
  onNodeClick,
  onEdgeClick,
  selectedNodeId,
  showAnomalies = true,
  simulationGraph,
  viewMode = 'current',
  className,
}: LineageGraphProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [hoveredEdge, setHoveredEdge] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });

  // PHASE 3: Compute diff between current and simulated graphs
  const { nodeDiffs, edgeDiffs } = useMemo(() => {
    if (!simulationGraph) return { nodeDiffs: new Map<string, NodeDiff>(), edgeDiffs: new Map<string, EdgeDiff>() };

    const nodeDiffMap = new Map<string, NodeDiff>();
    const edgeDiffMap = new Map<string, EdgeDiff>();

    // Compare nodes
    const simNodeMap = new Map(simulationGraph.nodes.map((n) => [n.id, n]));

    graph.nodes.forEach((origNode) => {
      const simNode = simNodeMap.get(origNode.id);
      if (!simNode) {
        // Node removed in simulation
        nodeDiffMap.set(origNode.id, {
          nodeId: origNode.id,
          status: 'removed',
          oldConfidence: origNode.confidence,
        });
      } else {
        // Node exists in both
        const oldConf = origNode.confidence || 0;
        const newConf = simNode.confidence || 0;
        const delta = newConf - oldConf;

        let status: DiffStatus = 'unchanged';
        if (Math.abs(delta) > 0.01) {
          if (delta > 0) status = 'improved';
          else status = 'degraded';
        }

        nodeDiffMap.set(origNode.id, {
          nodeId: origNode.id,
          status,
          oldConfidence: oldConf,
          newConfidence: newConf,
          confidenceDelta: delta,
        });
      }
    });

    // Find added nodes
    simulationGraph.nodes.forEach((simNode) => {
      if (!graph.nodes.find((n) => n.id === simNode.id)) {
        nodeDiffMap.set(simNode.id, {
          nodeId: simNode.id,
          status: 'added',
          newConfidence: simNode.confidence,
        });
      }
    });

    // Compare edges
    const simEdgeMap = new Map(simulationGraph.edges.map((e) => [`${e.source}-${e.target}`, e]));

    graph.edges.forEach((origEdge) => {
      const key = `${origEdge.source}-${origEdge.target}`;
      if (!simEdgeMap.has(key)) {
        edgeDiffMap.set(origEdge.id, { edgeId: origEdge.id, status: 'removed' });
      } else {
        edgeDiffMap.set(origEdge.id, { edgeId: origEdge.id, status: 'unchanged' });
      }
    });

    // Find added edges
    simulationGraph.edges.forEach((simEdge) => {
      const key = `${simEdge.source}-${simEdge.target}`;
      if (!graph.edges.find((e) => `${e.source}-${e.target}` === key)) {
        edgeDiffMap.set(simEdge.id, { edgeId: simEdge.id, status: 'added' });
      }
    });

    return { nodeDiffs: nodeDiffMap, edgeDiffs: edgeDiffMap };
  }, [graph, simulationGraph]);

  // PHASE 2: Detect anomalies in lineage graph
  const anomalyReport = useMemo(() => {
    if (!showAnomalies) return null;
    return detectLineageAnomalies(graph);
  }, [graph, showAnomalies]);

  // PHASE 3: Select which graph to render based on viewMode
  const activeGraph = useMemo(() => {
    if (viewMode === 'simulated' && simulationGraph) return simulationGraph;
    return graph;
  }, [viewMode, graph, simulationGraph]);

  // Transform graph data for D3 Sankey
  const sankeyData = useMemo<SankeyGraph<LineageNode, LineageEdge>>(() => {
    // D3 Sankey expects nodes with numeric indices
    const nodeMap = new Map<string, number>();
    activeGraph.nodes.forEach((node, idx) => {
      nodeMap.set(node.id, idx);
    });

    const nodes: LineageNode[] = [...activeGraph.nodes];
    const links: LineageEdge[] = activeGraph.edges.map((edge) => ({
      ...edge,
      source: nodeMap.get(edge.source)!,
      target: nodeMap.get(edge.target)!,
      value: edge.value || 1,
    })) as any;

    return { nodes, links };
  }, [activeGraph]);

  // Compute Sankey layout
  const { nodes: sankeyNodes, links: sankeyLinks } = useMemo(() => {
    if (!sankeyData.nodes.length) {
      return { nodes: [], links: [] };
    }

    const sankeyGenerator = d3Sankey<LineageNode, LineageEdge>()
      .nodeId((d: any) => d.id)
      .nodeWidth(16)
      .nodePadding(20)
      .extent([
        [40, 40],
        [width - 40, height - 40],
      ])
      .nodeAlign(sankeyLeft)
      .iterations(32); // More iterations for better layout

    const computed = sankeyGenerator(sankeyData);
    return {
      nodes: computed.nodes as D3Node[],
      links: computed.links as D3Link[],
    };
  }, [sankeyData, width, height]);

  // Reset zoom/pan when graph changes
  useEffect(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, [graph]);

  // Handle wheel zoom
  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setZoom((prev) => Math.max(0.5, Math.min(3, prev * delta)));
  };

  // Empty state
  if (!graph.nodes.length) {
    return (
      <div className={cn('flex items-center justify-center h-full bg-background', className)}>
        <div className="text-center">
          <div className="text-muted-foreground text-sm mb-2">No lineage data available</div>
          <div className="text-muted-foreground text-xs">
            Search for a record ID to view its lineage graph
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={cn('relative w-full h-full bg-background overflow-hidden', className)}>
      {/* Zoom Controls */}
      <div className="absolute top-4 right-4 z-10 flex flex-col gap-1 bg-card border border-border rounded-md shadow-md">
        <button
          onClick={() => setZoom((prev) => Math.min(3, prev * 1.2))}
          className="px-2 py-1 text-xs font-medium hover:bg-muted transition-colors"
          title="Zoom In"
        >
          +
        </button>
        <div className="px-2 py-0.5 text-[10px] text-center text-muted-foreground border-t border-b">
          {(zoom * 100).toFixed(0)}%
        </div>
        <button
          onClick={() => setZoom((prev) => Math.max(0.5, prev / 1.2))}
          className="px-2 py-1 text-xs font-medium hover:bg-muted transition-colors"
          title="Zoom Out"
        >
          −
        </button>
        <button
          onClick={() => {
            setZoom(1);
            setPan({ x: 0, y: 0 });
          }}
          className="px-2 py-1 text-xs font-medium hover:bg-muted transition-colors border-t"
          title="Reset View"
        >
          ⊙
        </button>
      </div>

      {/* Graph Stats - matching mockup style */}
      {viewMode === 'diff' && simulationGraph ? (
        <div className="absolute top-4 left-4 z-10 px-3 py-2 bg-white/95 dark:bg-neutral-900/95 backdrop-blur-sm border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg">
          <div className="flex items-center gap-3 text-xs">
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Improved:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">
                {Array.from(nodeDiffs.values()).filter((d) => d.status === 'improved').length}
              </span>
            </div>
            <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Degraded:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">
                {Array.from(nodeDiffs.values()).filter((d) => d.status === 'degraded').length}
              </span>
            </div>
            <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Added:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">
                {Array.from(nodeDiffs.values()).filter((d) => d.status === 'added').length}
              </span>
            </div>
            <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Removed:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">
                {Array.from(nodeDiffs.values()).filter((d) => d.status === 'removed').length}
              </span>
            </div>
          </div>
        </div>
      ) : (
        <div className="absolute top-4 left-4 z-10 px-3 py-2 bg-white/95 dark:bg-neutral-900/95 backdrop-blur-sm border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg">
          <div className="flex items-center gap-3 text-xs">
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Nodes:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">{activeGraph.nodes.length}</span>
            </div>
            <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Edges:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">{activeGraph.edges.length}</span>
            </div>
            <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
            <div className="flex items-center gap-1">
              <span className="text-neutral-600 dark:text-neutral-400">Datasets:</span>
              <span className="font-semibold text-neutral-900 dark:text-neutral-50">{activeGraph.metadata.datasets.size}</span>
            </div>
            {anomalyReport && anomalyReport.summary.total > 0 && (
              <>
                <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
                <div className="flex items-center gap-1">
                  <span className="text-neutral-600 dark:text-neutral-400">Issues:</span>
                  <span className="font-semibold text-red-600 dark:text-red-400">{anomalyReport.summary.critical + anomalyReport.summary.high}</span>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* SVG Canvas */}
      <svg
        ref={svgRef}
        width={width}
        height={height}
        className="w-full h-full"
        onWheel={handleWheel}
        style={{ cursor: 'grab' }}
      >
        {/* Grid Background */}
        <defs>
          {/* Premium grid pattern - matching mockup style */}
          <pattern
            id="grid"
            width="40"
            height="40"
            patternUnits="userSpaceOnUse"
            patternTransform={`translate(${pan.x} ${pan.y})`}
          >
            <circle cx="2" cy="2" r="1" className="dark:fill-neutral-700 fill-neutral-300" fillOpacity="0.3" />
          </pattern>

          {/* Gradient definitions for edges - using mockup colors */}
          <linearGradient id="edge-gradient-high" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#10b981" stopOpacity="0.6" />
            <stop offset="100%" stopColor="#10b981" stopOpacity="0.2" />
          </linearGradient>
          <linearGradient id="edge-gradient-medium" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#f59e0b" stopOpacity="0.6" />
            <stop offset="100%" stopColor="#f59e0b" stopOpacity="0.2" />
          </linearGradient>
          <linearGradient id="edge-gradient-low" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#ef4444" stopOpacity="0.6" />
            <stop offset="100%" stopColor="#ef4444" stopOpacity="0.2" />
          </linearGradient>
          <linearGradient id="edge-gradient-default" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#9CA1AB" stopOpacity="0.4" />
            <stop offset="100%" stopColor="#9CA1AB" stopOpacity="0.2" />
          </linearGradient>
        </defs>

        <rect width={width} height={height} fill="url(#grid)" />

        {/* Transform group for zoom/pan */}
        <g transform={`translate(${pan.x}, ${pan.y}) scale(${zoom})`}>
          {/* Render Links (Edges) */}
          {sankeyLinks.map((link, idx) => {
            const linkData = link as D3Link;
            const originalEdge = activeGraph.edges.find(
              (e) =>
                e.source === (linkData.source as any).id && e.target === (linkData.target as any).id
            );
            if (!originalEdge) return null;

            const isHovered = hoveredEdge === originalEdge.id;
            const confidence = originalEdge.confidence;

            // PHASE 3: Get diff status for this edge
            const edgeDiff = viewMode === 'diff' ? edgeDiffs.get(originalEdge.id) : undefined;
            const diffStatus = edgeDiff?.status;

            // Select gradient/color based on confidence or diff status
            let gradientUrl = 'url(#edge-gradient-default)';
            let strokeColor: string | undefined;

            if (viewMode === 'diff' && diffStatus) {
              // Diff mode: color by change status
              if (diffStatus === 'added') strokeColor = '#107C10'; // Green
              else if (diffStatus === 'removed') strokeColor = '#D13438'; // Red
              else strokeColor = '#9CA1AB'; // Gray (unchanged)
            } else {
              // Normal mode: color by confidence
              if (confidence !== undefined) {
                if (confidence >= 0.9) gradientUrl = 'url(#edge-gradient-high)';
                else if (confidence >= 0.7) gradientUrl = 'url(#edge-gradient-medium)';
                else gradientUrl = 'url(#edge-gradient-low)';
              }
            }

            const pathGenerator = sankeyLinkHorizontal();
            const pathData = pathGenerator(linkData as any);

            return (
              <g key={`link-${idx}`}>
                {/* Invisible thick path for easier hover */}
                <path
                  d={pathData || undefined}
                  fill="none"
                  stroke="transparent"
                  strokeWidth={Math.max(linkData.width || 1, 10)}
                  style={{ cursor: 'pointer' }}
                  onMouseEnter={() => setHoveredEdge(originalEdge.id)}
                  onMouseLeave={() => setHoveredEdge(null)}
                  onClick={() => onEdgeClick?.(originalEdge)}
                />
                {/* Visible path */}
                <path
                  d={pathData || undefined}
                  fill="none"
                  stroke={isHovered ? originalEdge.color || '#0078D4' : strokeColor || gradientUrl}
                  strokeWidth={isHovered ? (linkData.width || 1) * 1.5 : linkData.width || 1}
                  opacity={isHovered ? 1 : diffStatus === 'removed' ? 0.3 : 0.6}
                  strokeDasharray={diffStatus === 'removed' ? '4 4' : undefined}
                  style={{
                    transition: 'all 0.2s ease',
                    pointerEvents: 'none',
                  }}
                />
              </g>
            );
          })}

          {/* Render Nodes */}
          {sankeyNodes.map((node, idx) => {
            const nodeData = node as D3Node;
            const originalNode = activeGraph.nodes.find((n) => n.id === nodeData.id);
            if (!originalNode) return null;

            const isHovered = hoveredNode === originalNode.id;
            const isSelected = selectedNodeId === originalNode.id;
            const nodeHeight = (nodeData.y1 || 0) - (nodeData.y0 || 0);
            const nodeWidth = (nodeData.x1 || 0) - (nodeData.x0 || 0);

            // PHASE 3: Get diff status for this node
            const nodeDiff = viewMode === 'diff' ? nodeDiffs.get(originalNode.id) : undefined;
            const diffStatus = nodeDiff?.status;

            // PHASE 2: Get anomalies for this node
            const nodeAnomalies = anomalyReport ? getNodeAnomalies(originalNode.id, anomalyReport) : [];
            const hasCriticalAnomaly = nodeAnomalies.some((a) => a.severity === 'critical');
            const hasHighAnomaly = nodeAnomalies.some((a) => a.severity === 'high');
            const hasAnomaly = nodeAnomalies.length > 0;

            return (
              <g
                key={`node-${idx}`}
                transform={`translate(${nodeData.x0}, ${nodeData.y0})`}
                style={{ cursor: 'pointer' }}
                onMouseEnter={() => setHoveredNode(originalNode.id)}
                onMouseLeave={() => setHoveredNode(null)}
                onClick={() => onNodeClick?.(originalNode)}
              >
                {/* Anomaly Pulse Ring (Critical only) */}
                {hasCriticalAnomaly && (
                  <rect
                    x={-4}
                    y={-4}
                    width={nodeWidth + 8}
                    height={nodeHeight + 8}
                    rx={4}
                    fill="none"
                    stroke="#D13438"
                    strokeWidth="2"
                    opacity="0.6"
                    style={{
                      animation: 'pulse 2s ease-in-out infinite',
                    }}
                  />
                )}

                {/* Node Rectangle */}
                <rect
                  width={nodeWidth}
                  height={nodeHeight}
                  rx={4}
                  fill={
                    viewMode === 'diff' && diffStatus
                      ? diffStatus === 'improved'
                        ? '#107C10'
                        : diffStatus === 'degraded'
                        ? '#D13438'
                        : diffStatus === 'added'
                        ? '#0078D4'
                        : diffStatus === 'removed'
                        ? '#9CA1AB'
                        : originalNode.color || getNodeColor(originalNode.type, false)
                      : originalNode.color || getNodeColor(originalNode.type, false)
                  }
                  stroke={
                    isSelected
                      ? '#0078D4'
                      : hasAnomaly
                      ? hasCriticalAnomaly
                        ? '#D13438'
                        : '#F7630C'
                      : isHovered
                      ? '#626B7B'
                      : 'none'
                  }
                  strokeWidth={isSelected ? 3 : hasAnomaly || isHovered ? 2 : 0}
                  opacity={isHovered || isSelected ? 1 : diffStatus === 'removed' ? 0.3 : 0.85}
                  strokeDasharray={diffStatus === 'removed' ? '4 4' : undefined}
                  style={{ transition: 'all 0.2s ease' }}
                />

                {/* Anomaly Warning Badge */}
                {hasAnomaly && nodeHeight > 15 && (
                  <g transform={`translate(${nodeWidth - 8}, ${-6})`}>
                    <circle
                      r="6"
                      fill={hasCriticalAnomaly ? '#D13438' : hasHighAnomaly ? '#F7630C' : '#FDB913'}
                      stroke="white"
                      strokeWidth="1.5"
                    />
                    <text
                      x="0"
                      y="0"
                      dy="0.3em"
                      fontSize="8px"
                      fontWeight="bold"
                      fill="white"
                      textAnchor="middle"
                      style={{ pointerEvents: 'none', userSelect: 'none' }}
                    >
                      !
                    </text>
                  </g>
                )}

                {/* Node Label */}
                {(isHovered || isSelected || nodeHeight > 20) && (
                  <text
                    x={nodeWidth + 8}
                    y={nodeHeight / 2}
                    dy="0.35em"
                    fontSize="11px"
                    fontWeight={isSelected ? 600 : 500}
                    fill={isSelected || isHovered ? '#1B1F24' : '#626B7B'}
                    style={{ pointerEvents: 'none', userSelect: 'none' }}
                  >
                    {originalNode.label}
                  </text>
                )}

                {/* Type Badge */}
                {isHovered && !hasAnomaly && (
                  <text
                    x={nodeWidth + 8}
                    y={nodeHeight / 2 + 14}
                    fontSize="9px"
                    fontWeight={500}
                    fill="#9CA1AB"
                    style={{ pointerEvents: 'none', userSelect: 'none' }}
                  >
                    {originalNode.type}
                  </text>
                )}

                {/* Anomaly Details (on hover) */}
                {isHovered && hasAnomaly && viewMode !== 'diff' && (
                  <text
                    x={nodeWidth + 8}
                    y={nodeHeight / 2 + 14}
                    fontSize="9px"
                    fontWeight={500}
                    fill={hasCriticalAnomaly ? '#D13438' : '#F7630C'}
                    style={{ pointerEvents: 'none', userSelect: 'none' }}
                  >
                    {nodeAnomalies[0].message}
                  </text>
                )}

                {/* PHASE 3: Confidence Delta Badge (Diff Mode) */}
                {viewMode === 'diff' && nodeDiff && nodeDiff.confidenceDelta !== undefined && Math.abs(nodeDiff.confidenceDelta) > 0.01 && nodeHeight > 15 && (
                  <g transform={`translate(${nodeWidth + 8}, ${nodeHeight / 2 + 14})`}>
                    <text
                      x="0"
                      y="0"
                      fontSize="10px"
                      fontWeight={600}
                      fill={nodeDiff.confidenceDelta > 0 ? '#107C10' : '#D13438'}
                      style={{ pointerEvents: 'none', userSelect: 'none' }}
                    >
                      {nodeDiff.confidenceDelta > 0 ? '+' : ''}
                      {(nodeDiff.confidenceDelta * 100).toFixed(1)}%
                    </text>
                  </g>
                )}
              </g>
            );
          })}
        </g>
      </svg>

      {/* Legends */}
      <div className="absolute bottom-4 left-4 z-10 flex gap-3">
        {/* Node Types Legend - matching mockup */}
        {viewMode !== 'diff' && (
          <div className="bg-white/95 dark:bg-neutral-900/95 backdrop-blur-sm border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg px-3 py-2">
            <div className="text-[10px] font-semibold text-neutral-900 dark:text-neutral-50 mb-2 uppercase tracking-wide">
              Node Types
            </div>
            <div className="space-y-1.5">
              {[
                { color: '#3b82f6', darkColor: '#60a5fa', label: 'Source' },
                { color: '#f59e0b', darkColor: '#fbbf24', label: 'Quality Check' },
                { color: '#8b5cf6', darkColor: '#a78bfa', label: 'Transform' },
                { color: '#10b981', darkColor: '#34d399', label: 'Mapping' },
                { color: '#06b6d4', darkColor: '#22d3ee', label: 'Destination' }
              ].map(item => (
                <div key={item.label} className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm relative">
                    <div
                      className="w-full h-full rounded-sm dark:hidden"
                      style={{ backgroundColor: item.color }}
                    />
                    <div
                      className="w-full h-full rounded-sm hidden dark:block"
                      style={{ backgroundColor: item.darkColor }}
                    />
                  </div>
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">{item.label}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Confidence/Changes Legend */}
        <div className="bg-white/95 dark:bg-neutral-900/95 backdrop-blur-sm border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg px-3 py-2">
          <div className="text-[10px] font-semibold text-neutral-900 dark:text-neutral-50 mb-2 uppercase tracking-wide">
            {viewMode === 'diff' ? 'Changes' : 'Edge Confidence'}
          </div>
          <div className="flex flex-col gap-1.5">
            {viewMode === 'diff' ? (
              <>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#107C10]" />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">Improved</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#D13438]" />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">Degraded</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#0078D4]" />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">Added</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#9CA1AB]" style={{ opacity: 0.3 }} />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">Removed</span>
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#10b981]" />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">High (≥90%)</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#f59e0b]" />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">Medium (70-90%)</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-3 h-3 rounded-sm bg-[#ef4444]" />
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">Low (&lt;70%)</span>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
