/**
 * Lineage & Provenance API
 *
 * Provides functions for lineage tracking, impact analysis, and provenance queries
 */

import api from './client';
import {
  LineageResponse,
  LineageEvent,
  ModelImpactQuery,
  ModelImpactResponse,
  ImpactAnalysisRequest,
  ImpactAnalysisResponse,
  RowLineageResponse,
  RowJourneyResponse,
  BatchLineageResponse,
  JobStatsResponse,
  FilteredRowsResponse,
  RunLineageResponse,
} from './types';

/**
 * Get record lineage
 *
 * @param recordId - Record ID
 * @returns LineageResponse with events
 *
 * @example
 * const lineage = await getRecordLineage('customer_123');
 */
export async function getRecordLineage(recordId: string): Promise<LineageResponse> {
  return api.get<LineageResponse>(`/lineage/record/${recordId}`);
}

/**
 * Get lineage graph (upstream + downstream) for a record
 *
 * @param recordId - Record ID
 * @param maxDepth - Maximum traversal depth (default: 5)
 * @returns LineageGraphResponse with full lineage graph
 *
 * @example
 * const graph = await getRecordLineageGraph('customer_123', 3);
 */
export async function getRecordLineageGraph(
  recordId: string,
  maxDepth?: number
): Promise<import('./types').LineageGraphResponse> {
  return api.get(`/lineage/record/${recordId}/graph`, {
    params: maxDepth ? { max_depth: maxDepth } : undefined,
  });
}

/**
 * Get record lineage as of a specific timestamp
 *
 * @param recordId - Record ID
 * @param timestamp - ISO 8601 timestamp
 * @returns LineageResponse with events up to the timestamp
 *
 * @example
 * const lineage = await getRecordLineageAsOf('customer_123', '2024-01-01T00:00:00Z');
 */
export async function getRecordLineageAsOf(
  recordId: string,
  timestamp: string
): Promise<LineageResponse> {
  return api.get<LineageResponse>(`/lineage/record-as-of/${recordId}`, {
    params: { timestamp },
  });
}

/**
 * Get model impact analysis
 *
 * @param modelId - Model ID
 * @param query - Impact query parameters
 * @returns ModelImpactResponse with affected datasets and records
 *
 * @example
 * const impact = await getModelImpact('fraud_model_v2', { version: 'v2.1' });
 */
export async function getModelImpact(
  modelId: string,
  query: ModelImpactQuery
): Promise<ModelImpactResponse> {
  return api.get<ModelImpactResponse>(`/lineage/model/${modelId}/impact`, {
    params: query,
  });
}

/**
 * Forward impact analysis
 *
 * Analyzes downstream impact of changes to an entity
 *
 * @param request - Impact analysis request
 * @returns ImpactAnalysisResponse with downstream entities
 *
 * @example
 * const impact = await forwardImpactAnalysis({
 *   entity_uri: 'http://example.org/customer/123',
 *   max_depth: 3
 * });
 */
export async function forwardImpactAnalysis(
  request: ImpactAnalysisRequest
): Promise<ImpactAnalysisResponse> {
  return api.get<ImpactAnalysisResponse>('/lineage/impact/forward', {
    params: request,
  });
}

/**
 * Backward root cause analysis
 *
 * Traces root causes of data issues upstream
 *
 * @param request - Impact analysis request
 * @returns ImpactAnalysisResponse with upstream entities
 *
 * @example
 * const rootCause = await backwardRootCauseAnalysis({
 *   entity_uri: 'http://example.org/customer/123',
 *   timestamp: '2024-01-01T00:00:00Z'
 * });
 */
export async function backwardRootCauseAnalysis(
  request: ImpactAnalysisRequest
): Promise<ImpactAnalysisResponse> {
  return api.get<ImpactAnalysisResponse>('/lineage/impact/backward', {
    params: request,
  });
}

/**
 * Query lineage with filters
 *
 * @param filters - Query filters
 * @returns Array of LineageEvents
 *
 * @example
 * const events = await queryLineage({
 *   dataset: 'customers',
 *   operation: 'UPDATE',
 *   start_time: '2024-01-01T00:00:00Z',
 *   end_time: '2024-01-31T23:59:59Z'
 * });
 */
export async function queryLineage(filters: Record<string, any>): Promise<LineageEvent[]> {
  return api.post<LineageEvent[]>('/lineage/query', filters);
}

