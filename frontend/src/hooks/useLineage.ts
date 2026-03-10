/**
 * Lineage React Query hooks
 *
 * Provides hooks for lineage tracking, impact analysis, and provenance queries
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as lineageApi from '@/api/lineage';
import {
  ModelImpactQuery,
  ImpactAnalysisRequest,
  LineageEvent,
} from '@/api/types';

/**
 * Get record lineage query hook
 *
 * @param recordId - Record ID
 * @param enabled - Whether to enable the query
 * @example
 * const { data: lineage } = useRecordLineage('customer_123');
 */
export function useRecordLineage(recordId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['lineage', 'record', recordId],
    queryFn: () => lineageApi.getRecordLineage(recordId!),
    enabled: enabled && !!recordId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get record lineage as of timestamp query hook
 *
 * @param recordId - Record ID
 * @param timestamp - ISO 8601 timestamp
 * @param enabled - Whether to enable the query
 * @example
 * const { data: lineage } = useRecordLineageAsOf('customer_123', '2024-01-01T00:00:00Z');
 */
export function useRecordLineageAsOf(
  recordId: string | undefined,
  timestamp: string | undefined,
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'record-as-of', recordId, timestamp],
    queryFn: () => lineageApi.getRecordLineageAsOf(recordId!, timestamp!),
    enabled: enabled && !!recordId && !!timestamp,
    staleTime: 5 * 60 * 1000, // 5 minutes (historical data is more stable)
  });
}

/**
 * Get model impact analysis query hook
 *
 * @param modelId - Model ID
 * @param query - Impact query parameters
 * @param enabled - Whether to enable the query
 * @example
 * const { data: impact } = useModelImpact('fraud_model_v2', { version: 'v2.1' });
 */
export function useModelImpact(
  modelId: string | undefined,
  query: ModelImpactQuery,
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'model-impact', modelId, query],
    queryFn: () => lineageApi.getModelImpact(modelId!, query),
    enabled: enabled && !!modelId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Forward impact analysis query hook
 *
 * @param request - Impact analysis request
 * @param enabled - Whether to enable the query
 * @example
 * const { data: impact } = useForwardImpactAnalysis({
 *   entity_uri: 'http://example.org/customer/123',
 *   max_depth: 3
 * });
 */
export function useForwardImpactAnalysis(
  request: ImpactAnalysisRequest | undefined,
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'forward-impact', request],
    queryFn: () => lineageApi.forwardImpactAnalysis(request!),
    enabled: enabled && !!request?.entity_uri,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Backward root cause analysis query hook
 *
 * @param request - Impact analysis request
 * @param enabled - Whether to enable the query
 * @example
 * const { data: rootCause } = useBackwardRootCauseAnalysis({
 *   entity_uri: 'http://example.org/customer/123',
 *   timestamp: '2024-01-01T00:00:00Z'
 * });
 */
export function useBackwardRootCauseAnalysis(
  request: ImpactAnalysisRequest | undefined,
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'backward-impact', request],
    queryFn: () => lineageApi.backwardRootCauseAnalysis(request!),
    enabled: enabled && !!request?.entity_uri,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Query lineage mutation hook
 *
 * @example
 * const queryLineage = useQueryLineage();
 * queryLineage.mutate({
 *   dataset: 'customers',
 *   operation: 'UPDATE',
 *   start_time: '2024-01-01T00:00:00Z',
 *   end_time: '2024-01-31T23:59:59Z'
 * });
 */
export function useQueryLineage() {
  return useMutation({
    mutationFn: (filters: Record<string, any>) => lineageApi.queryLineage(filters),
    onError: (error: any) => {
      console.error('Lineage query failed:', error);
      toast.error('Failed to query lineage events');
    },
  });
}

/**
 * Write lineage events mutation hook
 *
 * @example
 * const writeEvents = useWriteLineageEvents();
 * writeEvents.mutate([{
 *   id: 'evt_123',
 *   record_id: 'customer_456',
 *   dataset: 'customers',
 *   operation: 'UPDATE',
 *   timestamp: new Date().toISOString(),
 *   user_id: 'user_789'
 * }]);
 */
export function useWriteLineageEvents() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (events: LineageEvent[]) => lineageApi.writeLineageEvents(events),
    onSuccess: () => {
      // Invalidate all lineage queries
      queryClient.invalidateQueries({ queryKey: ['lineage'] });
      toast.success('Lineage events written successfully');
    },
    onError: (error: any) => {
      console.error('Failed to write lineage events:', error);
      toast.error('Failed to write lineage events');
    },
  });
}

