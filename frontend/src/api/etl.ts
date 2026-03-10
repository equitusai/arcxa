/**
 * ETL (Extract-Transform-Load) API Module
 *
 * Provides API functions for ETL workflow operations:
 * - CSV/File operations (extract, scan, export)
 * - Database operations (extract, load)
 * - Field transformations
 * - Data validation and quality
 * - Deduplication
 * - Semantic mapping integration
 *
 * Integrates with existing field-mapping API for semantic mapper nodes.
 */

import { api } from './client';
import {
  analyzeForMapping,
  applyMappings,
  getMappingSession,
} from './field-mapping';
import type {
  CSVSourceConfig,
  DBExtractConfig,
  SemanticMapperConfig,
  FieldTransformerConfig,
  DataJoinerConfig,
  AggregatorConfig,
  DataValidatorConfig,
  DeduplicatorConfig,
  RDFLoaderConfig,
  DBLoaderConfig,
  CSVExporterConfig,
  SchedulerConfig,
} from '@/lib/workflow-etl-config';

// ============================================================================
// CSV Operations
// ============================================================================

export interface ScanCSVRequest {
  file_path: string;
  delimiter?: string;
  has_header?: boolean;
  encoding?: string;
  sample_rows?: number;  // Number of rows to analyze for type inference
}

export interface ScanCSVResponse {
  detected_fields: Array<{
    name: string;
    type: string;
    sample_values?: string[];
    nullable: boolean;
  }>;
  ontology_mappings?: Array<{
    field_name: string;
    ontology_id: string;
    concept_uri: string;
    concept_label: string;
    similarity: number;
    confidence: number;
    method: string;
    mapped_at: string;
  }>;
  total_rows?: number;
  scan_timestamp: string;
  delimiter_detected?: string;
  encoding_detected?: string;
}

/**
 * Scan CSV file and detect fields/schema
 *
 * @param request - CSV scan configuration
 * @returns Detected schema and metadata
 */
export async function scanCSV(request: ScanCSVRequest): Promise<ScanCSVResponse> {
  return api.post('/etl/csv/scan', request);
}

export interface ImportCSVRequest extends CSVSourceConfig {
  target_workflow?: string;  // Optional: attach to workflow
}

export interface ImportCSVResponse {
  import_id: string;
  rows_imported: number;
  fields_imported: number;
  duration_ms: number;
  data_preview?: any[];  // First few rows
}

/**
 * Import CSV data for ETL processing
 *
 * @param request - CSV import configuration
 * @returns Import job result
 */
export async function importCSV(request: ImportCSVRequest): Promise<ImportCSVResponse> {
  return api.post('/etl/csv/import', request);
}

export interface ExportCSVRequest extends CSVExporterConfig {
  source_data: any[];  // Data to export
}

export interface ExportCSVResponse {
  export_id: string;
  file_path: string;
  rows_exported: number;
  file_size_bytes: number;
  duration_ms: number;
}

/**
 * Export data to CSV file
 *
 * @param request - CSV export configuration
 * @returns Export job result
 */
export async function exportCSV(request: ExportCSVRequest): Promise<ExportCSVResponse> {
  return api.post('/etl/csv/export', request);
}

// ============================================================================
// Database Extract/Load Operations
// ============================================================================

export interface ExtractFromDatabaseRequest extends DBExtractConfig {
  limit?: number;  // Optional: max rows for testing
}

export interface ExtractFromDatabaseResponse {
  extract_id: string;
  rows_extracted: number;
  columns: string[];
  data_preview?: any[];
  duration_ms: number;
  incremental_metadata?: {
    last_value: any;
    next_run_watermark: any;
  };
}

/**
 * Extract data from datasource
 *
 * @param request - Database extraction configuration
 * @returns Extracted data metadata
 */
export async function extractFromDatabase(
  request: ExtractFromDatabaseRequest
): Promise<ExtractFromDatabaseResponse> {
  return api.post('/etl/database/extract', request);
}

export interface LoadToDatabaseRequest extends DBLoaderConfig {
  source_data: any[];  // Data to load
}

export interface LoadToDatabaseResponse {
  load_id: string;
  rows_loaded: number;
  rows_updated?: number;  // For upsert mode
  rows_inserted?: number;  // For upsert mode
  rows_replaced?: number;  // For replace mode
  duration_ms: number;
  batches_processed: number;
}

/**
 * Load data to database
 *
 * @param request - Database load configuration
 * @returns Load job result
 */
export async function loadToDatabase(
  request: LoadToDatabaseRequest
): Promise<LoadToDatabaseResponse> {
  return api.post('/etl/database/load', request);
}

// ============================================================================
// Semantic Mapping Integration
// ============================================================================

/**
 * Get field mapping session for semantic mapper node
 * Delegates to field-mapping API
 *
 * @param sessionId - Mapping session identifier
 * @returns Mapping session with candidates and status
 */
