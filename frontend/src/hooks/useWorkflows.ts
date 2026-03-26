/**
 * Workflow Orchestration React Query hooks
 *
 * Provides hooks for workflow management, execution, and monitoring
 */

import { useQuery, useMutation, useQueryClient, useInfiniteQuery } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as workflowApi from '@/api/workflows';
import {
  RegisterWorkflowRequest,
  ValidateWorkflowResponse,
  WorkflowDefinition,
  WorkflowExecutionRequest,
  PaginationParams,
} from '@/api/types';

/**
 * List workflows query hook
 *
 * Fetches all registered workflows
 *
 * @param params - Pagination parameters (optional)
 * @example
 * const { data: workflows, isLoading } = useWorkflows();
 */
export function useWorkflows(params?: PaginationParams) {
  return useQuery({
    queryKey: ['workflows', params],
    queryFn: () => workflowApi.listWorkflows(params),
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get workflow by ID query hook
 *
 * Fetches a specific workflow definition
 *
 * @param workflowId - Workflow ID
 * @param enabled - Whether to enable the query (default: true if workflowId exists)
 * @example
 * const { data: workflow } = useWorkflow('address_merge_v1');
 */
export function useWorkflow(workflowId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['workflows', workflowId],
    queryFn: () => workflowApi.getWorkflow(workflowId!),
    enabled: enabled && !!workflowId,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}

/**
 * Register workflow mutation hook
 *
 * Creates a new workflow definition
 *
 * @example
 * const registerWorkflow = useRegisterWorkflow();
 * registerWorkflow.mutate({
 *   id: 'my_workflow',
 *   name: 'My Workflow',
 *   definition: { steps: [...], fusion_threshold: 0.9, fallback: 'manual' }
 * });
 */
export function useRegisterWorkflow() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: RegisterWorkflowRequest) => workflowApi.registerWorkflow(request),
    onSuccess: (data) => {
      // Invalidate workflows list
      queryClient.invalidateQueries({ queryKey: ['workflows'] });

      // Set the new workflow in cache
      queryClient.setQueryData(['workflows', data.id], data);

      // Show success toast
      toast.success(`Workflow "${data.name}" registered successfully`);
    },
    onError: (error: any) => {
      console.error('Workflow registration failed:', error);
      console.error('Error details:', {
        response: error?.response,
        data: error?.response?.data,
        message: error?.message,
      });
      const errorMessage = error?.response?.data?.message || error?.response?.data || error?.message || 'Failed to register workflow';
      toast.error(typeof errorMessage === 'string' ? errorMessage : JSON.stringify(errorMessage));
    },
  });
}

/**
 * Update workflow mutation hook
 *
 * Updates an existing workflow definition
 *
 * @example
 * const updateWorkflow = useUpdateWorkflow();
 * updateWorkflow.mutate({
 *   workflowId: 'my_workflow',
 *   request: {
 *     name: 'Updated Name',
 *     definition: { steps: [...], fusion_threshold: 0.9, fallback: 'manual_review' }
 *   }
 * });
 */
export function useUpdateWorkflow() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workflowId,
      request,
    }: {
      workflowId: string;
      request: RegisterWorkflowRequest;
    }) => workflowApi.updateWorkflow(workflowId, request),
    onSuccess: (data, variables) => {
      // Update workflow in cache
      queryClient.setQueryData(['workflows', variables.workflowId], data);

      // Invalidate workflows list
      queryClient.invalidateQueries({ queryKey: ['workflows'] });

      // Show success toast
      toast.success(`Workflow "${data.name}" updated successfully`);
    },
    onError: (error: any) => {
      console.error('Workflow update failed:', error);
      console.error('Error details:', {
        response: error?.response,
        data: error?.response?.data,
        message: error?.message,
      });
      const errorMessage = error?.response?.data?.message || error?.response?.data || error?.message || 'Failed to update workflow';
      toast.error(typeof errorMessage === 'string' ? errorMessage : JSON.stringify(errorMessage));
    },
  });
}

/**
 * Delete workflow mutation hook
 *
 * Deletes a workflow definition
 *
 * @example
 * const deleteWorkflow = useDeleteWorkflow();
 * deleteWorkflow.mutate('my_workflow');
 */
