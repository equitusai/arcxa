/**
 * ETL Operations React Query hooks
 *
 * Provides hooks for ETL pipeline operations:
 * - CSV operations (scan, import, export)
 * - Database operations (extract, load)
 * - Transformations (field transforms, joins, aggregations)
 * - Quality operations (validation, deduplication)
 * - Data profiling and preview
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as etlApi from '@/api/etl';
import type {
  ScanCSVRequest,
  ImportCSVRequest,
  ExportCSVRequest,
  ExtractFromDatabaseRequest,
  LoadToDatabaseRequest,
  ApplyTransformationsRequest,
  JoinDataRequest,
  AggregateDataRequest,
  ValidateDataRequest,
  DeduplicateDataRequest,
  LoadToRDFRequest,
  DataProfileRequest,
  CreateScheduleRequest,
} from '@/api/etl';

// ============================================================================
// CSV Operations
// ============================================================================

/**
 * Scan CSV file to detect schema
 *
 * Detects fields, data types, delimiter, encoding
 * No caching (file content may change)
 *
 * @example
 * const scanCSV = useScanCSV();
 * scanCSV.mutate({ file_path: '/data/customers.csv', delimiter: ',' });
 */
export function useScanCSV() {
  return useMutation({
    mutationFn: (request: ScanCSVRequest) => etlApi.scanCSV(request),
    onSuccess: (data) => {
      toast.success('✅ CSV scanned successfully', {
        description: `Detected ${data.detected_fields.length} fields`,
      });
    },
    onError: (error: any) => {
      console.error('CSV scan failed:', error);
      toast.error('CSV scan failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Import CSV data for ETL processing
 *
 * @example
 * const importCSV = useImportCSV();
 * importCSV.mutate({
 *   file_path: '/data/customers.csv',
 *   delimiter: ',',
 *   has_header: true
 * });
 */
export function useImportCSV() {
  return useMutation({
    mutationFn: (request: ImportCSVRequest) => etlApi.importCSV(request),
    onSuccess: (data) => {
      toast.success('✅ CSV imported successfully', {
        description: `${data.rows_imported} rows imported in ${data.duration_ms}ms`,
      });
    },
    onError: (error: any) => {
      console.error('CSV import failed:', error);
      toast.error('CSV import failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Export data to CSV file
 *
 * @example
 * const exportCSV = useExportCSV();
 * exportCSV.mutate({
 *   output_path: '/data/export.csv',
 *   source_data: rows,
 *   delimiter: ',',
 *   include_header: true
 * });
 */
export function useExportCSV() {
  return useMutation({
    mutationFn: (request: ExportCSVRequest) => etlApi.exportCSV(request),
    onSuccess: (data) => {
      toast.success('✅ CSV exported successfully', {
        description: `${data.rows_exported} rows exported (${etlApi.formatFileSize(data.file_size_bytes)})`,
      });
    },
    onError: (error: any) => {
      console.error('CSV export failed:', error);
      toast.error('CSV export failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Database Extract/Load Operations
// ============================================================================

/**
 * Extract data from datasource
 *
 * Supports both table extraction and custom queries
 * Includes incremental extraction support
 *
 * @example
 * const extract = useExtractFromDatabase();
 * extract.mutate({
 *   datasource_id: 'postgres-crm-123',
 *   table_name: 'customers',
 *   incremental: true,
 *   incremental_column: 'updated_at'
 * });
 */
export function useExtractFromDatabase() {
  return useMutation({
    mutationFn: (request: ExtractFromDatabaseRequest) =>
      etlApi.extractFromDatabase(request),
    onSuccess: (data) => {
      const incrementalNote = data.incremental_metadata
        ? ` (incremental: last value = ${data.incremental_metadata.last_value})`
        : '';

      toast.success('✅ Database extract completed', {
        description: `${data.rows_extracted} rows extracted${incrementalNote}`,
      });
    },
    onError: (error: any) => {
      console.error('Database extract failed:', error);
      toast.error('Database extract failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Load data to database
 *
 * Supports insert, upsert, and replace modes
 * Batched loading for performance
 *
 * @example
 * const load = useLoadToDatabase();
 * load.mutate({
 *   datasource_id: 'postgres-crm-123',
 *   table_name: 'customers',
 *   mode: 'upsert',
 *   key_fields: ['id'],
 *   source_data: rows
 * });
 */
export function useLoadToDatabase() {
  return useMutation({
    mutationFn: (request: LoadToDatabaseRequest) =>
      etlApi.loadToDatabase(request),
    onSuccess: (data) => {
      const modeDetails =
        data.rows_updated !== undefined
          ? ` (${data.rows_inserted} inserted, ${data.rows_updated} updated)`
          : '';

      toast.success('✅ Database load completed', {
        description: `${data.rows_loaded} rows loaded${modeDetails}`,
      });
    },
    onError: (error: any) => {
      console.error('Database load failed:', error);
      toast.error('Database load failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Transformations
// ============================================================================

/**
 * Apply field transformations to data
 *
 * Supports: rename, calculate, cast, extract, concat, etc.
 *
 * @example
 * const transform = useApplyTransformations();
 * transform.mutate({
 *   source_data: rows,
 *   transformations: [
 *     { operation: 'rename', source_field: 'name', target_field: 'full_name' }
 *   ]
 * });
 */
export function useApplyTransformations() {
  return useMutation({
    mutationFn: (request: ApplyTransformationsRequest) =>
      etlApi.applyTransformations(request),
    onSuccess: (data) => {
      toast.success('✅ Transformations applied', {
        description: `${data.transformations_applied} transformations on ${data.rows_transformed} rows`,
      });
    },
    onError: (error: any) => {
      console.error('Transformations failed:', error);
      toast.error('Transformations failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Join two datasets
 *
 * Supports: inner, left, right, full joins
 *
 * @example
 * const join = useJoinData();
 * join.mutate({
 *   left_data: customers,
 *   right_data: orders,
 *   join_type: 'inner',
 *   left_key: ['customer_id'],
 *   right_key: ['customer_id']
 * });
 */
export function useJoinData() {
  return useMutation({
    mutationFn: (request: JoinDataRequest) => etlApi.joinData(request),
    onSuccess: (data) => {
      toast.success('✅ Data joined successfully', {
        description: `${data.matched_rows} matched rows from ${data.left_rows} + ${data.right_rows} inputs`,
      });
    },
    onError: (error: any) => {
      console.error('Data join failed:', error);
      toast.error('Data join failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Aggregate data with group by
 *
 * Supports: sum, avg, count, min, max
 *
 * @example
 * const aggregate = useAggregateData();
 * aggregate.mutate({
 *   source_data: orders,
 *   group_by: ['customer_id', 'product_category'],
 *   aggregations: [
 *     { function: 'sum', field: 'amount', alias: 'total_amount' }
 *   ]
 * });
 */
export function useAggregateData() {
  return useMutation({
    mutationFn: (request: AggregateDataRequest) =>
      etlApi.aggregateData(request),
    onSuccess: (data) => {
      toast.success('✅ Data aggregated', {
        description: `${data.groups_created} groups from ${data.rows_input} rows`,
      });
    },
    onError: (error: any) => {
      console.error('Data aggregation failed:', error);
      toast.error('Data aggregation failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Quality Operations
// ============================================================================

/**
 * Validate data against rules
 *
 * Checks: not_null, regex, range, enum, custom
 *
 * @example
 * const validate = useValidateData();
 * validate.mutate({
 *   source_data: rows,
 *   rules: [
 *     { field: 'email', rule_type: 'regex', pattern: '...' }
 *   ],
 *   fail_on_error: true
 * });
 */
export function useValidateData() {
  return useMutation({
    mutationFn: (request: ValidateDataRequest) => etlApi.validateData(request),
    onSuccess: (data) => {
      const status = data.rows_invalid > 0 ? '⚠️' : '✅';
      toast.success(`${status} Validation complete`, {
        description: `${data.rows_valid} valid, ${data.rows_invalid} invalid (${data.violations.length} violations)`,
      });
    },
    onError: (error: any) => {
      console.error('Data validation failed:', error);
      toast.error('Data validation failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Remove duplicates from data
 *
 * Modes: exact, fuzzy, semantic
 * Keep strategy: first, last, most_complete
 *
 * @example
 * const dedupe = useDeduplicateData();
 * dedupe.mutate({
 *   source_data: rows,
 *   dedup_mode: 'exact',
 *   match_fields: ['email', 'phone'],
 *   keep_strategy: 'first'
 * });
 */
export function useDeduplicateData() {
  return useMutation({
    mutationFn: (request: DeduplicateDataRequest) =>
      etlApi.deduplicateData(request),
    onSuccess: (data) => {
      const reductionPct = ((data.duplicates_removed / data.rows_input) * 100).toFixed(1);
      toast.success('✅ Deduplication complete', {
        description: `${data.duplicates_removed} duplicates removed (${reductionPct}% reduction)`,
      });
    },
    onError: (error: any) => {
      console.error('Deduplication failed:', error);
      toast.error('Deduplication failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// RDF Loading
// ============================================================================

/**
 * Load data to RDF triple store
 *
 * Creates entities with ontology mapping and lineage tracking
 *
 * @example
 * const loadRDF = useLoadToRDF();
 * loadRDF.mutate({
 *   source_data: rows,
 *   target_graph: 'http://example.org/data',
 *   entity_type: 'Person',
 *   id_field: 'id',
 *   capture_lineage: true
 * });
 */
export function useLoadToRDF() {
  return useMutation({
    mutationFn: (request: LoadToRDFRequest) => etlApi.loadToRDF(request),
    onSuccess: (data) => {
      const lineageNote = data.lineage_captured ? ' (lineage tracked)' : '';
      toast.success('✅ RDF load complete', {
        description: `${data.entities_created} entities, ${data.triples_stored} triples${lineageNote}`,
      });
    },
    onError: (error: any) => {
      console.error('RDF load failed:', error);
      toast.error('RDF load failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Data Profiling & Preview
// ============================================================================

/**
 * Profile datasource data
 *
 * Generates column statistics: distinct count, nulls, min/max, top values
 *
 * @param datasourceId - Datasource ID
 * @param tableName - Optional table name
 * @param enabled - Whether to enable the query
 * @example
 * const { data: profile } = useProfileData('postgres-crm-123', 'customers');
 */
export function useProfileData(
  datasourceId: string | undefined,
  tableName?: string,
  enabled = true
) {
  return useQuery({
    queryKey: ['datasources', datasourceId, 'profile', tableName],
    queryFn: () =>
      etlApi.profileData({
        datasource_id: datasourceId!,
        table_name: tableName,
        sample_size: 1000,
      }),
    enabled: enabled && !!datasourceId,
    staleTime: 10 * 60 * 1000, // 10 minutes
  });
}

// ============================================================================
// ETL Job Monitoring
// ============================================================================

/**
 * Get ETL job status
 *
 * Monitors running extract/transform/load jobs
 *
 * @param jobId - Job ID
 * @param enabled - Whether to enable the query
 * @example
 * const { data: status } = useETLJobStatus('job-123');
 */
export function useETLJobStatus(jobId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['etl', 'jobs', jobId, 'status'],
    queryFn: () => etlApi.getETLJobStatus(jobId!),
    enabled: enabled && !!jobId,
    staleTime: 0, // Always fresh for job monitoring
    refetchInterval: (query) => {
      // Poll every 2 seconds while job is running
      const data = query.state.data;
      if (data?.status === 'running' || data?.status === 'pending') {
        return 2000;
      }
      // Stop polling when job is complete
      return false;
    },
  });
}

/**
 * Cancel running ETL job
 *
 * @example
 * const cancel = useCancelETLJob();
 * cancel.mutate('job-123');
 */
export function useCancelETLJob() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (jobId: string) => etlApi.cancelETLJob(jobId),
    onSuccess: (_, jobId) => {
      // Invalidate job status to show updated state
      queryClient.invalidateQueries({
        queryKey: ['etl', 'jobs', jobId, 'status'],
      });

      toast.success('ETL job cancelled');
    },
    onError: (error: any) => {
      console.error('Cancel ETL job failed:', error);
      toast.error('Failed to cancel ETL job', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Workflow Scheduling
// ============================================================================

/**
 * Create workflow schedule
 *
 * @example
 * const schedule = useCreateSchedule();
 * schedule.mutate({
 *   workflow_id: 'etl-workflow-123',
 *   cron_expression: '0 2 * * *',
 *   enabled: true
 * });
 */
export function useCreateSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateScheduleRequest) =>
      etlApi.createSchedule(request),
    onSuccess: (data) => {
      queryClient.invalidateQueries({
        queryKey: ['workflows', data.workflow_id, 'schedule'],
      });

      toast.success('✅ Schedule created', {
        description: `Next run: ${new Date(data.next_run).toLocaleString()}`,
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
 * Get workflow schedule
 *
 * @param workflowId - Workflow ID
 * @param enabled - Whether to enable the query
 * @example
 * const { data: schedule } = useSchedule('etl-workflow-123');
 */
export function useSchedule(workflowId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['etl', 'schedule', workflowId],
    queryFn: () => etlApi.getSchedule(workflowId!),
    enabled: enabled && !!workflowId,
    staleTime: 30 * 1000, // 30 seconds
  });
}

/**
 * Update schedule status (enable/disable)
 *
 * @example
 * const update = useUpdateScheduleStatus();
 * update.mutate({ scheduleId: 'sched-123', enabled: false });
 */
export function useUpdateScheduleStatus() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      scheduleId,
      enabled,
    }: {
      scheduleId: string;
      enabled: boolean;
    }) => etlApi.updateScheduleStatus(scheduleId, enabled),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({
        queryKey: ['etl', 'schedule'],
      });

      toast.success(
        variables.enabled ? 'Schedule enabled' : 'Schedule disabled'
      );
    },
    onError: (error: any) => {
      console.error('Update schedule status failed:', error);
      toast.error('Failed to update schedule', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Delete schedule
 *
 * @example
 * const deleteSchedule = useDeleteSchedule();
 * deleteSchedule.mutate('sched-123');
 */
export function useDeleteSchedule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (scheduleId: string) => etlApi.deleteSchedule(scheduleId),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['etl', 'schedule'],
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
