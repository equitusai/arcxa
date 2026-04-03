/**
 * Workflow Orchestration API functions
 *
 * Handles workflow management, execution, and monitoring
 */

import { api } from './client';
import {
  ExecuteWorkflowRequest,
  Workflow,
  WorkflowDefinition,
  ValidateWorkflowResponse,
  RegisterWorkflowRequest,
  RegisterWorkflowResponse,
  WorkflowExecutionRequest,
  WorkflowExecutionResult,
  WorkflowExecutionSummary,
  PaginationParams,
} from './types';
import {
  adaptWorkflowDefinition,
  adaptWorkflowResponse,
  adaptWorkflowStepForBackend,
} from '@/lib/workflow-api-adapter';

interface BackendExecutionResultDto {
  execution_id: string;
  success: boolean;
  step_results: WorkflowExecutionResult['step_results'];
  final_output: unknown;
  confidence: number;
  materialized_dataset?: WorkflowExecutionResult['materialized_dataset'];
}

interface BackendExecuteWorkflowResponse {
  workflow_id: string;
  results: BackendExecutionResultDto[];
  batch_count: number;
  overall_success: boolean;
  average_confidence: number;
  started_at: string;
  completed_at: string;
}

interface BackendExecutionDetailsResponse {
  execution_id: string;
  workflow_id: string;
  status?: string;
  confidence?: number;
  step_results?: WorkflowExecutionResult['step_results'];
  output?: unknown;
  started_at: string;
  updated_at?: string;
  completed_at?: string;
  duration_ms?: number;
}

interface BackendExecutionSummaryDto {
  execution_id: string;
  workflow_id: string;
  workflow_name?: string;
  status?: string;
  started_at: string;
  updated_at?: string;
  completed_at?: string;
  duration_ms?: number;
  confidence?: number;
  actions_executed?: number;
}

interface BackendWorkflowDetailsResponse {
  workflow_id: string;
  name: string;
  description?: string;
  tags: string[];
  created_at: string;
  version: string;
  execution_count: number;
  last_executed_at?: string;
  definition: WorkflowDefinition;
}

function adaptRegisteredWorkflow(
  response: RegisterWorkflowResponse,
  request: RegisterWorkflowRequest
): Workflow {
  return {
    id: response.workflow_id,
    name: response.name,
    description: request.description,
    tags: request.tags,
    definition: request.definition,
    created_at: response.created_at,
  };
}

function isGraphInput(input: WorkflowExecutionRequest['input']): input is ExecuteWorkflowRequest['input'] {
  return typeof input === 'object' && input !== null && 'type' in input;
}

function isJsonWorkflowInput(
  input: WorkflowExecutionRequest['input']
): input is Extract<ExecuteWorkflowRequest['input'], { type: 'json' }> {
  return isGraphInput(input) && input.type === 'json';
}

function isBackendExecuteWorkflowResponse(
  response: BackendExecuteWorkflowResponse | WorkflowExecutionResult
): response is BackendExecuteWorkflowResponse {
  return 'overall_success' in response && 'average_confidence' in response;
}

function isBackendExecutionDetailsResponse(
  response: BackendExecutionDetailsResponse | WorkflowExecutionResult
): response is BackendExecutionDetailsResponse {
  return 'status' in response || 'updated_at' in response || 'output' in response;
}

function isPersistedExecutionOutput(
  output: unknown
): output is {
  final_output?: WorkflowExecutionResult['final_output'];
  materialized_dataset?: WorkflowExecutionResult['materialized_dataset'];
} {
  return typeof output === 'object' && output !== null &&
    ('final_output' in output || 'materialized_dataset' in output);
}

function buildExecutionPayload(request: WorkflowExecutionRequest) {
  const payload: Record<string, unknown> = {
    input: request.input,
  };

  if (request.context) {
    payload.context = request.context;
  }

  if (request.output_dataset) {
    payload.output_dataset = request.output_dataset;
  }

  return payload;
}

function calculateDurationMs(startedAt: string, completedAt: string): number {
  const started = new Date(startedAt).getTime();
  const completed = new Date(completedAt).getTime();

  if (Number.isNaN(started) || Number.isNaN(completed)) {
    return 0;
  }

  return Math.max(0, completed - started);
}

