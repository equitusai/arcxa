/**
 * Workflow Validation Utilities
 * DAG validation, cycle detection, and step compatibility checks
 */

import { Node, Edge, Connection } from 'reactflow';
import { toast } from 'sonner';
import type { StepType } from '@/api/types';

/**
 * Check if adding a connection would create a cycle in the DAG
 */
export function wouldCreateCycle(
  sourceId: string,
  targetId: string,
  edges: Edge[]
): boolean {
  // If connecting to itself, that's a cycle
  if (sourceId === targetId) {
    return true;
  }

  // Build adjacency list from current edges
  const adjacencyList = new Map<string, string[]>();
  edges.forEach(edge => {
    if (!adjacencyList.has(edge.source)) {
      adjacencyList.set(edge.source, []);
    }
    adjacencyList.get(edge.source)!.push(edge.target);
  });

  // Check if there's already a path from target to source
  // If yes, adding source→target would create a cycle
  const visited = new Set<string>();
  const queue: string[] = [targetId];

  while (queue.length > 0) {
    const current = queue.shift()!;

    if (current === sourceId) {
      // Found a path from target back to source - adding this edge would create a cycle
      return true;
    }

    if (visited.has(current)) {
      continue;
    }

    visited.add(current);
    const neighbors = adjacencyList.get(current) || [];
    queue.push(...neighbors);
  }

  return false;
}

/**
 * Check if two step types are compatible for connection
 * This can be customized based on your workflow rules
 */
export function areStepTypesCompatible(
  sourceType: StepType,
  targetType: StepType
): boolean {
  // For now, allow all connections
  // You can add specific rules here, e.g.:
  // - Aggregation steps can only follow prediction/logic steps
  // - Confidence gates must have an input
  return true;
}

/**
 * Validate a connection before adding it to the workflow
 */
export function validateConnection(
  connection: Connection,
  nodes: Node[],
  edges: Edge[]
): { valid: boolean; error?: string } {
  if (!connection.source || !connection.target) {
    return { valid: false, error: 'Invalid connection' };
  }

  // Check for self-loop
  if (connection.source === connection.target) {
    return { valid: false, error: 'Cannot connect a node to itself' };
  }

  // Check for duplicate connection (but allow multiple from conditional nodes with different handles)
  const sourceNode = nodes.find(n => n.id === connection.source);
  const isConditionalSource = sourceNode?.type === 'conditional';

  const isDuplicate = edges.some(
    edge =>
      edge.source === connection.source &&
      edge.target === connection.target &&
      // For conditional nodes, only consider duplicate if same handle is used
      (!isConditionalSource || edge.sourceHandle === connection.sourceHandle)
  );

  if (isDuplicate) {
    return { valid: false, error: 'Connection already exists' };
  }

  // Check conditional node outgoing connection limits
  if (isConditionalSource) {
    const handleId = connection.sourceHandle;
    const existingConnectionsForHandle = edges.filter(
      e => e.source === connection.source && e.sourceHandle === handleId
    );

    if (existingConnectionsForHandle.length >= 1) {
      const branchName = handleId === 'true' ? 'TRUE' : handleId === 'false' ? 'FALSE' : handleId;
      return {
        valid: false,
        error: `Conditional router ${branchName} branch can only connect to one step`,
      };
    }
  }

  // Check for cycles (DAG validation)
  if (wouldCreateCycle(connection.source, connection.target, edges)) {
    return {
      valid: false,
      error: 'Cannot create cycles - workflows must be directed acyclic graphs (DAGs)',
    };
  }

  // Check step type compatibility
  const targetNode = nodes.find(n => n.id === connection.target);

  if (sourceNode && targetNode) {
    const sourceType = sourceNode.data?.step_type;
    const targetType = targetNode.data?.step_type;

    if (sourceType && targetType) {
      if (!areStepTypesCompatible(sourceType, targetType)) {
        return {
          valid: false,
          error: `${sourceNode.data.label} cannot connect to ${targetNode.data.label}`,
        };
      }
    }
  }

  return { valid: true };
}

/**
 * Validate entire workflow definition
 */
