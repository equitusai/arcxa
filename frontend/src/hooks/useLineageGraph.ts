/**
 * useLineageGraph Hook
 * Transforms LineageEvent data into graph structure for Sankey visualization
 * Supports field-level isolation, confidence filtering, and temporal navigation
 */

import { useMemo } from 'react';
import type { LineageEvent } from '@/api/types';

// ============================================================================
// Graph Types
// ============================================================================

export interface LineageNode {
  id: string;
  label: string;
  type: 'datasource' | 'dataset' | 'record' | 'field' | 'model';
  recordId?: string;
  dataset?: string;
  modelId?: string;
  confidence?: number;
  timestamp: string;
  metadata?: Record<string, any>;
  // Visual properties
  color?: string;
  size?: number;
}

export interface LineageEdge {
  id: string;
  source: string;
  target: string;
  label?: string;
  operation: string; // CREATE, UPDATE, DELETE, READ, DERIVE
  confidence?: number;
  timestamp: string;
  modelId?: string;
  metadata?: Record<string, any>;
  // Visual properties
  value: number; // thickness for Sankey (required by D3)
  color?: string;
}

export interface LineageGraph {
  nodes: LineageNode[];
  edges: LineageEdge[];
  metadata: {
    totalEvents: number;
    dateRange: { start: string; end: string };
    datasets: Set<string>;
    models: Set<string>;
  };
}

// ============================================================================
// Filter Types
// ============================================================================

export interface LineageFilters {
  selectedField?: string; // Field-level isolation (Phase 2 feature)
  confidenceRange?: [number, number];
  timeRange?: [string, string]; // ISO timestamps
  selectedDatasets?: string[];
  selectedModels?: string[];
  focusRecordId?: string;
}

// ============================================================================
// Hook Interface
// ============================================================================

export interface UseLineageGraphOptions {
  events: LineageEvent[];
  filters?: LineageFilters;
  enableFieldIsolation?: boolean;
}