function toLimitOffsetParams(params?: PaginationParams) {
  if (!params) {
    return undefined;
  }

  const limit = params.page_size;
  const offset =
    params.page !== undefined && params.page_size !== undefined
      ? Math.max(0, (params.page - 1) * params.page_size)
      : undefined;

  return {
    limit,
    offset,
  };
}

function adaptExecutionResponse(
  response: BackendExecuteWorkflowResponse | WorkflowExecutionResult
): WorkflowExecutionResult {
  if (isBackendExecuteWorkflowResponse(response)) {
    const primaryResult = response.results[0];

    return {
      execution_id: primaryResult?.execution_id || '',
      workflow_id: response.workflow_id,
      success: response.overall_success,
      confidence: response.average_confidence,
      started_at: response.started_at,
      completed_at: response.completed_at,
      duration_ms: calculateDurationMs(response.started_at, response.completed_at),
      step_results: primaryResult?.step_results || [],
      final_output: primaryResult?.final_output,
      batch_count: response.batch_count,
      results: response.results,
      materialized_dataset: primaryResult?.materialized_dataset,
    };
  }

  return response;
}

function adaptExecutionSummary(
  summary: WorkflowExecutionSummary | BackendExecutionSummaryDto
): WorkflowExecutionSummary {
  const completedAt =
    summary.completed_at || summary.updated_at || summary.started_at;
  const status =
    'status' in summary && typeof summary.status === 'string'
      ? summary.status.toLowerCase()
      : undefined;
  const success =
    'success' in summary && typeof summary.success === 'boolean'
      ? summary.success
      : status === 'completed';
  const confidence =
    'confidence' in summary && typeof summary.confidence === 'number'
      ? summary.confidence
      : success
        ? 1
        : 0;

  return {
    ...summary,
    success,
    confidence,
    completed_at: completedAt,
    duration_ms:
      summary.duration_ms ?? calculateDurationMs(summary.started_at, completedAt),
  };
}

function adaptExecutionDetails(
  response: BackendExecutionDetailsResponse | WorkflowExecutionResult
): WorkflowExecutionResult {
  if (!isBackendExecutionDetailsResponse(response)) {
    return response;
  }

  const completedAt = response.completed_at || response.updated_at || response.started_at;
  const success = response.status?.toLowerCase() === 'completed';
  const confidence = response.confidence ?? (success ? 1 : 0);
  const stepResults = response.step_results ?? [];
  const persistedOutput = response.output;
  const finalOutput = isPersistedExecutionOutput(persistedOutput)
    ? persistedOutput.final_output
    : persistedOutput;
  const materializedDataset = isPersistedExecutionOutput(persistedOutput)
    ? persistedOutput.materialized_dataset
    : undefined;

  return {
    execution_id: response.execution_id,
    workflow_id: response.workflow_id,
    success,
    confidence,
    started_at: response.started_at,
    completed_at: completedAt,
    duration_ms: response.duration_ms ?? calculateDurationMs(response.started_at, completedAt),
    step_results: stepResults,
    final_output: finalOutput,
    batch_count: 1,
    materialized_dataset: materializedDataset,
    results: [
      {
        execution_id: response.execution_id,
        success,
        step_results: stepResults,
        final_output: finalOutput,
        confidence,
        materialized_dataset: materializedDataset,
      },
    ],
  };
}

function adaptWorkflowDetails(response: BackendWorkflowDetailsResponse): Workflow {
  const adaptedResponse = adaptWorkflowResponse(response);

  return {
    id: adaptedResponse.workflow_id,
    name: adaptedResponse.name,
    description: adaptedResponse.description,
    tags: adaptedResponse.tags,
    definition: adaptedResponse.definition,
    created_at: adaptedResponse.created_at,
    version: adaptedResponse.version,
    execution_count: adaptedResponse.execution_count,
    last_executed_at: adaptedResponse.last_executed_at,
  };
}

/**
 * List all registered workflows
 *
 * @param params - Pagination parameters (optional)
 * @returns Array of workflows
 */