/**
 * Write lineage events
 *
 * @param events - Array of lineage events to write
 * @returns Success response
 *
 * @example
 * await writeLineageEvents([{
 *   id: 'evt_123',
 *   record_id: 'customer_456',
 *   dataset: 'customers',
 *   operation: 'UPDATE',
 *   timestamp: new Date().toISOString(),
 *   user_id: 'user_789'
 * }]);
 */
export async function writeLineageEvents(events: LineageEvent[]): Promise<void> {
  return api.post<void>('/lineage/events', { events });
}

/**
 * Simulate change impact
 *
 * Simulates the impact of a change without executing it
 *
 * @param simulation - Simulation request
 * @returns Impact analysis response
 *
 * @example
 * const simulation = await simulateChangeImpact({
 *   entity_uri: 'http://example.org/customer/123',
 *   changes: { status: 'archived' },
 *   max_depth: 5
 * });
 */
export async function simulateChangeImpact(
  simulation: ImpactAnalysisRequest & { changes?: Record<string, any> }
): Promise<ImpactAnalysisResponse> {
  return api.post<ImpactAnalysisResponse>('/lineage/simulate', simulation);
}

/**
 * Get row lineage
 *
 * Gets complete lineage history for a single data row
 *
 * @param rowKey - Unique row identifier
 * @returns Row lineage with events and metadata
 *
 * @example
 * const lineage = await getRowLineage('customer_12345');
 */
export async function getRowLineage(rowKey: string): Promise<RowLineageResponse> {
  return api.get<RowLineageResponse>(`/lineage/row/${rowKey}`);
}

/**
 * Get row journey
 *
 * Gets visualization-optimized journey data for a row (graph format)
 *
 * @param rowKey - Unique row identifier
 * @param params - Optional query parameters
 * @returns Row journey with nodes and edges for visualization
 *
 * @example
 * const journey = await getRowJourney('customer_12345', {
 *   include_related_rows: true,
 *   format: 'graph'
 * });
 */
export async function getRowJourney(
  rowKey: string,
  params?: { include_related_rows?: boolean; format?: 'graph' | 'timeline' }
): Promise<RowJourneyResponse> {
  return api.get<RowJourneyResponse>(`/lineage/row/${rowKey}/journey`, { params });
}

/**
 * Get batch lineage
 *
 * Gets lineage for all rows processed in a batch (bulk operations)
 *
 * @param batchId - Batch identifier
 * @param params - Optional query parameters (pagination, filtering)
 * @returns Batch lineage with row summaries and statistics
 *
 * @example
 * const batch = await getBatchLineage('batch_001', {
 *   limit: 100,
 *   offset: 0,
 *   status_filter: 'FAILED'
 * });
 */
export async function getBatchLineage(
  batchId: string,
  params?: {
    limit?: number;
    offset?: number;
    status_filter?: 'SUCCESS' | 'FAILED' | 'PARTIAL' | 'ALL';
    include_transformations?: boolean;
  }
): Promise<BatchLineageResponse> {
  return api.get<BatchLineageResponse>(`/lineage/batch/${batchId}`, { params });
}

/**
 * Get job statistics
 *
 * Gets aggregate statistics for a job execution (for dashboards/monitoring)
 *
 * @param jobId - Job identifier
 * @returns Job statistics with quality metrics and operation summaries
 *
 * @example
 * const stats = await getJobStats('job_abc123');
 */
export async function getJobStats(jobId: string): Promise<JobStatsResponse> {
  return api.get<JobStatsResponse>(`/lineage/job/${jobId}/stats`);
}

/**
 * Get filtered rows
 *
 * Gets rows matching specific criteria (for troubleshooting/debugging)
 *
 * @param jobId - Job identifier
 * @param filters - Filter criteria
 * @returns Filtered rows matching the criteria
 *
 * @example
 * const rows = await getFilteredRows('job_abc123', {
 *   status: 'FAILED',
 *   quality_max: 0.5,
 *   limit: 50
 * });
 */
export async function getFilteredRows(
  jobId: string,
  filters: {
    status?: 'SUCCESS' | 'FAILED' | 'PARTIAL';
    quality_min?: number;
    quality_max?: number;
    operation?: string;
    dataset?: string;
    error_type?: string;
    limit?: number;
    offset?: number;
  }
): Promise<FilteredRowsResponse> {
  return api.get<FilteredRowsResponse>(`/lineage/job/${jobId}/filtered`, { params: filters });
}