/**
 * Simulate change impact mutation hook
 *
 * @example
 * const simulateImpact = useSimulateChangeImpact();
 * simulateImpact.mutate({
 *   entity_uri: 'http://example.org/customer/123',
 *   changes: { status: 'archived' },
 *   max_depth: 5
 * });
 */
export function useSimulateChangeImpact() {
  return useMutation({
    mutationFn: (simulation: ImpactAnalysisRequest & { changes?: Record<string, any> }) =>
      lineageApi.simulateChangeImpact(simulation),
    onSuccess: () => {
      toast.success('Impact simulation completed');
    },
    onError: (error: any) => {
      console.error('Impact simulation failed:', error);
      toast.error('Failed to simulate impact');
    },
  });
}

/**
 * Get row lineage query hook
 *
 * @param rowKey - Unique row identifier
 * @param enabled - Whether to enable the query
 * @example
 * const { data: lineage } = useRowLineage('customer_12345');
 */
export function useRowLineage(rowKey: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['lineage', 'row', rowKey],
    queryFn: () => lineageApi.getRowLineage(rowKey!),
    enabled: enabled && !!rowKey,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get row journey query hook
 *
 * @param rowKey - Unique row identifier
 * @param params - Optional query parameters
 * @param enabled - Whether to enable the query
 * @example
 * const { data: journey } = useRowJourney('customer_12345', {
 *   include_related_rows: true,
 *   format: 'graph'
 * });
 */
export function useRowJourney(
  rowKey: string | undefined,
  params?: { include_related_rows?: boolean; format?: 'graph' | 'timeline' },
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'row-journey', rowKey, params],
    queryFn: () => lineageApi.getRowJourney(rowKey!, params),
    enabled: enabled && !!rowKey,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get batch lineage query hook
 *
 * @param batchId - Batch identifier
 * @param params - Optional query parameters (pagination, filtering)
 * @param enabled - Whether to enable the query
 * @example
 * const { data: batch } = useBatchLineage('batch_001', {
 *   limit: 100,
 *   offset: 0,
 *   status_filter: 'FAILED'
 * });
 */
export function useBatchLineage(
  batchId: string | undefined,
  params?: {
    limit?: number;
    offset?: number;
    status_filter?: 'SUCCESS' | 'FAILED' | 'PARTIAL' | 'ALL';
    include_transformations?: boolean;
  },
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'batch', batchId, params],
    queryFn: () => lineageApi.getBatchLineage(batchId!, params),
    enabled: enabled && !!batchId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get job statistics query hook
 *
 * @param jobId - Job identifier
 * @param enabled - Whether to enable the query
 * @example
 * const { data: stats } = useJobStats('job_abc123');
 */
export function useJobStats(jobId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['lineage', 'job-stats', jobId],
    queryFn: () => lineageApi.getJobStats(jobId!),
    enabled: enabled && !!jobId,
    staleTime: 5 * 60 * 1000, // 5 minutes (stats don't change often)
  });
}

/**
 * Get filtered rows query hook
 *
 * @param jobId - Job identifier
 * @param filters - Filter criteria
 * @param enabled - Whether to enable the query
 * @example
 * const { data: rows } = useFilteredRows('job_abc123', {
 *   status: 'FAILED',
 *   quality_max: 0.5,
 *   limit: 50
 * });
 */
export function useFilteredRows(
  jobId: string | undefined,
  filters: {
    status?: 'SUCCESS' | 'FAILED' | 'PARTIAL';
    quality_min?: number;
    quality_max?: number;
    operation?: string;
    dataset?: string;
    error_type?: string;
    limit?: number;
    offset?: number;
  },
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'filtered-rows', jobId, filters],
    queryFn: () => lineageApi.getFilteredRows(jobId!, filters),
    enabled: enabled && !!jobId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get run lineage query hook
 *
 * @param runId - Workflow run identifier
 * @param params - Optional query parameters
 * @param enabled - Whether to enable the query
 * @example
 * const { data: run } = useRunLineage('run_xyz789', {
 *   include_steps: true,
 *   include_artifacts: true
 * });
 */
export function useRunLineage(
  runId: string | undefined,
  params?: { include_steps?: boolean; include_artifacts?: boolean },
  enabled = true
) {
  return useQuery({
    queryKey: ['lineage', 'run', runId, params],
    queryFn: () => lineageApi.getRunLineage(runId!, params),
    enabled: enabled && !!runId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}