export async function listWorkflows(params?: PaginationParams): Promise<Workflow[]> {
  // Backend returns: {workflow_id, name, description, tags, created_at}
  // Frontend expects: {id, name, definition, created_at}
  const backendWorkflows = await api.get<Array<{
    workflow_id: string;
    name: string;
    description?: string;
    tags?: string[];
    created_at?: string;
  }>>('/workflows', { params });

  // Transform to frontend format
  // Note: List endpoint doesn't include definition, so we set it to empty
  // Component should call getWorkflow(id) to fetch full definition when needed
  return backendWorkflows.map(w => ({
    id: w.workflow_id,
    name: w.name,
    description: w.description,
    tags: w.tags,
    definition: { steps: [], fusion_threshold: 0, fallback: 'manual_review' }, // Placeholder
    created_at: w.created_at,
  }));
}

/**
 * Get workflow by ID
 *
 * @param workflowId - Workflow ID
 * @returns Workflow with definition
 */
export async function getWorkflow(workflowId: string): Promise<Workflow> {
  const response = await api.get<BackendWorkflowDetailsResponse>(
    `/workflows/${workflowId}/details`
  );

  return adaptWorkflowDetails(response);
}

/**
 * Register new workflow
 *
 * @param request - Workflow registration request
 * @returns Created workflow
 */
export async function registerWorkflow(
  request: RegisterWorkflowRequest
): Promise<Workflow> {
  // Adapt frontend config format to backend format
  const adaptedRequest = adaptWorkflowDefinition(request);
  const response = await api.post<RegisterWorkflowResponse>('/workflows', adaptedRequest);
  return adaptRegisteredWorkflow(response, adaptedRequest);
}

/**
 * Update existing workflow
 *
 * @param workflowId - Workflow ID
 * @param request - Workflow update request
 * @returns Updated workflow
 */
export async function updateWorkflow(
  workflowId: string,
  request: RegisterWorkflowRequest
): Promise<Workflow> {
  // Adapt frontend config format to backend format
  const adaptedRequest = adaptWorkflowDefinition(request);
  const response = await api.put<RegisterWorkflowResponse>(
    `/workflows/${workflowId}`,
    adaptedRequest
  );

  return adaptRegisteredWorkflow(response, {
    id: workflowId,
    name: adaptedRequest.name,
    definition: adaptedRequest.definition,
    description: adaptedRequest.description,
    tags: adaptedRequest.tags,
  });
}

/**
 * Delete workflow
 *
 * @param workflowId - Workflow ID
 */
export async function deleteWorkflow(workflowId: string): Promise<void> {
  return api.delete(`/workflows/${workflowId}`);
}

/**
 * Validate workflow definition
 *
 * @param workflowId - Workflow ID
 * @param definition - Workflow definition to validate
 * @returns Validation result
 */
export async function validateWorkflow(
  _workflowId: string,
  definition: WorkflowDefinition
): Promise<ValidateWorkflowResponse> {
  return validateWorkflowDefinition(definition);
}

/**
 * Execute workflow synchronously
 *
 * @param workflowId - Workflow ID
 * @param request - Workflow execution request with input data
 * @returns Execution result with step-by-step outputs
 */
export async function executeWorkflow(
  workflowId: string,
  request: WorkflowExecutionRequest
): Promise<WorkflowExecutionResult> {
  const response = await api.post<BackendExecuteWorkflowResponse>(
    `/workflows/${workflowId}/execute`,
    buildExecutionPayload(request)
  );

  return adaptExecutionResponse(response);
}

/**
 * Execute workflow asynchronously (for long-running workflows)
 *
 * @param workflowId - Workflow ID
 * @param request - Workflow execution request with input data
 * @returns Execution ID and status (202 Accepted)
 */
export async function executeWorkflowAsync(
  workflowId: string,
  request: WorkflowExecutionRequest
): Promise<{ execution_id: string; workflow_id: string; workflow_name: string; status: string; started_at: string }> {
  return api.post(
    `/workflows/${workflowId}/execute-async`,
    buildExecutionPayload(request)
  );
}

/**
 * List workflow executions (history) for a specific workflow
 *
 * @param workflowId - Workflow ID
 * @param params - Pagination parameters (optional)
 * @returns Array of execution summaries
 */
export async function listWorkflowExecutions(
  workflowId: string,
  params?: PaginationParams
): Promise<WorkflowExecutionSummary[]> {
  const response = await api.get<Array<WorkflowExecutionSummary | BackendExecutionSummaryDto>>(
    `/workflows/${workflowId}/executions`,
    { params: toLimitOffsetParams(params) }
  );

  return response.map(adaptExecutionSummary);
}