/**
 * Get run lineage
 *
 * Gets lineage for a workflow/pipeline run (execution monitoring)
 *
 * @param runId - Workflow run identifier
 * @param params - Optional query parameters
 * @returns Run lineage with steps, artifacts, and lineage graph
 *
 * @example
 * const run = await getRunLineage('run_xyz789', {
 *   include_steps: true,
 *   include_artifacts: true
 * });
 */
export async function getRunLineage(
  runId: string,
  params?: { include_steps?: boolean; include_artifacts?: boolean }
): Promise<RunLineageResponse> {
  return api.get<RunLineageResponse>(`/lineage/run/${runId}`, { params });
}

/**
 * Query lineage by time range
 *
 * @param query - Time range query parameters
 * @returns Time range lineage response with events
 *
 * @example
 * const lineage = await queryLineageByTimeRange({
 *   start: '2024-01-01T00:00:00Z',
 *   end: '2024-01-31T23:59:59Z',
 *   dataset: 'customers',
 *   limit: 100
 * });
 */
export async function queryLineageByTimeRange(
  query: import('./types').TimeRangeLineageQuery
): Promise<import('./types').TimeRangeLineageResponse> {
  return api.post('/lineage/time-range', query);
}

// ============================================================================
// Column-Level Lineage API
// ============================================================================

/**
 * Get column lineage
 *
 * Gets all transformations that produce a specific column
 *
 * @param table - Table name
 * @param column - Column name
 * @param params - Optional query parameters
 * @returns Column lineage events
 *
 * @example
 * const lineage = await getColumnLineage('customers', 'email', {
 *   datasource_id: 'db2_prod',
 *   schema: 'public'
 * });
 */
export async function getColumnLineage(
  table: string,
  column: string,
  params?: {
    datasource_id?: string;
    schema?: string;
    data_type?: string;
  }
): Promise<import('./types').ColumnLineageEvent[]> {
  return api.get(`/lineage/column/${table}/${column}`, { params });
}

/**
 * Get column lineage graph
 *
 * Gets the full lineage graph for a column (upstream dependencies)
 *
 * @param table - Table name
 * @param column - Column name
 * @param params - Optional query parameters
 * @returns Column lineage graph with nodes and edges
 *
 * @example
 * const graph = await getColumnLineageGraph('orders', 'total_amount', {
 *   datasource_id: 'db2_prod',
 *   max_depth: 5
 * });
 */
export async function getColumnLineageGraph(
  table: string,
  column: string,
  params?: {
    datasource_id?: string;
    schema?: string;
    max_depth?: number;
  }
): Promise<import('./types').ColumnLineageGraph> {
  return api.get(`/lineage/column/${table}/${column}/graph`, { params });
}

/**
 * Get derived columns
 *
 * Gets all columns that are derived from a specific column
 *
 * @param table - Table name
 * @param column - Column name
 * @param params - Optional query parameters
 * @returns List of derived column references
 *
 * @example
 * const derived = await getDerivedColumns('customers', 'first_name', {
 *   datasource_id: 'db2_prod'
 * });
 */
export async function getDerivedColumns(
  table: string,
  column: string,
  params?: {
    datasource_id?: string;
    schema?: string;
  }
): Promise<import('./types').ColumnRef[]> {
  return api.get(`/lineage/column/${table}/${column}/derived`, { params });
}

/**
 * Analyze column impact
 *
 * Analyzes the impact of changes to a column across the data pipeline
 *
 * @param request - Column impact analysis request
 * @returns Column impact analysis results
 *
 * @example
 * const impact = await analyzeColumnImpact({
 *   column: {
 *     datasource_id: 'db2_prod',
 *     table_name: 'customers',
 *     column_name: 'email',
 *     data_type: 'VARCHAR'
 *   },
 *   change_type: 'data_type_change',
 *   proposed_change: { new_data_type: 'TEXT' }
 * });
 */
export async function analyzeColumnImpact(
  request: import('./types').ColumnImpactRequest
): Promise<import('./types').ColumnImpactAnalysis> {
  return api.post('/lineage/column/impact-analysis', request);
}

// ============================================================================
// Schema Evolution API
// ============================================================================