export function useDeleteWorkflow() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (workflowId: string) => workflowApi.deleteWorkflow(workflowId),
    onSuccess: (_, workflowId) => {
      // Remove workflow from cache
      queryClient.removeQueries({ queryKey: ['workflows', workflowId] });

      // Invalidate workflows list
      queryClient.invalidateQueries({ queryKey: ['workflows'] });

      // Show success toast
      toast.success('Workflow deleted successfully');
    },
    onError: (error: any) => {
      console.error('Workflow deletion failed:', error);
      toast.error('Failed to delete workflow');
    },
  });
}

/**
 * Validate workflow mutation hook
 *
 * Validates a workflow definition without executing it
 *
 * @example
 * const validateWorkflow = useValidateWorkflow();
 * validateWorkflow.mutate({
 *   workflowId: 'my_workflow',
 *   definition: { steps: [...], fusion_threshold: 0.9, fallback: 'manual' }
 * });
 */
export function useValidateWorkflow() {
  return useMutation({
    mutationFn: ({
      workflowId,
      definition,
    }: {
      workflowId: string;
      definition: WorkflowDefinition;
    }) => workflowApi.validateWorkflow(workflowId, definition),
    onSuccess: (response: ValidateWorkflowResponse) => {
      if (response.valid) {
        toast.success('Workflow validation successful');
        return;
      }

      toast.error('Workflow validation failed', {
        description:
          response.issues?.find((issue) => issue.level === 'error')?.message ||
          response.message,
      });
    },
    onError: (error: any) => {
      console.error('Workflow validation failed:', error);
      toast.error('Workflow validation failed');
    },
  });
}

/**
 * Execute workflow mutation hook
 *
 * Executes a workflow with input data
 *
 * @example
 * const executeWorkflow = useExecuteWorkflow();
 * executeWorkflow.mutate({
 *   workflowId: 'my_workflow',
 *   request: {
 *     input: { street: '123 Main St', city: 'Boston' },
 *     context: { user_id: '123' }
 *   }
 * });
 */
export function useExecuteWorkflow() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workflowId,
      request,
    }: {
      workflowId: string;
      request: WorkflowExecutionRequest;
    }) => workflowApi.executeWorkflow(workflowId, request),
    onSuccess: (data, variables) => {
      // Invalidate workflow executions list
      queryClient.invalidateQueries({
        queryKey: ['workflows', variables.workflowId, 'executions'],
      });

      // Set execution result in cache
      queryClient.setQueryData(['executions', data.execution_id], data);

      // Show success toast
      toast.success('Workflow executed successfully');
    },
    onError: (error: any) => {
      console.error('Workflow execution failed:', error);
      toast.error('Workflow execution failed');
    },
  });
}

/**
 * List workflow executions query hook
 *
 * Fetches execution history for a workflow
 *
 * @param workflowId - Workflow ID
 * @param params - Pagination parameters (optional)
 * @example
 * const { data: executions } = useWorkflowExecutions('my_workflow');
 */
export function useWorkflowExecutions(workflowId: string | undefined, params?: PaginationParams) {
  return useQuery({
    queryKey: ['workflows', workflowId, 'executions', params],
    queryFn: () => workflowApi.listWorkflowExecutions(workflowId!, params),
    enabled: !!workflowId,
    staleTime: 30 * 1000, // 30 seconds (execution history changes frequently)
  });
}

/**
 * Infinite workflow executions query hook
 *
 * Fetches execution history with infinite scroll/pagination
 *
 * @param workflowId - Workflow ID
 * @example
 * const { data, fetchNextPage, hasNextPage } = useWorkflowExecutionsInfinite('my_workflow');
 */
export function useWorkflowExecutionsInfinite(workflowId: string | undefined) {
  return useInfiniteQuery({
    queryKey: ['workflows', workflowId, 'executions', 'infinite'],
    queryFn: ({ pageParam = 1 }) =>
      workflowApi.listWorkflowExecutions(workflowId!, { page: pageParam, page_size: 20 }),
    getNextPageParam: (lastPage, allPages) => {
      // If we got less than page_size results, there are no more pages
      const hasMore = lastPage.length === 20;
      return hasMore ? allPages.length + 1 : undefined;
    },
    enabled: !!workflowId,
    initialPageParam: 1,
    staleTime: 30 * 1000, // 30 seconds
  });
}

