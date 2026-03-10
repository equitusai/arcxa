/**
 * Lineage Anomaly Detection Utility
 * Phase 2 feature: Detect quality issues, broken chains, and confidence drops
 */

import type { LineageNode, LineageEdge, LineageGraph } from '@/hooks/useLineageGraph';

// ============================================================================
// Anomaly Types
// ============================================================================

export type AnomalyType =
  | 'confidence_drop' // Sudden drop in confidence between nodes
  | 'low_confidence' // Overall low confidence (<70%)
  | 'broken_chain' // Missing parent/child nodes
  | 'stale_data' // Old timestamp (>30 days)
  | 'missing_metadata' // Missing critical metadata
  | 'circular_dependency'; // Circular reference detected

export interface Anomaly {
  id: string;
  type: AnomalyType;
  severity: 'low' | 'medium' | 'high' | 'critical';
  nodeId?: string;
  edgeId?: string;
  message: string;
  details?: Record<string, any>;
}

export interface AnomalyReport {
  anomalies: Anomaly[];
  summary: {
    total: number;
    critical: number;
    high: number;
    medium: number;
    low: number;
  };
  affectedNodes: Set<string>;
  affectedEdges: Set<string>;
}

// ============================================================================
// Detection Functions
// ============================================================================

/**
 * Detect confidence drops along lineage chains
 * Flag when confidence decreases by >20% between connected nodes
 */
function detectConfidenceDrops(
  nodes: LineageNode[],
  edges: LineageEdge[]
): Anomaly[] {
  const anomalies: Anomaly[] = [];
  const nodeMap = new Map(nodes.map((n) => [n.id, n]));

  edges.forEach((edge) => {
    const sourceNode = nodeMap.get(edge.source);
    const targetNode = nodeMap.get(edge.target);

    if (!sourceNode || !targetNode) return;

    const sourceConf = sourceNode.confidence || 1;
    const targetConf = targetNode.confidence || 1;
    const drop = sourceConf - targetConf;

    if (drop > 0.2) {
      // >20% confidence drop
      anomalies.push({
        id: `conf-drop-${edge.id}`,
        type: 'confidence_drop',
        severity: drop > 0.4 ? 'critical' : drop > 0.3 ? 'high' : 'medium',
        edgeId: edge.id,
        nodeId: targetNode.id,
        message: `Confidence dropped ${(drop * 100).toFixed(0)}% (${(sourceConf * 100).toFixed(0)}% → ${(targetConf * 100).toFixed(0)}%)`,
        details: {
          sourceName: sourceNode.label,
          targetName: targetNode.label,
          sourceConfidence: sourceConf,
          targetConfidence: targetConf,
          drop,
        },
      });
    }
  });

  return anomalies;
}

/**
 * Detect nodes with consistently low confidence
 */
function detectLowConfidence(nodes: LineageNode[]): Anomaly[] {
  const anomalies: Anomaly[] = [];

  nodes.forEach((node) => {
    if (node.confidence === undefined) return;

    if (node.confidence < 0.5) {
      anomalies.push({
        id: `low-conf-${node.id}`,
        type: 'low_confidence',
        severity: 'critical',
        nodeId: node.id,
        message: `Very low confidence: ${(node.confidence * 100).toFixed(0)}%`,
        details: { confidence: node.confidence },
      });
    } else if (node.confidence < 0.7) {
      anomalies.push({
        id: `low-conf-${node.id}`,
        type: 'low_confidence',
        severity: 'high',
        nodeId: node.id,
        message: `Low confidence: ${(node.confidence * 100).toFixed(0)}%`,
        details: { confidence: node.confidence },
      });
    }
  });

  return anomalies;
}

/**
 * Detect broken lineage chains (missing nodes)
 */
function detectBrokenChains(
  nodes: LineageNode[],
  edges: LineageEdge[]
): Anomaly[] {
  const anomalies: Anomaly[] = [];
  const nodeIds = new Set(nodes.map((n) => n.id));

  edges.forEach((edge) => {
    if (!nodeIds.has(edge.source)) {
      anomalies.push({
        id: `broken-source-${edge.id}`,
        type: 'broken_chain',
        severity: 'high',
        edgeId: edge.id,
        message: `Missing source node: ${edge.source}`,
        details: { missingNodeId: edge.source, edgeId: edge.id },
      });
    }

    if (!nodeIds.has(edge.target)) {
      anomalies.push({
        id: `broken-target-${edge.id}`,
        type: 'broken_chain',
        severity: 'high',
        edgeId: edge.id,
        message: `Missing target node: ${edge.target}`,
        details: { missingNodeId: edge.target, edgeId: edge.id },
      });
    }
  });

  return anomalies;
}

/**
 * Detect stale data (old timestamps)
 */
function detectStaleData(nodes: LineageNode[]): Anomaly[] {
  const anomalies: Anomaly[] = [];
  const now = Date.now();
  const thirtyDaysMs = 30 * 24 * 60 * 60 * 1000;
  const ninetyDaysMs = 90 * 24 * 60 * 60 * 1000;

  nodes.forEach((node) => {
    const nodeTime = new Date(node.timestamp).getTime();
    const age = now - nodeTime;

    if (age > ninetyDaysMs) {
      anomalies.push({
        id: `stale-${node.id}`,
        type: 'stale_data',
        severity: 'medium',
        nodeId: node.id,
        message: `Data is ${Math.floor(age / (24 * 60 * 60 * 1000))} days old`,
        details: { timestamp: node.timestamp, ageMs: age },
      });
    } else if (age > thirtyDaysMs) {
      anomalies.push({
        id: `stale-${node.id}`,
        type: 'stale_data',
        severity: 'low',
        nodeId: node.id,
        message: `Data is ${Math.floor(age / (24 * 60 * 60 * 1000))} days old`,
        details: { timestamp: node.timestamp, ageMs: age },
      });
    }
  });

  return anomalies;
}