/**
 * List ALL executions across all workflows (global)
 *
 * @param params - Filter and pagination (workflow_id, status, limit, offset)
 * @returns Array of execution summaries
 */
export async function listExecutions(
  params?: { workflow_id?: string; status?: string; limit?: number; offset?: number }
): Promise<{
  executions: WorkflowExecutionSummary[];
  total: number;
  limit: number;
  offset: number;
}> {
  const response = await api.get<{
    executions: BackendExecutionSummaryDto[];
    total: number;
    limit: number;
    offset: number;
  }>('/executions', { params });

  return {
    ...response,
    executions: response.executions.map(adaptExecutionSummary),
  };
}

/**
 * Get execution details
 *
 * @param executionId - Execution ID
 * @returns Detailed execution result
 */
export async function getExecutionDetails(
  executionId: string
): Promise<WorkflowExecutionResult> {
  const response = await api.get<BackendExecutionDetailsResponse | WorkflowExecutionResult>(
    `/executions/${executionId}`
  );

  return adaptExecutionDetails(response);
}

/**
 * Get execution logs
 *
 * @param executionId - Execution ID
 * @param params - Optional log level filter and pagination
 * @returns Execution logs with metadata
 */
export async function getExecutionLogs(
  executionId: string,
  params?: { level?: string; limit?: number; offset?: number }
): Promise<{
  execution_id: string;
  logs: Array<{
    timestamp: string;
    level: string;
    message: string;
    action?: string;
    metadata?: Record<string, unknown>;
  }>;
  total: number;
  limit: number;
  offset: number;
}> {
  return api.get(`/executions/${executionId}/logs`, { params });
}

// ============================================================================
// Execution Lifecycle Control (API v0.2.0)
// ============================================================================

/**
 * Stop execution gracefully (completes current action then stops)
 *
 * @param executionId - Execution ID
 * @returns Updated execution status
 */
export async function stopExecution(
  executionId: string
): Promise<{ execution_id: string; status: string; message: string }> {
  return api.post(`/executions/${executionId}/stop`);
}

/**
 * Pause execution (can be resumed later)
 *
 * @param executionId - Execution ID
 * @returns Updated execution status
 */
export async function pauseExecution(
  executionId: string
): Promise<{ execution_id: string; status: string; message: string }> {
  return api.post(`/executions/${executionId}/pause`);
}

/**
 * Resume a paused execution
 *
 * @param executionId - Execution ID
 * @returns Updated execution status
 */
export async function resumeExecution(
  executionId: string
): Promise<{ execution_id: string; status: string; message: string }> {
  return api.post(`/executions/${executionId}/resume`);
}

/**
 * Abort execution immediately (may leave partial results)
 *
 * @param executionId - Execution ID
 * @returns Updated execution status
 */
export async function abortExecution(
  executionId: string
): Promise<{ execution_id: string; status: string; message: string }> {
  return api.post(`/executions/${executionId}/abort`);
}

// ============================================================================
// Pre-Deployment Testing & Validation (API v0.2.0)
// ============================================================================

/**
 * Validate workflow definition without registering it
 *
 * @param definition - Workflow definition to validate
 * @returns Validation result
 */
export async function validateWorkflowDefinition(
  definition: WorkflowDefinition
): Promise<ValidateWorkflowResponse> {
  const adaptedDefinition = adaptWorkflowDefinition(definition);
  return api.post(`/workflows/validate`, adaptedDefinition);
}

/**
 * Test a single workflow step with sample input
 *
 * @param request - Test step request with step definition and input
 * @returns Test result with output or error
 */
export async function testWorkflowStep(
  request: import('./types').TestStepRequest
): Promise<import('./types').TestStepResponse> {
  return api.post(`/workflows/test-step`, {
    ...request,
    step: adaptWorkflowStepForBackend(request.step),
  });
}

/**
 * Execute workflow without persisting results (dry-run mode)
 *
 * @param workflowId - Workflow ID
 * @param request - Dry-run request with input data
 * @returns Execution result without persistence
 */
export async function dryRunWorkflow(
  workflowId: string,
  request: import('./types').DryRunRequest
): Promise<import('./types').DryRunResponse> {
  return api.post(`/workflows/${workflowId}/dry-run`, request);
}