export function validateWorkflow(
  nodes: Node[],
  edges: Edge[],
  options?: {
    includeNodeValidationErrors?: boolean;
  }
): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  const includeNodeValidationErrors = options?.includeNodeValidationErrors ?? true;

  // Check if workflow has at least one node
  if (nodes.length === 0) {
    errors.push('Workflow must have at least one step');
  }

  // Check for isolated nodes (no connections)
  if (nodes.length > 1) {
    const connectedNodeIds = new Set<string>();
    edges.forEach(edge => {
      connectedNodeIds.add(edge.source);
      connectedNodeIds.add(edge.target);
    });

    const isolatedNodes = nodes.filter(node => !connectedNodeIds.has(node.id));
    if (isolatedNodes.length > 0) {
      errors.push(
        `Found ${isolatedNodes.length} isolated node(s): ${isolatedNodes
          .map(n => n.data?.label || n.id)
          .join(', ')}`
      );
    }
  }

  // Check for cycles
  const hasCycle = detectCycles(nodes, edges);
  if (hasCycle) {
    errors.push('Workflow contains cycles - must be a directed acyclic graph (DAG)');
  }

  // Check each node has required configuration
  nodes.forEach(node => {
    if (!node.data?.step_type) {
      errors.push(`Node ${node.data?.label || node.id} is missing step type`);
    }

    // Add step-specific validation here
    if (includeNodeValidationErrors && node.data?.validationError) {
      errors.push(`Node ${node.data.label}: ${node.data.validationError}`);
    }

    // Conditional router specific validation
    if (node.data?.step_type === 'conditional_router' || node.type === 'conditional') {
      const nodeLabel = node.data?.label || node.id;

      // Check if condition expression is configured
      if (!node.data?.config?.condition || node.data.config.condition.trim() === '') {
        errors.push(`Conditional router "${nodeLabel}" must have a condition expression`);
      }

      // Check if it has at least one outgoing connection
      const outgoingEdges = edges.filter(e => e.source === node.id);
      if (outgoingEdges.length === 0) {
        errors.push(`Conditional router "${nodeLabel}" must have at least one branch connection`);
      }

      // Optionally warn if only one branch is connected (not an error, but informational)
      const trueEdges = edges.filter(e => e.source === node.id && e.sourceHandle === 'true');
      const falseEdges = edges.filter(e => e.source === node.id && e.sourceHandle === 'false');

      if (trueEdges.length === 0 && falseEdges.length > 0) {
        // Only FALSE branch connected - this is valid but uncommon
      }
      if (falseEdges.length === 0 && trueEdges.length > 0) {
        // Only TRUE branch connected - this is valid but uncommon
      }
    }

    // Field mapper specific validation
    if (node.data?.step_type === 'field_mapper') {
      const nodeLabel = node.data?.label || node.id;

      if (!node.data?.config?.target_field || node.data.config.target_field.trim() === '') {
        errors.push(`Field mapper "${nodeLabel}" must have a target ontology field`);
      }
    }
  });

  return {
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Detect if there are any cycles in the graph
 */
function detectCycles(nodes: Node[], edges: Edge[]): boolean {
  // Build adjacency list
  const adjacencyList = new Map<string, string[]>();
  nodes.forEach(node => {
    adjacencyList.set(node.id, []);
  });
  edges.forEach(edge => {
    adjacencyList.get(edge.source)?.push(edge.target);
  });

  // DFS with cycle detection
  const visited = new Set<string>();
  const recursionStack = new Set<string>();

  function dfs(nodeId: string): boolean {
    visited.add(nodeId);
    recursionStack.add(nodeId);

    const neighbors = adjacencyList.get(nodeId) || [];
    for (const neighbor of neighbors) {
      if (!visited.has(neighbor)) {
        if (dfs(neighbor)) {
          return true;
        }
      } else if (recursionStack.has(neighbor)) {
        // Found a back edge - cycle detected
        return true;
      }
    }

    recursionStack.delete(nodeId);
    return false;
  }

  // Check from each node
  for (const node of nodes) {
    if (!visited.has(node.id)) {
      if (dfs(node.id)) {
        return true;
      }
    }
  }

  return false;
}

/**
 * Show validation error toast
 */
export function showValidationError(error: string) {
  toast.error(error, {
    duration: 3000,
  });
}

/**
 * Show validation success toast
 */
export function showValidationSuccess(message: string = 'Workflow is valid') {
  toast.success(message, {
    duration: 2000,
  });
}
