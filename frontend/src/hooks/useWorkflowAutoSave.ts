/**
 * Workflow Auto-save Hook
 * Automatically saves workflow changes with debouncing
 */

import { useState, useEffect, useMemo, useRef } from 'react';
import { Node, Edge } from 'reactflow';
import { useUpdateWorkflow } from './useWorkflows';
import type { WorkflowDefinition } from '@/api/types';

interface UseWorkflowAutoSaveOptions {
  workflowId: string;
  workflowName: string;
  enabled?: boolean;
  debounceMs?: number;
}

interface AutoSaveState {
  isSaving: boolean;
  lastSaved: Date | null;
  error: Error | null;
}

export function useWorkflowAutoSave(
  nodes: Node[],
  edges: Edge[],
  options: UseWorkflowAutoSaveOptions
) {
  const {
    workflowId,
    workflowName,
    enabled = true,
    debounceMs = 3000,
  } = options;

  const [state, setState] = useState<AutoSaveState>({
    isSaving: false,
    lastSaved: null,
    error: null,
  });

  const updateWorkflow = useUpdateWorkflow();
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);
  const lastSavedDataRef = useRef<string>('');

  // Convert React Flow state to WorkflowDefinition
  const workflowDefinition = useMemo((): WorkflowDefinition => {
    return {
      steps: nodes.map(node => ({
        id: node.id,
        step_type: node.data.step_type,
        config: node.data.config || {},
        depends_on: edges
          .filter(edge => edge.target === node.id)
          .map(edge => edge.source),
      })),
      fusion_threshold: 0.8, // Default, can be made configurable
      fallback: 'manual_review', // Default, can be made configurable
    };
  }, [nodes, edges]);

  // Save function
  const save = async (force = false) => {
    if (!enabled && !force) return;
    if (!workflowId) return;

    // Check if data has actually changed
    const currentData = JSON.stringify({ nodes, edges });
    if (!force && currentData === lastSavedDataRef.current) {
      return;
    }

    setState(prev => ({ ...prev, isSaving: true, error: null }));

    try {
      await updateWorkflow.mutateAsync({
        workflowId,
        request: {
          name: workflowName,
          definition: workflowDefinition,
        },
      });

      lastSavedDataRef.current = currentData;
      setState({
        isSaving: false,
        lastSaved: new Date(),
        error: null,
      });
    } catch (error) {
      setState(prev => ({
        ...prev,
        isSaving: false,
        error: error as Error,
      }));
    }
  };

  // Debounced auto-save effect
  useEffect(() => {
    if (!enabled || nodes.length === 0) return;

    // Clear existing timeout
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    // Set new timeout
    timeoutRef.current = setTimeout(() => {
      save();
    }, debounceMs);

    // Cleanup
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, [nodes, edges, enabled, debounceMs]);

  return {
    ...state,
    save: () => save(true), // Force save
  };
}