// ============================================================================
// Workflow Scheduling (API v0.2.0+)
// NOTE: Frontend uses /schedules (plural) for multiple schedule support,
// but backend docs show /schedule (singular). Both implementations provided.
// ============================================================================

/**
 * Create a new schedule for a workflow
 *
 * @param workflowId - Workflow ID
 * @param request - Schedule configuration (cron/interval/one-time)
 * @returns Schedule confirmation with schedule_id
 */
export async function createSchedule(
  workflowId: string,
  request: import('./types').ScheduleWorkflowRequest
): Promise<import('./types').ScheduleWorkflowResponse> {
  return api.post(`/workflows/${workflowId}/schedules`, request);
}

/**
 * List all schedules for a workflow
 *
 * @param workflowId - Workflow ID
 * @returns Array of schedules for this workflow
 */
export async function listWorkflowSchedules(
  workflowId: string
): Promise<import('./types').WorkflowSchedule[]> {
  return api.get(`/workflows/${workflowId}/schedules`);
}

/**
 * Get a specific schedule by ID
 *
 * @param workflowId - Workflow ID
 * @param scheduleId - Schedule ID
 * @returns Schedule information
 */
export async function getSchedule(
  workflowId: string,
  scheduleId: string
): Promise<import('./types').WorkflowSchedule> {
  return api.get(`/workflows/${workflowId}/schedules/${scheduleId}`);
}

/**
 * Update an existing schedule
 *
 * @param workflowId - Workflow ID
 * @param scheduleId - Schedule ID
 * @param request - Updated schedule configuration (partial)
 * @returns Updated schedule information
 */
export async function updateSchedule(
  workflowId: string,
  scheduleId: string,
  request: import('./types').UpdateScheduleRequest
): Promise<import('./types').WorkflowSchedule> {
  return api.put(`/workflows/${workflowId}/schedules/${scheduleId}`, request);
}

/**
 * Delete a specific schedule
 *
 * @param workflowId - Workflow ID
 * @param scheduleId - Schedule ID
 * @returns Promise that resolves when deleted
 */
export async function deleteSchedule(
  workflowId: string,
  scheduleId: string
): Promise<void> {
  return api.delete(`/workflows/${workflowId}/schedules/${scheduleId}`);
}

// ============================================================================
// Legacy function names for backward compatibility (deprecated)
// ============================================================================

/**
 * @deprecated Use createSchedule instead
 */
export const scheduleWorkflow = createSchedule;

/**
 * @deprecated Use listWorkflowSchedules instead
 */
export const getWorkflowSchedule = listWorkflowSchedules;

/**
 * @deprecated Use deleteSchedule instead (now requires scheduleId)
 * This function deletes the first enabled schedule for backward compatibility
 */
export async function unscheduleWorkflow(workflowId: string): Promise<void> {
  const schedules = await listWorkflowSchedules(workflowId);
  const firstEnabled = schedules.find(s => s.enabled);
  if (firstEnabled) {
    return deleteSchedule(workflowId, firstEnabled.schedule_id);
  }
  throw new Error('No enabled schedule found to delete');
}

/**
 * Preview next N execution times for a cron expression
 *
 * @param request - Cron expression, timezone, and count
 * @returns Array of next scheduled run times
 */
export async function previewSchedule(
  request: { cron_expression: string; timezone: string; count: number }
): Promise<{ cron_expression: string; timezone: string; next_runs: string[] }> {
  return api.post('/schedule/preview', request);
}

// ============================================================================
// Analytics (API v0.2.0)
// ============================================================================

/**
 * Get route statistics - analyze which routes would match sample data
 *
 * @param workflowId - Workflow ID
 * @param sampleData - Array of sample data records
 * @returns Route match statistics
 */
export async function getRouteStatistics(
  workflowId: string,
  sampleData: Array<Record<string, unknown>>
): Promise<{
  workflow_id: string;
  total_samples: number;
  route_matches: Record<string, {
    route_id: string;
    route_name: string;
    match_count: number;
    match_percentage: number;
  }>;
  no_match_count: number;
  error_count: number;
}> {
  return api.post(`/workflows/${workflowId}/route-stats`, { sample_data: sampleData });
}