/**
 * Get execution details query hook
 *
 * Fetches detailed execution result
 *
 * @param executionId - Execution ID
 * @example
 * const { data: execution } = useExecutionDetails('exec_abc123');
 */
export function useExecutionDetails(executionId: string | undefined) {
  return useQuery({
    queryKey: ['executions', executionId],
    queryFn: () => workflowApi.getExecutionDetails(executionId!),
    enabled: !!executionId,
    staleTime: 5 * 60 * 1000, // 5 minutes (execution results don't change)
  });
}

/**
 * Validate workflow definition (pre-deployment)
 *
 * Validates a workflow without registering it
 *
 * @example
 * const validateDef = useValidateWorkflowDefinition();
 * validateDef.mutate({ steps: [...], fusion_threshold: 0.85, fallback: 'manual_review' });
 */
export function useValidateWorkflowDefinition() {
  return useMutation({
    mutationFn: (definition: import('@/api/types').WorkflowDefinition) =>
      workflowApi.validateWorkflowDefinition(definition),
    onSuccess: (data) => {
      if (data.valid) {
        toast.success('✅ Workflow is valid', {
          description: data.warnings?.length
            ? `${data.warnings.length} warnings`
            : `${data.step_count} steps validated`,
        });
      } else {
        toast.error('❌ Workflow validation failed', {
          description: data.message,
        });
      }
    },
    onError: (error: any) => {
      console.error('Workflow validation failed:', error);
      toast.error('Validation request failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Test workflow step mutation hook
 *
 * Tests a single step with sample input
 *
 * @example
 * const testStep = useTestWorkflowStep();
 * testStep.mutate({
 *   step: { id: 'test', step_type: 'ml_prediction', config: {...} },
 *   input: { data: 'test' }
 * });
 */
export function useTestWorkflowStep() {
  return useMutation({
    mutationFn: (request: import('@/api/types').TestStepRequest) =>
      workflowApi.testWorkflowStep(request),
    onSuccess: (data) => {
      if (data.success) {
        toast.success('✅ Step test passed', {
          description: `Executed in ${data.execution_time_ms}ms`,
        });
      } else {
        toast.warning('⚠️ Step test failed', {
          description: data.error || 'Unknown error',
        });
      }
    },
    onError: (error: any) => {
      console.error('Step test failed:', error);
      toast.error('Step test request failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Dry-run workflow mutation hook
 *
 * Executes workflow without persisting results
 *
 * @example
 * const dryRun = useDryRunWorkflow();
 * dryRun.mutate({
 *   workflowId: 'my_workflow',
 *   request: { input: { test: 'data' } }
 * });
 */
export function useDryRunWorkflow() {
  return useMutation({
    mutationFn: ({
      workflowId,
      request,
    }: {
      workflowId: string;
      request: import('@/api/types').DryRunRequest;
    }) => workflowApi.dryRunWorkflow(workflowId, request),
    onSuccess: (data) => {
      if (data.success) {
        toast.success('✅ Dry-run completed successfully', {
          description: `${data.steps_executed.length} steps in ${data.total_execution_time_ms}ms`,
        });
      } else {
        toast.error('❌ Dry-run failed', {
          description: `Failed at step: ${data.failed_step}`,
        });
      }
    },
    onError: (error: any) => {
      console.error('Dry-run failed:', error);
      toast.error('Dry-run request failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Create schedule mutation hook
 *
 * Creates a new schedule for automatic workflow execution
 * Multiple schedules per workflow are supported
 *
 * @example
 * const createSchedule = useCreateSchedule();
 * createSchedule.mutate({
 *   workflowId: 'my_workflow',
 *   request: {
 *     cron_expression: '0 0 * * *',
 *     timezone: 'America/New_York',
 *     input: { data: 'daily' },
 *     enabled: true
 *   }
 * });
 */
export function useCreateSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workflowId,
      request,
    }: {
      workflowId: string;
      request: import('@/api/types').ScheduleWorkflowRequest;
    }) => workflowApi.createSchedule(workflowId, request),
    onSuccess: (data, variables) => {
      queryClient.invalidateQueries({
        queryKey: ['workflows', variables.workflowId, 'schedules'],
      });

      toast.success('✅ Schedule created', {
        description: `Next execution: ${new Date(data.next_execution).toLocaleString()}`,
      });
    },
    onError: (error: any) => {
      console.error('Create schedule failed:', error);
      toast.error('Failed to create schedule', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * List workflow schedules query hook
 *
 * Fetches all schedules for a workflow
 * Returns an array since multiple schedules per workflow are supported
 *
 * @param workflowId - Workflow ID
 * @example
 * const { data: schedules } = useWorkflowSchedules('my_workflow');
 */
export function useWorkflowSchedules(workflowId: string | undefined) {
  return useQuery({
    queryKey: ['workflows', workflowId, 'schedules'],
    queryFn: () => workflowApi.listWorkflowSchedules(workflowId!),
    enabled: !!workflowId,
    staleTime: 30 * 1000, // 30 seconds
  });
}

/**
 * Get specific schedule query hook
 *
 * Fetches a single schedule by ID
 *
 * @param workflowId - Workflow ID
 * @param scheduleId - Schedule ID
 * @example
 * const { data: schedule } = useSchedule('my_workflow', 'schedule_123');
 */
export function useSchedule(workflowId: string | undefined, scheduleId: string | undefined) {
  return useQuery({
    queryKey: ['workflows', workflowId, 'schedules', scheduleId],
    queryFn: () => workflowApi.getSchedule(workflowId!, scheduleId!),
    enabled: !!workflowId && !!scheduleId,
    staleTime: 30 * 1000, // 30 seconds
  });
}

/**
 * Update schedule mutation hook
 *
 * Updates an existing schedule
 *
 * @example
 * const updateSchedule = useUpdateSchedule();
 * updateSchedule.mutate({
 *   workflowId: 'my_workflow',
 *   scheduleId: 'schedule_123',
 *   request: { enabled: false }
 * });
 */
export function useUpdateSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workflowId,
      scheduleId,
      request,
    }: {
      workflowId: string;
      scheduleId: string;
      request: import('@/api/types').UpdateScheduleRequest;
    }) => workflowApi.updateSchedule(workflowId, scheduleId, request),
    onSuccess: (data, variables) => {
      // Invalidate schedules list
      queryClient.invalidateQueries({
        queryKey: ['workflows', variables.workflowId, 'schedules'],
      });

      // Update specific schedule in cache
      queryClient.setQueryData(
        ['workflows', variables.workflowId, 'schedules', variables.scheduleId],
        data
      );

      toast.success('Schedule updated');
    },
    onError: (error: any) => {
      console.error('Update schedule failed:', error);
      toast.error('Failed to update schedule', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Delete schedule mutation hook
 *
 * Deletes a specific schedule
 *
 * @example
 * const deleteSchedule = useDeleteSchedule();
 * deleteSchedule.mutate({
 *   workflowId: 'my_workflow',
 *   scheduleId: 'schedule_123'
 * });
 */
export function useDeleteSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      workflowId,
      scheduleId,
    }: {
      workflowId: string;
      scheduleId: string;
    }) => workflowApi.deleteSchedule(workflowId, scheduleId),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ['workflows', variables.workflowId, 'schedules'],
      });

      toast.success('Schedule deleted');
    },
    onError: (error: any) => {
      console.error('Delete schedule failed:', error);
      toast.error('Failed to delete schedule', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Legacy hooks for backward compatibility (deprecated)
// ============================================================================

/**
 * @deprecated Use useCreateSchedule instead
 */
export const useScheduleWorkflow = useCreateSchedule;

/**
 * @deprecated Use useWorkflowSchedules instead (note plural)
 * This hook returns an array of schedules
 */
export const useWorkflowSchedule = useWorkflowSchedules;

/**
 * @deprecated Use useDeleteSchedule instead (now requires scheduleId)
 * This deletes the first enabled schedule for backward compatibility
 */
export function useUnscheduleWorkflow() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (workflowId: string) => workflowApi.unscheduleWorkflow(workflowId),
    onSuccess: (_, workflowId) => {
      queryClient.invalidateQueries({
        queryKey: ['workflows', workflowId, 'schedules'],
      });

      toast.success('Workflow unscheduled');
    },
    onError: (error: any) => {
      console.error('Unschedule workflow failed:', error);
      toast.error('Failed to unschedule workflow', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// New API v0.2.0 Hooks
// ============================================================================

/**
 * Execute workflow asynchronously (for long-running workflows)
 */
export function useExecuteWorkflowAsync() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ workflowId, request }: { workflowId: string; request: any }) =>
      workflowApi.executeWorkflowAsync(workflowId, request),
    onSuccess: (result, { workflowId }) => {
      queryClient.invalidateQueries({
        queryKey: ['workflows', workflowId, 'executions'],
      });

      toast.success('Workflow execution started', {
        description: `Execution ID: ${result.execution_id}`,
      });
    },
    onError: (error: any) => {
      console.error('Async workflow execution failed:', error);
      toast.error('Failed to start workflow execution', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * List ALL executions across all workflows (global)
 */
export function useExecutions(
  params?: { workflow_id?: string; status?: string; limit?: number; offset?: number }
) {
  return useQuery({
    queryKey: ['executions', 'global', params],
    queryFn: () => workflowApi.listExecutions(params),
    staleTime: 10000,
  });
}

/**
 * Get execution logs
 */
export function useExecutionLogs(
  executionId: string | undefined,
  params?: { level?: string; limit?: number; offset?: number }
) {
  return useQuery({
    queryKey: ['executions', executionId, 'logs', params],
    queryFn: () => workflowApi.getExecutionLogs(executionId!, params),
    enabled: !!executionId,
    refetchInterval: 2000, // Poll every 2 seconds for real-time logs
  });
}

/**
 * Stop execution gracefully
 */
export function useStopExecution() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (executionId: string) => workflowApi.stopExecution(executionId),
    onSuccess: (_, executionId) => {
      queryClient.invalidateQueries({
        queryKey: ['executions', executionId],
      });

      toast.success('Execution stopped');
    },
    onError: (error: any) => {
      console.error('Stop execution failed:', error);
      toast.error('Failed to stop execution', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Pause execution
 */
export function usePauseExecution() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (executionId: string) => workflowApi.pauseExecution(executionId),
    onSuccess: (_, executionId) => {
      queryClient.invalidateQueries({
        queryKey: ['executions', executionId],
      });

      toast.success('Execution paused');
    },
    onError: (error: any) => {
      console.error('Pause execution failed:', error);
      toast.error('Failed to pause execution', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Resume execution
 */
export function useResumeExecution() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (executionId: string) => workflowApi.resumeExecution(executionId),
    onSuccess: (_, executionId) => {
      queryClient.invalidateQueries({
        queryKey: ['executions', executionId],
      });

      toast.success('Execution resumed');
    },
    onError: (error: any) => {
      console.error('Resume execution failed:', error);
      toast.error('Failed to resume execution', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Abort execution immediately
 */
export function useAbortExecution() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (executionId: string) => workflowApi.abortExecution(executionId),
    onSuccess: (_, executionId) => {
      queryClient.invalidateQueries({
        queryKey: ['executions', executionId],
      });

      toast.warning('Execution aborted');
    },
    onError: (error: any) => {
      console.error('Abort execution failed:', error);
      toast.error('Failed to abort execution', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Preview schedule next runs
 */
export function usePreviewSchedule() {
  return useMutation({
    mutationFn: (request: { cron_expression: string; timezone: string; count: number }) =>
      workflowApi.previewSchedule(request),
    onError: (error: any) => {
      console.error('Preview schedule failed:', error);
      toast.error('Failed to preview schedule', {
        description: error.message || 'Invalid cron expression',
      });
    },
  });
}

/**
 * Get route statistics
 */
export function useRouteStatistics() {
  return useMutation({
    mutationFn: ({ workflowId, sampleData }: { workflowId: string; sampleData: Array<Record<string, any>> }) =>
      workflowApi.getRouteStatistics(workflowId, sampleData),
    onError: (error: any) => {
      console.error('Get route statistics failed:', error);
      toast.error('Failed to analyze route statistics', {
        description: error.message || 'Server error',
      });
    },
  });
}