export async function getSemanticMappingSession(sessionId: string) {
  return getMappingSession(sessionId);
}

/**
 * Create semantic mapping session from ETL node config
 *
 * @param datasourceId - Source datasource
 * @param config - Semantic mapper configuration
 * @returns Session ID and summary
 */
export async function createSemanticMapping(
  datasourceId: string,
  config: SemanticMapperConfig
) {
  return analyzeForMapping(datasourceId, {
    user_id: 'current_user',  // TODO: Get from auth store
    auto_approve_threshold: config.auto_approve_threshold,
    ontology_namespaces: config.target_ontology,
    max_candidates: 10,
    sample_size: 1000,
  });
}

/**
 * Apply semantic mappings
 *
 * @param sessionId - Mapping session identifier
 * @returns Application result
 */
export async function applySemanticMappings(sessionId: string) {
  return applyMappings(sessionId, {
    create_default_import: true,
  });
}

// ============================================================================
// Field Transformations
// ============================================================================

export interface ApplyTransformationsRequest extends FieldTransformerConfig {
  source_data: any[];
}

export interface ApplyTransformationsResponse {
  transform_id: string;
  rows_transformed: number;
  transformations_applied: number;
  data_preview?: any[];
  duration_ms: number;
}

/**
 * Apply field transformations to data
 *
 * @param request - Transformation configuration
 * @returns Transformed data result
 */
export async function applyTransformations(
  request: ApplyTransformationsRequest
): Promise<ApplyTransformationsResponse> {
  return api.post('/etl/transform/apply', request);
}

// ============================================================================
// Data Joining
// ============================================================================

export interface JoinDataRequest extends DataJoinerConfig {
  left_data: any[];
  right_data: any[];
}

export interface JoinDataResponse {
  join_id: string;
  rows_output: number;
  left_rows: number;
  right_rows: number;
  matched_rows: number;
  unmatched_left?: number;
  unmatched_right?: number;
  data_preview?: any[];
  duration_ms: number;
}

/**
 * Join two datasets
 *
 * @param request - Join configuration
 * @returns Joined data result
 */
export async function joinData(request: JoinDataRequest): Promise<JoinDataResponse> {
  return api.post('/etl/transform/join', request);
}

// ============================================================================
// Aggregation
// ============================================================================

export interface AggregateDataRequest extends AggregatorConfig {
  source_data: any[];
}

export interface AggregateDataResponse {
  aggregate_id: string;
  rows_output: number;
  rows_input: number;
  groups_created: number;
  data_preview?: any[];
  duration_ms: number;
}

/**
 * Aggregate data with group by
 *
 * @param request - Aggregation configuration
 * @returns Aggregated data result
 */
export async function aggregateData(
  request: AggregateDataRequest
): Promise<AggregateDataResponse> {
  return api.post('/etl/transform/aggregate', request);
}

// ============================================================================
// Data Validation
// ============================================================================

export interface ValidateDataRequest extends DataValidatorConfig {
  source_data: any[];
}

export interface ValidateDataResponse {
  validation_id: string;
  rows_validated: number;
  rows_valid: number;
  rows_invalid: number;
  violations: Array<{
    row_index: number;
    field: string;
    rule: string;
    message: string;
    severity: 'error' | 'warning';
  }>;
  valid_data_preview?: any[];
  invalid_data_preview?: any[];
  duration_ms: number;
}

/**
 * Validate data against rules
 *
 * @param request - Validation configuration
 * @returns Validation results
 */
export async function validateData(
  request: ValidateDataRequest
): Promise<ValidateDataResponse> {
  return api.post('/etl/quality/validate', request);
}

// ============================================================================
// Deduplication
// ============================================================================

export interface DeduplicateDataRequest extends DeduplicatorConfig {
  source_data: any[];
}

export interface DeduplicateDataResponse {
  dedupe_id: string;
  rows_input: number;
  rows_output: number;
  duplicates_removed: number;
  duplicate_groups: number;
  data_preview?: any[];
  duration_ms: number;
}

/**
 * Remove duplicates from data
 *
 * @param request - Deduplication configuration
 * @returns Deduplicated data result
 */
export async function deduplicateData(
  request: DeduplicateDataRequest
): Promise<DeduplicateDataResponse> {
  return api.post('/etl/quality/deduplicate', request);
}

// ============================================================================
// RDF Loading
// ============================================================================

export interface LoadToRDFRequest extends RDFLoaderConfig {
  source_data: any[];
}

export interface LoadToRDFResponse {
  load_id: string;
  entities_created: number;
  triples_stored: number;
  lineage_captured: boolean;
  target_graph: string;
  duration_ms: number;
}

/**
 * Load data to RDF triple store
 *
 * @param request - RDF load configuration
 * @returns RDF load result
 */