export interface UseLineageGraphResult {
  graph: LineageGraph;
  filteredGraph: LineageGraph;
  isFiltered: boolean;
  stats: {
    totalNodes: number;
    totalEdges: number;
    filteredNodes: number;
    filteredEdges: number;
    confidenceAvg: number;
  };
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Generate unique node ID from event data
 */
function generateNodeId(event: LineageEvent, suffix?: string): string {
  const base = `${event.dataset}:${event.record_id}`;
  return suffix ? `${base}:${suffix}` : base;
}

/**
 * Calculate confidence color based on score
 */
function getConfidenceColor(confidence?: number): string {
  if (!confidence) return '#9CA1AB'; // Gray for unknown
  if (confidence >= 0.9) return '#107C10'; // Green - high confidence
  if (confidence >= 0.7) return '#FDB913'; // Amber - medium confidence
  return '#D13438'; // Red - low confidence
}

/**
 * Build graph from lineage events
 */
function buildGraph(events: LineageEvent[]): LineageGraph {
  const nodes: LineageNode[] = [];
  const edges: LineageEdge[] = [];
  const nodeMap = new Map<string, LineageNode>();
  const datasets = new Set<string>();
  const models = new Set<string>();

  let minTimestamp = events[0]?.timestamp || new Date().toISOString();
  let maxTimestamp = events[0]?.timestamp || new Date().toISOString();

  // Process each event
  events.forEach((event, idx) => {
    const nodeId = generateNodeId(event);
    datasets.add(event.dataset);

    if (event.model_id) {
      models.add(event.model_id);
    }

    // Track date range
    if (event.timestamp < minTimestamp) minTimestamp = event.timestamp;
    if (event.timestamp > maxTimestamp) maxTimestamp = event.timestamp;

    // Create or update node for this record
    if (!nodeMap.has(nodeId)) {
      const node: LineageNode = {
        id: nodeId,
        label: `${event.dataset} • ${event.record_id.substring(0, 8)}`,
        type: 'record',
        recordId: event.record_id,
        dataset: event.dataset,
        timestamp: event.timestamp,
        metadata: event.metadata,
        confidence: event.metadata?.confidence as number | undefined,
        color: getConfidenceColor(event.metadata?.confidence as number | undefined),
        size: 1,
      };
      nodeMap.set(nodeId, node);
      nodes.push(node);
    }

    // Create model node if present
    if (event.model_id) {
      const modelNodeId = `model:${event.model_id}`;
      if (!nodeMap.has(modelNodeId)) {
        const modelNode: LineageNode = {
          id: modelNodeId,
          label: event.model_id,
          type: 'model',
          modelId: event.model_id,
          timestamp: event.timestamp,
          color: '#8764B8', // Purple for models
          size: 2,
        };
        nodeMap.set(modelNodeId, modelNode);
        nodes.push(modelNode);
      }

      // Create edge from model to record (for derived data)
      if (event.operation === 'CREATE' || event.operation === 'UPDATE') {
        edges.push({
          id: `model-edge-${idx}`,
          source: modelNodeId,
          target: nodeId,
          operation: 'DERIVE',
          label: event.model_version ? `v${event.model_version}` : undefined,
          confidence: event.metadata?.confidence as number | undefined,
          timestamp: event.timestamp,
          modelId: event.model_id,
          metadata: event.metadata,
          value: 1,
          color: getConfidenceColor(event.metadata?.confidence as number | undefined),
        });
      }
    }

    // Create edges from parent records
    if (event.parent_record_ids && event.parent_record_ids.length > 0) {
      event.parent_record_ids.forEach((parentId, pIdx) => {
        const parentNodeId = `${event.dataset}:${parentId}`;

        // Create parent node if it doesn't exist
        if (!nodeMap.has(parentNodeId)) {
          const parentNode: LineageNode = {
            id: parentNodeId,
            label: `${event.dataset} • ${parentId.substring(0, 8)}`,
            type: 'record',
            recordId: parentId,
            dataset: event.dataset,
            timestamp: event.timestamp,
            color: '#9CA1AB',
            size: 1,
          };
          nodeMap.set(parentNodeId, parentNode);
          nodes.push(parentNode);
        }

        // Create edge from parent to child
        edges.push({
          id: `parent-edge-${idx}-${pIdx}`,
          source: parentNodeId,
          target: nodeId,
          operation: event.operation,
          confidence: event.metadata?.confidence as number | undefined,
          timestamp: event.timestamp,
          metadata: event.metadata,
          value: 1,
          color: getConfidenceColor(event.metadata?.confidence as number | undefined),
        });
      });
    }

    // If no parents and no model, create dataset source node
    if ((!event.parent_record_ids || event.parent_record_ids.length === 0) && !event.model_id && event.operation === 'CREATE') {
      const datasetNodeId = `dataset:${event.dataset}`;
      if (!nodeMap.has(datasetNodeId)) {
        const datasetNode: LineageNode = {
          id: datasetNodeId,
          label: event.dataset,
          type: 'dataset',
          dataset: event.dataset,
          timestamp: event.timestamp,
          color: '#0078D4', // Blue for datasets
          size: 3,
        };
        nodeMap.set(datasetNodeId, datasetNode);
        nodes.push(datasetNode);
      }

      // Create edge from dataset to record
      edges.push({
        id: `dataset-edge-${idx}`,
        source: datasetNodeId,
        target: nodeId,
        operation: 'INGEST',
        timestamp: event.timestamp,
        value: 1,
        color: '#9CA1AB',
      });
    }
  });

  return {
    nodes,
    edges,
    metadata: {
      totalEvents: events.length,
      dateRange: { start: minTimestamp, end: maxTimestamp },
      datasets,
      models,
    },
  };
}

/**
 * Apply filters to graph using BFS traversal for field isolation
 */
function applyFilters(graph: LineageGraph, filters: LineageFilters): LineageGraph {
  if (!filters || Object.keys(filters).length === 0) {
    return graph;
  }

  let filteredNodes = [...graph.nodes];
  let filteredEdges = [...graph.edges];

  // PHASE 2 FEATURE: Field-Level Isolation
  // Filter to show only nodes/edges that touch the selected field
  if (filters.selectedField) {
    const fieldName = filters.selectedField;
    const relevantNodeIds = new Set<string>();

    // Find nodes with this field in metadata
    filteredNodes.forEach((node) => {
      if (!node.metadata) return;

      let hasField = false;

      // Check various metadata locations
      if (node.metadata.fields && Array.isArray(node.metadata.fields)) {
        hasField = node.metadata.fields.includes(fieldName);
      }
      if (!hasField && node.metadata.attributes && typeof node.metadata.attributes === 'object') {
        hasField = fieldName in node.metadata.attributes;
      }
      if (!hasField && node.metadata.schema && Array.isArray(node.metadata.schema)) {
        hasField = node.metadata.schema.some((s: any) => s.name === fieldName || s.field_name === fieldName);
      }

      if (hasField) {
        relevantNodeIds.add(node.id);
      }
    });

    // Find edges that transform this field
    filteredEdges.forEach((edge) => {
      if (!edge.metadata) return;

      let hasField = false;

      if (edge.metadata.source_field === fieldName || edge.metadata.target_field === fieldName) {
        hasField = true;
      }
      if (!hasField && edge.metadata.transformed_fields && Array.isArray(edge.metadata.transformed_fields)) {
        hasField = edge.metadata.transformed_fields.includes(fieldName);
      }

      if (hasField) {
        // Mark both source and target nodes as relevant
        relevantNodeIds.add(edge.source);
        relevantNodeIds.add(edge.target);
      }
    });

    // BFS traversal to include connected nodes (lineage chain)
    const queue = Array.from(relevantNodeIds);
    const visited = new Set(relevantNodeIds);

    while (queue.length > 0) {
      const currentId = queue.shift()!;

      // Find connected edges
      filteredEdges.forEach((edge) => {
        if (edge.source === currentId && !visited.has(edge.target)) {
          visited.add(edge.target);
          queue.push(edge.target);
        }
        if (edge.target === currentId && !visited.has(edge.source)) {
          visited.add(edge.source);
          queue.push(edge.source);
        }
      });
    }

    // Apply the filter
    filteredNodes = filteredNodes.filter((node) => visited.has(node.id));
    filteredEdges = filteredEdges.filter(
      (edge) => visited.has(edge.source) && visited.has(edge.target)
    );
  }

  // Filter by confidence range
  if (filters.confidenceRange) {
    const [min, max] = filters.confidenceRange;
    filteredNodes = filteredNodes.filter(
      (node) => !node.confidence || (node.confidence >= min && node.confidence <= max)
    );
    filteredEdges = filteredEdges.filter(
      (edge) => !edge.confidence || (edge.confidence >= min && edge.confidence <= max)
    );
  }

  // Filter by time range
  if (filters.timeRange) {
    const [start, end] = filters.timeRange;
    filteredNodes = filteredNodes.filter(
      (node) => node.timestamp >= start && node.timestamp <= end
    );
    filteredEdges = filteredEdges.filter(
      (edge) => edge.timestamp >= start && edge.timestamp <= end
    );
  }

  // Filter by datasets
  if (filters.selectedDatasets && filters.selectedDatasets.length > 0) {
    filteredNodes = filteredNodes.filter(
      (node) => !node.dataset || filters.selectedDatasets!.includes(node.dataset)
    );
  }

  // Filter by models
  if (filters.selectedModels && filters.selectedModels.length > 0) {
    filteredNodes = filteredNodes.filter(
      (node) => !node.modelId || filters.selectedModels!.includes(node.modelId)
    );
    filteredEdges = filteredEdges.filter(
      (edge) => !edge.modelId || filters.selectedModels!.includes(edge.modelId)
    );
  }

  // Focus on specific record (BFS traversal)
  if (filters.focusRecordId) {
    const connectedNodeIds = new Set<string>();
    const queue: string[] = [];

    // Find starting nodes matching the focus record
    filteredNodes.forEach((node) => {
      if (node.recordId === filters.focusRecordId) {
        connectedNodeIds.add(node.id);
        queue.push(node.id);
      }
    });

    // BFS traversal
    while (queue.length > 0) {
      const currentId = queue.shift()!;

      // Find all connected edges
      filteredEdges.forEach((edge) => {
        if (edge.source === currentId && !connectedNodeIds.has(edge.target)) {
          connectedNodeIds.add(edge.target);
          queue.push(edge.target);
        }
        if (edge.target === currentId && !connectedNodeIds.has(edge.source)) {
          connectedNodeIds.add(edge.source);
          queue.push(edge.source);
        }
      });
    }

    filteredNodes = filteredNodes.filter((node) => connectedNodeIds.has(node.id));
    filteredEdges = filteredEdges.filter(
      (edge) => connectedNodeIds.has(edge.source) && connectedNodeIds.has(edge.target)
    );
  }

  // Remove orphaned edges (edges with missing nodes)
  const nodeIds = new Set(filteredNodes.map((n) => n.id));
  filteredEdges = filteredEdges.filter(
    (edge) => nodeIds.has(edge.source) && nodeIds.has(edge.target)
  );

  // Remove orphaned nodes (nodes with no edges)
  const connectedNodeIds = new Set<string>();
  filteredEdges.forEach((edge) => {
    connectedNodeIds.add(edge.source);
    connectedNodeIds.add(edge.target);
  });
  filteredNodes = filteredNodes.filter((node) => connectedNodeIds.has(node.id));

  return {
    nodes: filteredNodes,
    edges: filteredEdges,
    metadata: graph.metadata,
  };
}

/**
 * Calculate graph statistics
 */
function calculateStats(fullGraph: LineageGraph, filteredGraph: LineageGraph) {
  const allConfidences = [
    ...fullGraph.nodes.map((n) => n.confidence).filter((c) => c !== undefined),
    ...fullGraph.edges.map((e) => e.confidence).filter((c) => c !== undefined),
  ] as number[];

  const confidenceAvg =
    allConfidences.length > 0
      ? allConfidences.reduce((sum, c) => sum + c, 0) / allConfidences.length
      : 0;

  return {
    totalNodes: fullGraph.nodes.length,
    totalEdges: fullGraph.edges.length,
    filteredNodes: filteredGraph.nodes.length,
    filteredEdges: filteredGraph.edges.length,
    confidenceAvg,
  };
}

// ============================================================================
// Main Hook
// ============================================================================

export function useLineageGraph({
  events,
  filters,
  enableFieldIsolation = false,
}: UseLineageGraphOptions): UseLineageGraphResult {
  // Build full graph from events
  const graph = useMemo(() => {
    if (!events || events.length === 0) {
      return {
        nodes: [],
        edges: [],
        metadata: {
          totalEvents: 0,
          dateRange: { start: '', end: '' },
          datasets: new Set<string>(),
          models: new Set<string>(),
        },
      };
    }
    return buildGraph(events);
  }, [events]);

  // Apply filters to create filtered graph
  const filteredGraph = useMemo(() => {
    if (!filters) return graph;
    return applyFilters(graph, filters);
  }, [graph, filters]);

  // Calculate statistics
  const stats = useMemo(() => {
    return calculateStats(graph, filteredGraph);
  }, [graph, filteredGraph]);

  const isFiltered = useMemo(() => {
    if (!filters) return false;
    return (
      Boolean(filters.selectedField) ||
      Boolean(filters.confidenceRange) ||
      Boolean(filters.timeRange) ||
      Boolean(filters.selectedDatasets?.length) ||
      Boolean(filters.selectedModels?.length) ||
      Boolean(filters.focusRecordId)
    );
  }, [filters]);

  return {
    graph,
    filteredGraph,
    isFiltered,
    stats,
  };
}