/**
 * Detect missing critical metadata
 */
function detectMissingMetadata(nodes: LineageNode[]): Anomaly[] {
  const anomalies: Anomaly[] = [];

  nodes.forEach((node) => {
    // Check for nodes with model_id but no confidence
    if (node.modelId && node.confidence === undefined) {
      anomalies.push({
        id: `missing-meta-${node.id}`,
        type: 'missing_metadata',
        severity: 'medium',
        nodeId: node.id,
        message: 'Model-generated data missing confidence score',
        details: { modelId: node.modelId },
      });
    }

    // Check for record nodes missing dataset
    if (node.type === 'record' && !node.dataset) {
      anomalies.push({
        id: `missing-meta-${node.id}`,
        type: 'missing_metadata',
        severity: 'low',
        nodeId: node.id,
        message: 'Record missing dataset information',
      });
    }
  });

  return anomalies;
}

/**
 * Detect circular dependencies using cycle detection
 */
function detectCircularDependencies(
  nodes: LineageNode[],
  edges: LineageEdge[]
): Anomaly[] {
  const anomalies: Anomaly[] = [];
  const adjacencyList = new Map<string, string[]>();

  // Build adjacency list
  edges.forEach((edge) => {
    if (!adjacencyList.has(edge.source)) {
      adjacencyList.set(edge.source, []);
    }
    adjacencyList.get(edge.source)!.push(edge.target);
  });

  // DFS-based cycle detection
  const visited = new Set<string>();
  const recursionStack = new Set<string>();
  const cycles: string[][] = [];

  function dfs(nodeId: string, path: string[]): void {
    visited.add(nodeId);
    recursionStack.add(nodeId);
    path.push(nodeId);

    const neighbors = adjacencyList.get(nodeId) || [];
    for (const neighbor of neighbors) {
      if (!visited.has(neighbor)) {
        dfs(neighbor, path);
      } else if (recursionStack.has(neighbor)) {
        // Cycle detected
        const cycleStart = path.indexOf(neighbor);
        const cycle = path.slice(cycleStart);
        cycles.push(cycle);
      }
    }

    recursionStack.delete(nodeId);
    path.pop();
  }

  nodes.forEach((node) => {
    if (!visited.has(node.id)) {
      dfs(node.id, []);
    }
  });

  // Create anomalies for detected cycles
  cycles.forEach((cycle, idx) => {
    anomalies.push({
      id: `circular-${idx}`,
      type: 'circular_dependency',
      severity: 'critical',
      message: `Circular dependency detected: ${cycle.length} nodes in cycle`,
      details: { cycle },
    });
  });

  return anomalies;
}

// ============================================================================
// Main Anomaly Detection Function
// ============================================================================

/**
 * Analyze entire lineage graph for anomalies
 */
export function detectLineageAnomalies(graph: LineageGraph): AnomalyReport {
  const allAnomalies: Anomaly[] = [];

  // Run all detection functions
  allAnomalies.push(...detectConfidenceDrops(graph.nodes, graph.edges));
  allAnomalies.push(...detectLowConfidence(graph.nodes));
  allAnomalies.push(...detectBrokenChains(graph.nodes, graph.edges));
  allAnomalies.push(...detectStaleData(graph.nodes));
  allAnomalies.push(...detectMissingMetadata(graph.nodes));
  allAnomalies.push(...detectCircularDependencies(graph.nodes, graph.edges));

  // Calculate summary
  const summary = {
    total: allAnomalies.length,
    critical: allAnomalies.filter((a) => a.severity === 'critical').length,
    high: allAnomalies.filter((a) => a.severity === 'high').length,
    medium: allAnomalies.filter((a) => a.severity === 'medium').length,
    low: allAnomalies.filter((a) => a.severity === 'low').length,
  };

  // Track affected nodes and edges
  const affectedNodes = new Set<string>();
  const affectedEdges = new Set<string>();

  allAnomalies.forEach((anomaly) => {
    if (anomaly.nodeId) affectedNodes.add(anomaly.nodeId);
    if (anomaly.edgeId) affectedEdges.add(anomaly.edgeId);
  });

  return {
    anomalies: allAnomalies,
    summary,
    affectedNodes,
    affectedEdges,
  };
}

/**
 * Get anomalies for a specific node
 */
export function getNodeAnomalies(
  nodeId: string,
  report: AnomalyReport
): Anomaly[] {
  return report.anomalies.filter((a) => a.nodeId === nodeId);
}

/**
 * Get anomalies for a specific edge
 */
export function getEdgeAnomalies(
  edgeId: string,
  report: AnomalyReport
): Anomaly[] {
  return report.anomalies.filter((a) => a.edgeId === edgeId);
}

/**
 * Get severity color for UI rendering
 */
export function getAnomalySeverityColor(severity: Anomaly['severity']): string {
  switch (severity) {
    case 'critical':
      return '#D13438'; // Red
    case 'high':
      return '#F7630C'; // Orange
    case 'medium':
      return '#FDB913'; // Amber
    case 'low':
      return '#8764B8'; // Purple
  }
}