/**
 * Record schema change event
 *
 * Records a schema change for tracking and drift analysis
 *
 * @param event - Schema change event
 * @returns Success response with event ID
 *
 * @example
 * await recordSchemaChange({
 *   datasource_id: 'db2_prod',
 *   table_name: 'customers',
 *   change_type: 'add_column',
 *   column_name: 'phone_number',
 *   new_data_type: 'VARCHAR(20)'
 * });
 */
export async function recordSchemaChange(
  event: import('./types').SchemaChangeEvent
): Promise<{ status: string; event_id: string }> {
  return api.post('/lineage/schema/change', event);
}

/**
 * Get datasource schema changes
 *
 * Gets all schema change events for a datasource
 *
 * @param datasourceId - Datasource identifier
 * @param params - Optional query parameters
 * @returns List of schema change events
 *
 * @example
 * const changes = await getDatasourceSchemaChanges('db2_prod', {
 *   start_time: '2024-01-01T00:00:00Z',
 *   end_time: '2024-12-31T23:59:59Z'
 * });
 */
export async function getDatasourceSchemaChanges(
  datasourceId: string,
  params?: {
    start_time?: string;
    end_time?: string;
    limit?: number;
  }
): Promise<import('./types').SchemaChangeEvent[]> {
  return api.get(`/lineage/schema/datasource/${datasourceId}/changes`, { params });
}

/**
 * Get table schema changes
 *
 * Gets schema change history for a specific table
 *
 * @param datasourceId - Datasource identifier
 * @param tableName - Table name
 * @param params - Optional query parameters
 * @returns List of schema change events for the table
 *
 * @example
 * const changes = await getTableSchemaChanges('db2_prod', 'customers', {
 *   start_time: '2024-01-01T00:00:00Z'
 * });
 */
export async function getTableSchemaChanges(
  datasourceId: string,
  tableName: string,
  params?: {
    start_time?: string;
    end_time?: string;
    limit?: number;
  }
): Promise<import('./types').SchemaChangeEvent[]> {
  return api.get(`/lineage/schema/datasource/${datasourceId}/table/${tableName}/changes`, {
    params,
  });
}

/**
 * Save schema version
 *
 * Saves a snapshot of a datasource schema at a point in time
 *
 * @param version - Schema version to save
 * @returns Success response
 *
 * @example
 * await saveSchemaVersion({
 *   datasource_id: 'db2_prod',
 *   version_id: 'v1.2.3',
 *   captured_at: new Date().toISOString(),
 *   schema_snapshot: { ... }
 * });
 */
export async function saveSchemaVersion(
  version: import('./types').SchemaVersion
): Promise<{ status: string }> {
  return api.post('/lineage/schema/version', version);
}

/**
 * Get latest schema version
 *
 * Gets the most recent schema snapshot for a datasource
 *
 * @param datasourceId - Datasource identifier
 * @returns Latest schema version
 *
 * @example
 * const latest = await getLatestSchemaVersion('db2_prod');
 */
export async function getLatestSchemaVersion(
  datasourceId: string
): Promise<import('./types').SchemaVersion> {
  return api.get(`/lineage/schema/datasource/${datasourceId}/version/latest`);
}

/**
 * Analyze schema drift
 *
 * Analyzes differences between two schema versions
 *
 * @param sourceVersion - Source version ID
 * @param targetVersion - Target version ID
 * @returns Schema drift analysis
 *
 * @example
 * const drift = await analyzeSchemaD rift('v1.0.0', 'v1.1.0');
 */
export async function analyzeSchemaDrift(
  sourceVersion: string,
  targetVersion: string
): Promise<import('./types').SchemaDriftAnalysis> {
  return api.get(`/lineage/schema/drift/${sourceVersion}/${targetVersion}`);
}

/**
 * Analyze migration impact
 *
 * Analyzes the impact of a proposed schema migration
 *
 * @param request - Migration impact request
 * @returns Migration impact analysis
 *
 * @example
 * const impact = await analyzeMigrationImpact({
 *   datasource_id: 'db2_prod',
 *   source_version: 'v1.0.0',
 *   target_version: 'v1.1.0',
 *   proposed_changes: [...]
 * });
 */
export async function analyzeMigrationImpact(
  request: import('./types').MigrationImpactRequest
): Promise<import('./types').MigrationImpactAnalysis> {
  return api.post('/lineage/schema/impact', request);
}