export async function loadToRDF(request: LoadToRDFRequest): Promise<LoadToRDFResponse> {
  return api.post('/etl/load/rdf', request);
}

// ============================================================================
// Workflow Scheduling
// ============================================================================

export interface CreateScheduleRequest extends SchedulerConfig {
  workflow_id: string;
}

export interface CreateScheduleResponse {
  schedule_id: string;
  workflow_id: string;
  next_run: string;
  cron_expression?: string;
  interval_seconds?: number;
  enabled: boolean;
}

/**
 * Create workflow schedule
 *
 * @param request - Schedule configuration
 * @returns Schedule confirmation
 */
export async function createSchedule(
  request: CreateScheduleRequest
): Promise<CreateScheduleResponse> {
  return api.post('/etl/schedule/create', request);
}

export interface ScheduleInfo {
  schedule_id: string;
  workflow_id: string;
  enabled: boolean;
  next_run?: string;
  last_run?: string;
  run_count: number;
  config: SchedulerConfig;
}

/**
 * Get workflow schedule information
 *
 * @param workflowId - Workflow identifier
 * @returns Schedule details
 */
export async function getSchedule(workflowId: string): Promise<ScheduleInfo> {
  return api.get(`/etl/schedule/${workflowId}`);
}

/**
 * Enable/disable workflow schedule
 *
 * @param scheduleId - Schedule identifier
 * @param enabled - True to enable, false to disable
 */
export async function updateScheduleStatus(
  scheduleId: string,
  enabled: boolean
): Promise<void> {
  return api.patch(`/etl/schedule/${scheduleId}`, { enabled });
}

/**
 * Delete workflow schedule
 *
 * @param scheduleId - Schedule identifier
 */
export async function deleteSchedule(scheduleId: string): Promise<void> {
  return api.delete(`/etl/schedule/${scheduleId}`);
}

// ============================================================================
// ETL Job Monitoring
// ============================================================================

export interface ETLJobStatus {
  job_id: string;
  job_type: 'extract' | 'transform' | 'load' | 'validate' | 'dedupe';
  status: 'pending' | 'running' | 'completed' | 'failed';
  progress_percent: number;
  rows_processed: number;
  started_at: string;
  completed_at?: string;
  error?: {
    message: string;
    details?: string;
  };
}

/**
 * Get ETL job status
 *
 * @param jobId - Job identifier
 * @returns Job status and progress
 */
export async function getETLJobStatus(jobId: string): Promise<ETLJobStatus> {
  return api.get(`/etl/jobs/${jobId}/status`);
}

/**
 * Cancel running ETL job
 *
 * @param jobId - Job identifier
 */
export async function cancelETLJob(jobId: string): Promise<void> {
  return api.post(`/etl/jobs/${jobId}/cancel`, {});
}

// ============================================================================
// Data Profiling & Preview
// ============================================================================

export interface DataProfileRequest {
  datasource_id: string;
  table_name?: string;
  sample_size?: number;
}

export interface ColumnProfile {
  column_name: string;
  data_type: string;
  distinct_count: number;
  null_count: number;
  min_value?: any;
  max_value?: any;
  avg_value?: number;
  top_values?: Array<{ value: any; count: number }>;
}

export interface DataProfileResponse {
  datasource_id: string;
  table_name: string;
  total_rows: number;
  total_columns: number;
  column_profiles: ColumnProfile[];
  profiled_at: string;
}

/**
 * Profile datasource data
 *
 * @param request - Profiling configuration
 * @returns Data profile statistics
 */
export async function profileData(
  request: DataProfileRequest
): Promise<DataProfileResponse> {
  return api.post('/etl/profile', request);
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Format file size for display
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

/**
 * Format row count for display
 */
export function formatRowCount(count: number): string {
  if (count < 1000) return count.toString();
  if (count < 1000000) return `${(count / 1000).toFixed(1)}K`;
  if (count < 1000000000) return `${(count / 1000000).toFixed(1)}M`;
  return `${(count / 1000000000).toFixed(1)}B`;
}

/**
 * Estimate processing time based on row count and operation
 */
export function estimateProcessingTime(
  rowCount: number,
  operation: 'extract' | 'transform' | 'load' | 'validate'
): number {
  // Rough estimates in milliseconds per 1000 rows
  const rates = {
    extract: 100,
    transform: 200,
    load: 300,
    validate: 150,
  };

  return Math.ceil((rowCount / 1000) * rates[operation]);
}

/**
 * Validate CSV file path format
 */
export function isValidCSVPath(path: string): boolean {
  return /\.(csv|tsv|txt)$/i.test(path) && path.length > 0;
}

/**
 * Parse CSV delimiter from name
 */
export function parseDelimiter(name: string): string {
  if (name.toLowerCase().includes('tab') || name.endsWith('.tsv')) {
    return '\t';
  }
  if (name.toLowerCase().includes('pipe')) {
    return '|';
  }
  return ',';
}
