/**
 * Workflow Execution Hook
 * Handles workflow execution and real-time visualization
 */

import { useState, useCallback } from 'react';
import { Node } from 'reactflow';
import { useExecuteWorkflow } from './useWorkflows';
import type { WorkflowExecutionRequest, WorkflowExecutionResult, StepResult } from '@/api/types';

interface ExecutionState {
  isExecuting: boolean;
  result: WorkflowExecutionResult | null;
  currentStepId: string | null;
  completedSteps: Set<string>;
  error: Error | null;
  executionId: string | null; // Track execution ID for cancellation
}

export function useWorkflowExecution(
  workflowId: string,
  setNodes: (updater: (nodes: Node[]) => Node[]) => void
) {
  const [state, setState] = useState<ExecutionState>({
    isExecuting: false,
    result: null,
    currentStepId: null,
    completedSteps: new Set(),
    error: null,
    executionId: null,
  });

  const executeWorkflow = useExecuteWorkflow();

  // Update node execution status
  const updateNodeStatus = useCallback(
    (
      nodeId: string,
      status: 'idle' | 'executing' | 'success' | 'error',
      stepResult?: StepResult
    ) => {
      setNodes(nodes =>
        nodes.map(node =>
          node.id === nodeId
            ? {
                ...node,
                data: {
                  ...node.data,
                  executionStatus: status,
                  executionConfidence: stepResult?.confidence,
                  executionDuration: stepResult?.duration_ms,
                },
              }
            : node
        )
      );
    },
    [setNodes]
  );

  // Reset all nodes to idle
  const resetNodeStatuses = useCallback(() => {
    setNodes(nodes =>
      nodes.map(node => ({
        ...node,
        data: {
          ...node.data,
          executionStatus: 'idle',
          executionConfidence: undefined,
          executionDuration: undefined,
        },
      }))
    );
  }, [setNodes]);

  // Execute workflow with visualization
  const execute = useCallback(
    async (request: WorkflowExecutionRequest) => {
      if (state.isExecuting) return;

      // Reset state
      setState({
        isExecuting: true,
        result: null,
        currentStepId: null,
        completedSteps: new Set(),
        error: null,
        executionId: null,
      });
      resetNodeStatuses();

      try {
        const result = await executeWorkflow.mutateAsync({
          workflowId,
          request,
        });

        // Store execution ID for cancellation
        setState(prev => ({
          ...prev,
          executionId: result.execution_id,
        }));

        // Animate through steps
        if (result.step_results) {
          for (let i = 0; i < result.step_results.length; i++) {
            const stepResult = result.step_results[i];

            // Show as executing
            setState(prev => ({
              ...prev,
              currentStepId: stepResult.step_id,
            }));
            updateNodeStatus(stepResult.step_id, 'executing');

            // Wait for animation (simulated execution time)
            await new Promise(resolve => setTimeout(resolve, 500));

            // Show result
            const status = stepResult.success ? 'success' : 'error';
            updateNodeStatus(stepResult.step_id, status, stepResult);

            setState(prev => {
              const newCompleted = new Set(prev.completedSteps);
              newCompleted.add(stepResult.step_id);
              return {
                ...prev,
                completedSteps: newCompleted,
                currentStepId: null,
              };
            });

            // Brief pause between steps
            await new Promise(resolve => setTimeout(resolve, 200));
          }
        }

        setState(prev => ({
          ...prev,
          isExecuting: false,
          result,
        }));

        return result;
      } catch (error) {
        setState(prev => ({
          ...prev,
          isExecuting: false,
          error: error as Error,
          executionId: null, // Clear execution ID on error
        }));
        resetNodeStatuses();
        throw error;
      }
    },
    [workflowId, state.isExecuting, executeWorkflow, updateNodeStatus, resetNodeStatuses]
  );

  // Reset execution state
  const reset = useCallback(() => {
    setState({
      isExecuting: false,
      result: null,
      currentStepId: null,
      completedSteps: new Set(),
      error: null,
      executionId: null,
    });
    resetNodeStatuses();
  }, [resetNodeStatuses]);

  return {
    ...state,
    execute,
    reset,
  };
}
