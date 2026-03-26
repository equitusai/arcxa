/**
 * TypeScript type definitions for Graphica API
 *
 * Generated from backend REST API schema
 */

import { User, UserRole } from '@/stores/auth';

// ============================================================================
// Common Types
// ============================================================================

export interface ApiError {
  error: string;
  code?: string;
  details?: Record<string, any>;
}

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface PaginationParams {
  page?: number;
  page_size?: number;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  page_size: number;
  has_next: boolean;
  has_previous: boolean;
}

// ============================================================================
// Authentication Types
// ============================================================================

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  expires_at: string;
  role: string;
}

export interface SetupAdminRequest {
  username: string;
  password: string;
  email?: string;
  setup_token: string;
}

export interface CreateUserRequest {
  username: string;
  password: string;
  role: UserRole;
  email?: string;
  full_name?: string;
}

export interface CreateUserResponse {
  user_id: string;
  username: string;
  role: UserRole;
  created_at: string;
}

// ============================================================================
// Lineage Types
// ============================================================================

export type LineageOperation = 'CREATE' | 'UPDATE' | 'DELETE' | 'READ';

export interface LineageEvent {
  id: string;
  record_id: string;
  dataset: string;
  operation: LineageOperation;
  timestamp: string;
  model_id?: string;
  model_version?: string;
  user_id?: string;
  metadata?: Record<string, any>;
  parent_record_ids?: string[];
}

export interface LineageResponse {
  record_id: string;
  events: LineageEvent[];
  total_count: number;
}

export interface ModelImpactQuery {
  version: string;
  timestamp?: string;
}

export interface ModelImpactResponse {
  model_id: string;
  model_version: string;
  affected_datasets: string[];
  affected_records: number;
  impact_events: LineageEvent[];
}

export interface ImpactAnalysisRequest {
  entity_uri: string;
  timestamp?: string;
  max_depth?: number;
}

export interface ImpactAnalysisResponse {
  entity_uri: string;
  downstream_entities: string[];
  total_affected: number;
  impact_graph: Record<string, string[]>;
}

// Row-Level Lineage Types

export interface RowLineageEvent {
  event_id: string;
  timestamp: string;
  operation: string;
  source_dataset?: string;
  target_dataset?: string;
  job_id: string;
  batch_id?: string;
  status: 'SUCCESS' | 'FAILED' | 'PARTIAL';
  rows_processed: number;
  quality_score?: number;
  entity_uri?: string;
  transformations_applied?: Array<{
    type: string;
    field?: string;
    rule?: string;
    result?: string;
    ontology?: string;
    mapped_class?: string;
    confidence?: number;
  }>;
}

export interface RowLineageResponse {
  row_key: string;
  events: RowLineageEvent[];
  metadata: {
    total_events: number;
    total_transformations: number;
    data_quality_score?: number;
    processing_duration_ms?: number;
    current_status: string;
  };
}

export interface RowJourneyNode {
  node_id: string;
  dataset: string;
  operation: string;
  timestamp: string;
  status: string;
  node_type: 'source' | 'intermediate' | 'destination';
  metadata?: Record<string, any>;
}

export interface RowJourneyEdge {
  edge_id: string;
  source: string;
  target: string;
  operation: string;
  timestamp: string;
  confidence: number;
  transformation?: Record<string, any>;
}

export interface RowJourneyResponse {
  row_key: string;
  journey: {
    nodes: RowJourneyNode[];
    edges: RowJourneyEdge[];
  };
  metadata: {
    total_nodes: number;
    total_edges: number;
    journey_duration_ms: number;
    first_seen: string;
    last_updated: string;
    current_status: string;
  };
}

export interface BatchLineageRow {
  row_key: string;
  status: 'SUCCESS' | 'FAILED' | 'PARTIAL';
  processing_time_ms: number;
  transformations_applied: number;
  quality_score?: number;
  entity_uri?: string;
  error?: string;
}

export interface BatchLineageResponse {
  batch_id: string;
  job_id: string;
  started_at: string;
  completed_at: string;
  status: 'SUCCESS' | 'FAILED' | 'PARTIAL';
  rows: BatchLineageRow[];
  summary: {
    total_rows: number;
    successful_rows: number;
    failed_rows: number;
    success_rate: number;
    avg_processing_time_ms: number;
    avg_quality_score?: number;
    total_transformations: number;
  };
  pagination: {
    limit: number;
    offset: number;
    total: number;
    has_more: boolean;
  };
}

export interface JobStatsResponse {
  job_id: string;
  job_name: string;
  started_at: string;
  completed_at: string;
  duration_ms: number;
  status: 'SUCCESS' | 'FAILED' | 'PARTIAL' | 'RUNNING';
  statistics: {
    total_rows_processed: number;
    successful_rows: number;
    failed_rows: number;
    skipped_rows: number;
    success_rate: number;
    rows_per_second: number;
    avg_processing_time_ms: number;
    total_transformations: number;
    transformations_per_row: number;
  };
  quality_metrics?: {
    avg_quality_score: number;
    min_quality_score: number;
    max_quality_score: number;
    quality_distribution: {
      excellent: number;
      good: number;
      fair: number;
      poor: number;
    };
  };
  operations: Record<string, {
    count: number;
    success_rate: number;
    avg_duration_ms: number;
  }>;
  datasets: {
    sources: string[];
    intermediates: string[];
    destinations: string[];
  };
  errors?: Array<{
    error_type: string;
    count: number;
    sample_row_keys: string[];
  }>;
}

export interface FilteredRowsResponse {
  job_id: string;
  filters: Record<string, any>;
  rows: Array<{
    row_key: string;
    status: 'SUCCESS' | 'FAILED' | 'PARTIAL';
    quality_score?: number;
    failed_at_operation?: string;
    failed_at_dataset?: string;
    error_type?: string;
    error_message?: string;
    timestamp: string;
    processing_time_ms: number;
    transformations_completed: number;
  }>;
  summary: {
    total_matching: number;
    returned: number;
    limit: number;
    offset: number;
    has_more: boolean;
  };
}

export interface RunLineageArtifact {
  artifact_id: string;
  type: string;
  name: string;
  location?: string;
  rows?: number;
  size_bytes?: number;
  quality_score?: number;
  triple_count?: number;
  entity_count?: number;
}

export interface RunLineageStep {
  step_id: string;
  step_name: string;
  step_type: string;
  started_at: string;
  completed_at: string;
  status: 'SUCCESS' | 'FAILED' | 'RUNNING';
  input_artifacts: RunLineageArtifact[];
  output_artifacts: RunLineageArtifact[];
  metrics: Record<string, number>;
}

export interface RunLineageResponse {
  run_id: string;
  workflow_id: string;
  workflow_name: string;
  started_at: string;
  completed_at: string;
  status: 'SUCCESS' | 'FAILED' | 'RUNNING' | 'CANCELLED';
  triggered_by: string;
  steps: RunLineageStep[];
  summary: {
    total_steps: number;
    successful_steps: number;
    failed_steps: number;
    total_duration_ms: number;
    total_rows_processed: number;
    final_rows: number;
    data_quality_score?: number;
  };
  lineage_graph?: {
    nodes: Array<{ id: string; type: string; name: string }>;
    edges: Array<{ source: string; target: string; step: string }>;
  };
}

// Time Range Lineage Types

export interface TimeRangeLineageQuery {
  start: string; // ISO 8601 timestamp
  end: string; // ISO 8601 timestamp
  dataset?: string; // Optional dataset filter
  limit?: number; // Maximum results
}

export interface TimeRangeLineageResponse {
  start: string;
  end: string;
  total_events: number;
  events: Array<{
    record_id: string;
    dataset: string;
    run_id: string;
    tenant_id: string;
    timestamp: string;
    sources: Array<{
      system: string;
      path: string;
      version?: string;
      extracted_at: string;
    }>;
    transforms: Array<{
      id: string;
      transform_type: string;
      rule_id: string;
      version: string;
      applied_at: string;
    }>;
    models: Array<{
      model_id: string;
      version: string;
      model_type: string;
      inference_at: string;
    }>;
    output: {
      system: string;
      path: string;
      version?: string;
      extracted_at: string;
    };
    metadata: Record<string, string>;
  }>;
  datasets: string[];
}

export interface LineageGraphResponse {
  root_record_id: string;
  events: TimeRangeLineageResponse['events'];
  upstream_records: string[];
  downstream_records: string[];
  lineage_depth: number;
  total_events: number;
  statistics: {
    source_systems: number;
    transform_count: number;
    model_count: number;
    output_systems: number;
    has_circular_dependency: boolean;
  };
}

// Column-Level Lineage Types

export interface ColumnRef {
  datasource_id: string;
  schema?: string;
  table_name: string;
  column_name: string;
  data_type: string;
}

export interface ColumnLineageEvent {
  event_id: string;
  target_column: ColumnRef;
  source_columns: ColumnRef[];
  transformation_type: string;
  transformation_expression?: string;
  job_id: string;
  timestamp: string;
  confidence: number;
  metadata?: Record<string, any>;
}

export interface ColumnLineageNode {
  column: ColumnRef;
  node_type: 'source' | 'intermediate' | 'target';
  depth: number;
}

export interface ColumnLineageEdge {
  source: ColumnRef;
  target: ColumnRef;
  transformation_type: string;
  transformation_expression?: string;
  confidence: number;
}

export interface ColumnLineageGraph {
  target_column: ColumnRef;
  nodes: ColumnLineageNode[];
  edges: ColumnLineageEdge[];
  max_depth: number;
  total_source_columns: number;
}

export interface ColumnImpactRequest {
  column: ColumnRef;
  change_type: 'data_type_change' | 'rename' | 'delete' | 'add_constraint';
  proposed_change: Record<string, any>;
  max_depth?: number;
}

export interface ColumnImpactAnalysis {
  impacted_column: ColumnRef;
  change_type: string;
  directly_affected_columns: ColumnRef[];
  indirectly_affected_columns: ColumnRef[];
  total_affected: number;
  breaking_changes: Array<{
    column: ColumnRef;
    reason: string;
    severity: 'low' | 'medium' | 'high' | 'critical';
  }>;
  suggested_actions: string[];
}

// Schema Evolution Types

export type SchemaChangeType =
  | 'add_column'
  | 'drop_column'
  | 'rename_column'
  | 'change_data_type'
  | 'add_constraint'
  | 'drop_constraint'
  | 'add_table'
  | 'drop_table'
  | 'rename_table';

export interface SchemaChangeEvent {
  id?: string;
  datasource_id: string;
  table_name: string;
  change_type: SchemaChangeType;
  column_name?: string;
  old_data_type?: string;
  new_data_type?: string;
  old_column_name?: string;
  new_column_name?: string;
  constraint_definition?: string;
  timestamp?: string;
  applied_by?: string;
  description?: string;
  metadata?: Record<string, any>;
}

export interface SchemaSnapshot {
  tables: Array<{
    table_name: string;
    columns: Array<{
      column_name: string;
      data_type: string;
      nullable: boolean;
      default_value?: string;
      constraints?: string[];
    }>;
    primary_key?: string[];
    foreign_keys?: Array<{
      column: string;
      referenced_table: string;
      referenced_column: string;
    }>;
    indexes?: Array<{
      name: string;
      columns: string[];
      unique: boolean;
    }>;
  }>;
}

export interface SchemaVersion {
  id?: string;
  datasource_id: string;
  version_id: string;
  captured_at: string;
  schema_snapshot: SchemaSnapshot;
  description?: string;
  tags?: string[];
}

export interface SchemaDriftAnalysis {
  source_version: string;
  target_version: string;
  drift_detected: boolean;
  changes: SchemaChangeEvent[];
  breaking_changes: SchemaChangeEvent[];
  backward_compatible: boolean;
  total_changes: number;
  severity: 'none' | 'low' | 'medium' | 'high' | 'critical';
  recommendations: string[];
}

export interface MigrationImpactRequest {
  datasource_id: string;
  source_version: string;
  target_version: string;
  proposed_changes: SchemaChangeEvent[];
  analyze_downstream?: boolean;
}

export interface MigrationImpactAnalysis {
  datasource_id: string;
  source_version: string;
  target_version: string;
  proposed_changes: SchemaChangeEvent[];
  impacted_tables: string[];
  impacted_columns: ColumnRef[];
  downstream_pipelines: Array<{
    pipeline_id: string;
    pipeline_name: string;
    impact_severity: 'low' | 'medium' | 'high' | 'critical';
    affected_steps: string[];
    breaking: boolean;
  }>;
  estimated_downtime_minutes?: number;
  rollback_plan?: string[];
  validation_queries?: string[];
  total_impact_score: number;
  recommended_order: string[];
}

// ============================================================================
// Quality Types
// ============================================================================

export interface QualityScorecard {
  dataset: string;
  overall_score: number;
  dimensions: {
    completeness: number;
    accuracy: number;
    consistency: number;
    timeliness: number;
    validity: number;
  };
  total_violations: number;
  last_assessed: string;
}

export interface QualityViolation {
  id: string;
  rule_id: string;
  dataset: string;
  record_id?: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  message: string;
  detected_at: string;
  resolved: boolean;
}

export interface QualityRule {
  id: string;
  name: string;
  description?: string;
  dimension: 'completeness' | 'accuracy' | 'consistency' | 'timeliness' | 'validity';
  severity: 'low' | 'medium' | 'high' | 'critical';
  condition: string; // SPARQL or SQL condition
  enabled: boolean;
  created_at: string;
}

export interface CreateQualityRuleRequest {
  name: string;
  description?: string;
  dimension: QualityRule['dimension'];
  severity: QualityRule['severity'];
  condition: string;
  enabled?: boolean;
}

// ============================================================================
// Model Types
// ============================================================================

export interface Model {
  id: string;
  name: string;
  version: string;
  type: 'ml' | 'rule' | 'statistical';
  framework?: string;
  description?: string;
  training_dataset?: string;
  features?: string[];
  metrics?: Record<string, number>;
  created_at: string;
  updated_at?: string;
  status: 'active' | 'deprecated' | 'archived';
}

export interface RegisterRdfModelRequest {
  id: string;
  name: string;
  version: string;
  type: Model['type'];
  framework?: string;
  description?: string;
  training_dataset?: string;
  features?: string[];
  metrics?: Record<string, number>;
}

export interface ModelTrainingDataResponse {
  model_id: string;
  model_version: string;
  training_dataset: string;
  records: any[];
  as_of_timestamp: string;
}

// ============================================================================
// Dataset Types
// ============================================================================

export interface Dataset {
  id: string;
  name: string;
  description?: string;
  schema?: DatasetSchema;
  record_count: number;
  size_bytes?: number;
  created_at: string;
  updated_at?: string;
  tags?: string[];
  // Backend RDF fields (new)
  dataset_type?: 'source' | 'imported' | 'workflow_output' | 'training_data' | 'fusion_result';
  asset_kind?: 'source_asset' | 'materialized_dataset';
  last_ingested_at?: string;
  source_datasource_id?: string;
  workflow_execution_id?: string;
  // Enhanced fields for Data Catalogue
  source?: string; // datasource ID (alias for source_datasource_id)
  source_name?: string; // datasource display name
  entity_count?: number; // alias for record_count
  quality_score?: number;
  quality_breakdown?: QualityBreakdown;
  fusion_candidates?: number;
  last_updated?: string; // alias for updated_at
  status?: 'active' | 'stale' | 'error';
}

export interface QualityBreakdown {
  completeness: number;
  validity: number;
  uniqueness: number;
  timeliness: number;
}

export interface DatasetSchema {
  fields: DatasetField[];
  primary_key?: string[];
}

export interface DatasetField {
  name: string;
  type: string;
  nullable: boolean;
  description?: string;
  completeness?: number; // for quality metrics
}

export interface DatasetListResponse {
  datasets: Dataset[];
  total: number;
  page?: number;
  page_size?: number;
}

export interface DatasetStats {
  total_entities: number;
  avg_confidence: number;
  fusion_operations: {
    total_committed: number;
    pending_candidates: number;
    last_fusion_at?: string;
  };
  workflows: {
    active_count: number;
    total_executions: number;
    last_execution_at?: string;
  };
  quality_metrics?: QualityBreakdown;
}

// ============================================================================
// Governance/RDF Types
// ============================================================================

export interface SparqlQueryRequest {
  sparql: string;  // Backend expects "sparql" field, not "query"
  format?: string; // Output format: "json", "xml", "csv", "tsv"
  timeout_ms?: number;
  max_results?: number;
}

export interface SparqlQueryResponse {
  results: Record<string, any>[];
  query_time_ms: number;
  total_results: number;
}

export interface RdfStatsResponse {
  total_triples: number;
  store_type: string;
  materialization_enabled: boolean;
}

// Type alias for consistency
export type RdfStoreStats = RdfStatsResponse;

// ============================================================================
// Entity Types (matching backend exactly)
// ============================================================================

export interface DerivedAttribute {
  name: string;
  value: string;
  confidence: number;
  model_id?: string;
  timestamp: string;
}

export interface EntityResponse {
  entity_id: string;
  entity_type?: string;
  properties: Record<string, any>;
  derived_attributes: DerivedAttribute[];
}

export interface EntityAttributesResponse {
  entity_id: string;
  attributes: DerivedAttribute[];
  total: number;
}

export interface EntityLineageResponse {
  entity_id: string;
  lineage_graph: any[];
  format: string;
}

export interface AttributeDatapoint {
  timestamp: string;
  value: string;
  confidence: number;
  model_id?: string;
}

export interface AttributeTimeseriesResponse {
  entity_id: string;
  attribute_name: string;
  datapoints: AttributeDatapoint[];
  total: number;
}

// Frontend-only types for UI
export interface Entity {
  id: string;
  entity_type?: string;
  domain?: string;
  attribute_count: number;
  avg_confidence: number;
  created_at?: string;
  status?: 'active' | 'review' | 'archived';
  source_count?: number; // Number of datasources this entity was fused from (1 = raw, N = resolved)
  source_ids?: string[]; // IDs of source entities that were fused (for resolved entities)
  fusion_rule?: string; // Matching rule used for fusion (for resolved entities)
  fusion_confidence?: number; // Confidence score of the fusion (for resolved entities)
  fusion_date?: string; // When the fusion was committed (for resolved entities)
}

export interface EntityQueryParams {
  domain?: string;
  limit?: number;
  offset?: number;
  min_confidence?: number;
}

// ============================================================================
// Model Registry Types (matching backend exactly)
// ============================================================================

export type ModelProtocol = 'http' | 'grpc' | 'lambda';

export type ServingFramework = 'tensorflow' | 'torch' | 'sagemaker' | 'custom';

export type FeatureDataType = 'string' | 'integer' | 'float' | 'boolean' | 'array' | 'object';

export type CircuitState = 'closed' | 'open' | 'half_open';

export interface ModelEndpoint {
  protocol: ModelProtocol;
  url: string;
  timeout_ms: number;
  headers?: Record<string, string>;
}

export interface FeatureSchema {
  name: string;
  data_type: FeatureDataType;
  required: boolean;
}

export interface ModelMetadata {
  id: string;
  name: string;
  version: string;
  endpoint: ModelEndpoint;
  framework: ServingFramework;
  input_schema: FeatureSchema[];
  output_schema: string[];
  created_at: string;
  updated_at: string;
}

export interface ModelSummary {
  id: string;
  name: string;
  version: string;
  protocol: ModelProtocol;
}

export interface RegisterModelRequest {
  id: string;
  name: string;
  version: string;
  endpoint: {
    protocol: string;
    url: string;
    timeout_ms: number;
    headers?: Record<string, string>;
  };
  framework: string;
  input_schema: Array<{
    name: string;
    data_type: string;
    required: boolean;
  }>;
  output_schema: string[];
  description?: string;
  tags?: string[];
  circuitBreaker?: {
    enabled: boolean;
    failureThreshold: number;
    successThreshold: number;
    timeoutMs: number;
  };
  retry?: {
    enabled: boolean;
    maxAttempts: number;
  };
  cache?: {
    enabled: boolean;
    ttlSeconds: number;
  };
}

export interface CircuitBreakerStatus {
  state: CircuitState;
  failure_count: number;
  success_count: number;
  last_failure_time?: string;
}

export interface ModelCacheStats {
  hits: number;
  misses: number;
  hit_rate: number;
  entries: number;
  memory_bytes?: number;
}

export interface ModelInvocationRequest {
  model_id: string;
  features: Record<string, any>;
  bypass_cache?: boolean;
}

export interface ModelInvocationResponse {
  predictions: Record<string, any>;
  latency_ms: number;
  from_cache: boolean;
}

// ============================================================================
// Workflow Orchestration Types (matching backend exactly)
// ============================================================================

export type StepType =
  // ML/Fusion nodes
  | 'ml_prediction'
  | 'heuristic_rule'
  | 'wasm_rule'
  | 'confidence_gate'
  | 'weighted_vote'
  | 'confidence_aggregate'
  | 'conditional_router'
  | 'field_mapper'
  | 'data_transformer'
  // ETL Extract nodes
  | 'csv_source'
  | 'db_extract'
  | 'multi_source_input'
  // ETL Transform nodes
  | 'semantic_mapper'
  | 'field_transformer'
  | 'data_joiner'
  | 'aggregator'
  // ETL Quality nodes
  | 'data_validator'
  | 'deduplicator'
  // ETL Load nodes
  | 'rdf_loader'
  | 'db_loader'
  | 'csv_exporter'
  // ETL Orchestration
  | 'scheduler';

export type FallbackStrategy = 'manual_review' | 'reject_fusion' | 'accept_fusion';

export interface MLPredictionConfig {
  model_id: string;
  features?: string[];
  timeout_ms?: number;
  cache_ttl_secs?: number;
}

export interface HeuristicConfig {
  rule_id: string;
  min_confidence?: number;
}

export interface WasmRuleConfig {
  rule_id: string;
}

export interface ConfidenceGateConfig {
  threshold: number;
  input_step?: string;
}

export interface WeightedVoteConfig {
  weights: Record<string, number>;
}

export interface ConfidenceAggregateConfig {
  method: string; // 'weighted_average' | 'bayesian' | 'voting'
  inputs?: string[];
}

export interface ConditionalRouterConfig {
  condition: string; // Expression to evaluate (e.g., "confidence >= 0.90")
  true_branch?: string; // Step ID to route to if true
  false_branch?: string; // Step ID to route to if false
}

export interface FieldMappingSource {
  source_id: string; // Datasource or field identifier
  source_field: string; // Field name in source
  weight: number; // Weight for voting (0.0 to 1.0)
}

export interface FieldMapperConfig {
  target_field: string; // Ontology field to map to (e.g., "schema:streetAddress")
  sources: FieldMappingSource[]; // Array of source mappings with weights
  aggregation_method: 'weighted_vote' | 'highest_confidence' | 'most_recent' | 'manual_priority';
  min_confidence?: number; // Minimum confidence threshold
}

export interface DataTransformerConfig {
  operations: Array<{
    type: 'normalize' | 'validate' | 'clean' | 'format' | 'extract';
    field: string;
    parameters?: Record<string, any>;
  }>;
}

export type StepConfig =
  | { type: 'ml_prediction'; config: MLPredictionConfig }
  | { type: 'heuristic_rule'; config: HeuristicConfig }
  | { type: 'wasm_rule'; config: WasmRuleConfig }
  | { type: 'confidence_gate'; config: ConfidenceGateConfig }
  | { type: 'weighted_vote'; config: WeightedVoteConfig }
  | { type: 'confidence_aggregate'; config: ConfidenceAggregateConfig }
  | { type: 'conditional_router'; config: ConditionalRouterConfig }
  | { type: 'field_mapper'; config: FieldMapperConfig }
  | { type: 'data_transformer'; config: DataTransformerConfig };

export interface WorkflowStep {
  id: string;
  step_type: StepType;
  config: any; // Will be one of the config types above
  depends_on?: string[];
}

export interface WorkflowDefinition {
  steps: WorkflowStep[];
  fusion_threshold?: number;
  fallback?: FallbackStrategy;
}

export interface WorkflowMetadata {
  id: string;
  name: string;
  created_at: string;
  updated_at?: string;
  version?: number;
}

export interface Workflow {
  id: string;
  name: string;
  description?: string;
  tags?: string[];
  definition: WorkflowDefinition;
  created_at?: string;
  updated_at?: string;
  version?: string;
  execution_count?: number;
  last_executed_at?: string;
}

export interface RegisterWorkflowRequest {
  id?: string;
  name: string;
  definition: WorkflowDefinition;
  description?: string;
  tags?: string[];
}

export interface RegisterWorkflowResponse {
  workflow_id: string;
  name: string;
  created_at: string;
}

export interface WorkflowExecutionContext {
  request_id?: string;
  initiator?: string;
  metadata?: Record<string, any>;
}

export interface WorkflowJsonInput {
  type: 'json';
  data: any;
}

export interface WorkflowDatasetInput {
  type: 'dataset';
  dataset_id: string;
  batch_size?: number;
  limit?: number;
}

export interface WorkflowDataSourceQueryInput {
  type: 'data_source_query';
  source_id: string;
  query: string;
  parameters?: Record<string, any>;
  batch_size?: number;
  limit?: number;
  timeout_secs?: number;
}

export interface WorkflowEntityFilterInput {
  type: 'entity_filter';
  entity_type: string;
  graph?: string;
  created_after?: string;
  updated_after?: string;
  limit?: number;
  batch_size?: number;
}

export interface WorkflowSparqlQueryInput {
  type: 'sparql_query';
  query: string;
  graph?: string;
  batch_size?: number;
  limit?: number;
}

export type WorkflowGraphInput =
  | WorkflowJsonInput
  | WorkflowDatasetInput
  | WorkflowDataSourceQueryInput
  | WorkflowEntityFilterInput
  | WorkflowSparqlQueryInput;

export interface WorkflowOutputDatasetRequest {
  name?: string;
}

export interface WorkflowOutputDatasetRef {
  dataset_id: string;
  name: string;
  dataset_type: string;
  asset_kind: string;
  record_count: number;
  file_size_bytes: number;
  created_at: string;
}

export interface ExecuteWorkflowRequest {
  input: WorkflowGraphInput | Record<string, any>;
  context?: WorkflowExecutionContext;
  output_dataset?: WorkflowOutputDatasetRequest;
}

export interface StepResult {
  step_id: string;
  success: boolean;
  confidence: number;
  duration_ms: number;
  output?: any;
  error?: string;
}

export interface WorkflowExecutionResult {
  execution_id: string;
  workflow_id: string;
  success: boolean;
  confidence: number;
  started_at: string;
  completed_at: string;
  duration_ms: number;
  step_results: StepResult[];
  final_output?: any;
  batch_count?: number;
  results?: Array<{
    execution_id: string;
    success: boolean;
    step_results: StepResult[];
    final_output: any;
    confidence: number;
    materialized_dataset?: WorkflowOutputDatasetRef;
  }>;
  materialized_dataset?: WorkflowOutputDatasetRef;
}

export interface WorkflowExecutionSummary {
  execution_id: string;
  workflow_id: string;
  success: boolean;
  confidence: number;
  started_at: string;
  completed_at: string;
  duration_ms: number;
}

// ============================================================================
// Workflow Testing & Validation Types (New API v0.2.0)
// ============================================================================

export type ValidateWorkflowRequest = WorkflowDefinition;

export interface WorkflowValidationIssue {
  level: 'warning' | 'error';
  step_id: string;
  code: string;
  message: string;
  field?: string;
}

export interface ValidateWorkflowResponse {
  valid: boolean;
  message: string;
  warnings?: string[];
  step_count?: number;
  has_conditional_logic?: boolean;
  has_error_handling?: boolean;
  issues?: WorkflowValidationIssue[];
}

export interface TestStepRequest {
  step: WorkflowStep;
  input: Record<string, any>;
  context?: Record<string, any>;
}

export interface TestStepResponse {
  success: boolean;
  output: any;
  error: string | null;
  execution_time_ms: number;
  step_type: string;
}

export interface DryRunRequest {
  input: JsonValue;
  context?: Record<string, any>;
}

export interface DryRunStepResult {
  step_id: string;
  step_type: string;
  success: boolean;
  output: any;
  error: string | null;
  execution_time_ms: number;
}

export interface DryRunResponse {
  success: boolean;
  steps_executed: DryRunStepResult[];
  final_output: any;
  total_execution_time_ms: number;
  failed_step: string | null;
}

// ============================================================================
// Workflow Scheduling Types (New API v0.2.0+)
// ============================================================================
// NOTE: Multiple schedules per workflow are supported
// API Endpoints:
//   POST   /api/v1/workflows/{workflow_id}/schedules - Create new schedule
//   GET    /api/v1/workflows/{workflow_id}/schedules - List all schedules
//   GET    /api/v1/workflows/{workflow_id}/schedules/{schedule_id} - Get specific schedule
//   PUT    /api/v1/workflows/{workflow_id}/schedules/{schedule_id} - Update schedule
//   DELETE /api/v1/workflows/{workflow_id}/schedules/{schedule_id} - Delete schedule

/**
 * Request to create a new schedule for a workflow
 */
export interface ScheduleWorkflowRequest {
  cron_expression?: string;
  interval_seconds?: number;
  scheduled_at?: string; // ISO 8601 timestamp
  timezone?: string; // IANA timezone (e.g., "America/New_York", "UTC") - requires backend v0.3.0+
  input: JsonValue;
  context?: Record<string, any>;
  enabled: boolean;
}

/**
 * Request to update an existing schedule
 */
export interface UpdateScheduleRequest {
  cron_expression?: string;
  interval_seconds?: number;
  scheduled_at?: string;
  timezone?: string;
  input?: JsonValue;
  context?: Record<string, any>;
  enabled?: boolean;
}

/**
 * Workflow schedule information
 * A workflow can have multiple independent schedules
 */
export interface WorkflowSchedule {
  schedule_id: string;
  workflow_id: string;
  cron_expression: string | null;
  interval_seconds: number | null;
  scheduled_at: string | null;
  timezone: string | null; // IANA timezone - requires backend v0.3.0+
  next_execution: string | null;
  last_execution: string | null;
  enabled: boolean;
  created_at: string;
  execution_count: number;
}

/**
 * Response when creating a new schedule
 */
export interface ScheduleWorkflowResponse {
  workflow_id: string;
  schedule_id: string;
  cron_expression?: string;
  interval_seconds?: number;
  scheduled_at?: string;
  timezone?: string;
  next_execution: string;
  enabled: boolean;
  created_at: string;
}

// ============================================================================
// Workflow Orchestration Types (Legacy - deprecated, use above)
// ============================================================================

export type LegacyWorkflowStepType = 'ml_prediction' | 'heuristic' | 'confidence_gate' | 'fallback' | 'filter' | 'validate';

export interface LegacyWorkflowStep {
  id: string;
  step_type: LegacyWorkflowStepType;
  config: Record<string, any>;
  depends_on: string[];
}

export interface LegacyWorkflowDefinition {
  id: string;
  name: string;
  description?: string;
  definition: {
    steps: LegacyWorkflowStep[];
    fusion_threshold: number;
    fallback: string;
  };
  tags?: string[];
  created_at?: string;
  updated_at?: string;
}

export interface LegacyRegisterWorkflowRequest {
  id: string;
  name: string;
  description?: string;
  definition: LegacyWorkflowDefinition['definition'];
  tags?: string[];
}

export interface WorkflowExecutionRequest {
  input: WorkflowGraphInput | Record<string, any>;
  context?: WorkflowExecutionContext;
  output_dataset?: WorkflowOutputDatasetRequest;
}

export interface LegacyStepResult {
  step_id: string;
  success: boolean;
  output: Record<string, any>;
  confidence: number;
  duration_ms: number;
  error?: string;
}

export interface WorkflowExecutionResult {
  execution_id: string;
  workflow_id: string;
  success: boolean;
  confidence: number;
  started_at: string;
  completed_at: string;
  duration_ms: number;
  step_results: StepResult[];
  final_output?: any;
  batch_count?: number;
  results?: Array<{
    execution_id: string;
    success: boolean;
    step_results: StepResult[];
    final_output: any;
    confidence: number;
    materialized_dataset?: WorkflowOutputDatasetRef;
  }>;
  materialized_dataset?: WorkflowOutputDatasetRef;
}

export interface WorkflowExecutionSummary {
  execution_id: string;
  workflow_id: string;
  success: boolean;
  confidence: number;
  started_at: string;
  completed_at: string;
  duration_ms: number;
}

// ============================================================================
// Rule Management Types
// ============================================================================

export interface LoadRuleRequest {
  wasm_bytes: string; // Base64-encoded WASM binary
}

export interface LoadRuleResponse {
  rule_id: string;
  status: 'loaded' | 'error';
  error?: string;
}

export interface ExecuteRuleRequest {
  input: Record<string, any>;
}

export interface ExecuteRuleResponse {
  success: boolean;
  output: Record<string, any>;
  confidence: number;
  duration_ms?: number;
}

// ============================================================================
// Monitoring & Cache Types
// ============================================================================

export interface CacheStatsResponse {
  size: number;
  capacity: number;
  utilization: number;
  hit_rate?: number;
  miss_rate?: number;
}

// Type alias for consistency
export type CacheStats = CacheStatsResponse;

export interface CircuitBreakerStatus {
  model_id: string;
  state: 'closed' | 'open' | 'half_open';
  failure_count: number;
  success_count: number;
  last_failure?: string;
  next_retry?: string;
}

// ============================================================================
// Admin/Temporal Types
// ============================================================================

export interface TemporalStats {
  total_versions: number;
  active_versions: number;
  temporal_chains: number;
  avg_chain_length: number;
  oldest_version: string;
  newest_version: string;
}

export interface TemporalSummary {
  stats: TemporalStats;
  index_stats: {
    temporal_index_size: number;
    wal_size: number;
    checkpoint_count: number;
  };
  recent_operations: Array<{
    operation: string;
    timestamp: string;
    record_count: number;
  }>;
}

export interface WalStatus {
  enabled: boolean;
  current_sequence: number;
  last_checkpoint: string;
  pending_operations: number;
  file_size_bytes: number;
}

export interface WalOperation {
  sequence: number;
  operation_type: string;
  timestamp: string;
  record_id?: string;
  status: 'pending' | 'committed' | 'failed';
}

// ============================================================================
// Audit Types
// ============================================================================

export type AuditEventType =
  | 'login_success'
  | 'login_failure'
  | 'logout'
  | 'token_generated'
  | 'token_refreshed'
  | 'token_revoked'
  | 'user_created'
  | 'user_updated'
  | 'user_deleted'
  | 'password_changed'
  | 'account_locked'
  | 'account_unlocked'
  | 'setup_token_generated'
  | 'setup_token_used'
  | 'setup_token_expired'
  | 'admin_created'
  | 'access_granted'
  | 'access_denied'
  | 'permission_changed'
  | 'role_changed'
  | 'data_read'
  | 'data_written'
  | 'data_deleted'
  | 'query_executed'
  | 'configuration_changed'
  | 'system_started'
  | 'system_shutdown'
  | 'backup_created'
  | 'backup_restored'
  | 'rate_limit_exceeded'
  | 'suspicious_activity'
  | 'security_violation'
  | 'encryption_key_rotated';

export type AuditResult = 'success' | 'failure' | 'partial_success' | 'denied';

export interface AuditLogEntry {
  id: string;
  timestamp: string;
  event_type: AuditEventType;
  user_id?: string;
  username?: string;
  user_role?: string;
  ip_address?: string;
  user_agent?: string;
  resource?: string;
  action: string;
  result: AuditResult;
  metadata: Record<string, any>;
  session_id?: string;
}

export interface AuditQueryRequest {
  user_id?: string;
  username?: string;
  event_types?: string[];
  start_time?: string;
  end_time?: string;
  limit?: number;
}

export interface AuditQueryResponse {
  events: AuditLogEntry[];
  count: number;
  query_time: string;
}

export interface AuditExportRequest {
  format: 'json' | 'csv';
  start_date: string;
  end_date: string;
  filters?: Omit<AuditQueryRequest, 'limit' | 'start_time' | 'end_time'>;
}

// ============================================================================
// Health Check Types
// ============================================================================

export interface HealthResponse {
  status: 'alive' | 'healthy' | 'unhealthy' | 'degraded';
  version: string;
  timestamp: string;
  components?: Record<string, ComponentHealth> | null;
}

export interface ComponentHealth {
  status: 'healthy' | 'unhealthy';
  message?: string;
  latency_ms?: number;
}

// ============================================================================
// Cluster & Sharding Management Types
// ============================================================================

export interface HashRangeResponse {
  start: number;
  end: number;
}

export interface ShardResponse {
  shard_id: number;
  leader_address: string;
  replica_addresses: string[];
  status: string;
  hash_range: HashRangeResponse;
  triple_count: number;
  size_bytes: number;
  last_heartbeat: string;
  raft_term: number;
}

export interface TopologyResponse {
  total_shards: number;
  replication_factor: number;
  cluster_version: number;
  total_triples: number;
  total_size_bytes: number;
  updated_at: string;
  shards: ShardResponse[];
}

export interface ClusterStatsResponse {
  total_shards: number;
  healthy_shards: number;
  degraded_shards: number;
  down_shards: number;
  total_triples: number;
  total_size_gb: number;
  queries_per_second: number;
  writes_per_second: number;
  p99_query_latency_ms: number;
  p99_write_latency_ms: number;
  average_shard_utilization: number;
  timestamp: string;
}

export interface HealthIssue {
  severity: 'warning' | 'error' | 'critical';
  component: string;
  message: string;
  detected_at: string;
}

export interface ClusterHealthResponse {
  status: 'healthy' | 'degraded' | 'critical';
  total_shards: number;
  healthy_shards: number;
  degraded_shards: number;
  down_shards: number;
  issues: HealthIssue[];
  last_check: string;
  uptime_seconds: number;
}

export interface AutoScalingConfig {
  enabled: boolean;
  min_shards: number;
  max_shards: number;
  target_utilization: number;
  scale_out_threshold: number;
  scale_in_threshold: number;
  cooldown_minutes: number;
}

export interface DataRetentionConfig {
  auto_save_interval_seconds: number;
  backup_enabled: boolean;
  backup_interval_hours: number;
  retention_days: number;
}

export interface PerformanceConfig {
  query_timeout_seconds: number;
  write_timeout_seconds: number;
  max_query_result_size: number;
  connection_pool_size: number;
}

export interface ClusterConfigResponse {
  cluster_name: string;
  mode: 'single-node' | 'distributed';
  auto_scaling: AutoScalingConfig;
  data_retention: DataRetentionConfig;
  performance: PerformanceConfig;
}

export interface ScaleOutRequest {
  new_shard_count: number;
  replication_factor: number;
  node_addresses: string[];
  rebalance_strategy: 'gradual' | 'immediate';
  rebalance_throttle_mbps?: number;
}

export interface ScaleOutResponse {
  operation_id: string;
  status: string;
  old_shard_count: number;
  new_shard_count: number;
  message: string;
  estimated_duration_minutes?: number;
  started_at: string;
}

export interface ShardNodeInfo {
  address: string;
  raft_term: number;
  is_healthy: boolean;
  last_heartbeat: string;
}

export interface ShardReplicaInfo {
  address: string;
  role: string;
  lag_bytes: number;
  is_healthy: boolean;
}

export interface ShardStatistics {
  triple_count: number;
  size_bytes: number;
  queries_per_second: number;
  writes_per_second: number;
  p99_query_latency_ms: number;
}

export interface ShardDetailResponse {
  shard_id: number;
  status: string;
  leader: ShardNodeInfo;
  replicas: ShardReplicaInfo[];
  hash_range: HashRangeResponse;
  statistics: ShardStatistics;
}

export interface ReplicationConfigResponse {
  replication_factor: number;
  sync_replication: boolean;
  async_replication_lag_ms: number;
  raft_election_timeout_ms: number;
  raft_heartbeat_interval_ms: number;
  enable_auto_failover: boolean;
  failover_timeout_seconds: number;
}

export interface ClusterMetadataResponse {
  cluster_id: string;
  created_at: string;
  cluster_version: number;
  graphica_version: string;
  mode: string;
  total_operations: number;
  last_topology_change?: string;
}

// ============================================================================
// SPARQL Query Types
// ============================================================================

export interface SparqlTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  sparql: string;
  parameters: SparqlTemplateParameter[];
  exampleResults?: string;
}

export interface SparqlTemplateParameter {
  name: string;
  label: string;
  type: 'entity_id' | 'model_id' | 'date' | 'number' | 'threshold' | 'text';
  required: boolean;
  defaultValue?: any;
  helpText?: string;
  placeholder?: string;
}

export interface SavedSparqlQuery {
  id: string;
  name: string;
  description: string;
  query: string;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface SparqlQueryHistoryItem {
  id: string;
  query: string;
  timestamp: string;
  results_count: number;
  execution_time_ms: number;
  success: boolean;
  error?: string;
}

// ============================================================================
// Datasource Types
// ============================================================================

export type DatasourceType =
  | 'Relational'
  | 'Document'
  | 'Search'
  | 'ObjectStorage'
  | 'Streaming'
  | 'Graph'
  | 'TimeSeries'
  | { Custom: string };

export type ConnectionStatus =
  | 'Disconnected'
  | 'Connecting'
  | 'Unverified'
  | 'Connected'
  | { Degraded: string }
  | { Error: string };

export type HealthStatus =
  | 'Healthy'
  | { Degraded: string }
  | 'Unhealthy';

export interface PluginMetadata {
  name: string;
  version: string;
  author: string;
  description: string;
  datasource_type: DatasourceType;
}

export interface PluginCapabilities {
  cdc: boolean;
  batch_read: boolean;
  batch_write: boolean;
  profiling: boolean;
  lineage_discovery: boolean;
  schema_evolution: boolean;
  transactions: boolean;
}

/**
 * Connection details for a datasource
 * Credentials are stored in a secret vault and referenced via secretRef
 */
export interface ConnectionDetails {
  /** Reference to credentials in secret vault (e.g., "vault://credentials/my-postgres") */
  secretRef: string;
  /** Source-specific configuration (includes type field) */
  config: {
    type: string; // e.g., "PostgreSQL", "Oracle", etc.
    [key: string]: any; // Additional source-specific config (host, port, database, etc.)
  };
  /** Whether encryption/SSL is enabled for this connection */
  encryptionEnabled: boolean;
}

/**
 * @deprecated - Use ConnectionDetails instead
 * This structure is no longer used by the backend API
 */
export interface DatasourceConfig {
  connection: Record<string, any>;
  cdc?: Record<string, any>;
  profiling?: Record<string, any>;
  lineage?: Record<string, any>;
  custom?: Record<string, any>;
}

export interface DatasourceInstanceCapabilities {
  canTest: boolean;
  canInferSchema: boolean;
  canQuery: boolean;
  canReadWorkflow: boolean;
  canWriteWorkflow: boolean;
  supportsParameters: boolean;
  supportsTls: boolean;
  supportsIncremental: boolean;
  supportsCancellation: boolean;
}

export interface Datasource {
  id: string;
  name: string;
  plugin_name: string;
  source_type?: string;
  version?: string;
  enabled: boolean;
  description?: string;
  tags?: string[];
  metadata: PluginMetadata;
  capabilities: PluginCapabilities;
  instance_capabilities?: DatasourceInstanceCapabilities;
  status: ConnectionStatus;
  config: DatasourceConfig;
  created_at: string;
  updated_at: string;

  // File-based datasource integration fields
  file_id?: string;
  file_based?: boolean;
}

/**
 * Request to register a new datasource
 * Backend API format
 */
export interface RegisterDatasourceRequest {
  /** Display title for the datasource */
  title: string;
  /** Type of datasource (e.g., "PostgreSQL", "Oracle", "Snowflake") */
  sourceType: string;
  /** Connection details with vault reference */
  connection: ConnectionDetails;
  /** Optional description */
  description?: string;
  /** Optional metadata */
  metadata?: Record<string, any>;
}

export interface UpdateDatasourceRequest {
  title?: string;
  description?: string;
  sourceType?: string;
  connection?: ConnectionDetails;
  schemaRef?: string;
  tags?: string[];
  metadata?: Record<string, string>;
}

/**
 * @deprecated This interface is deprecated for security reasons.
 * The inline connection testing feature (passing credentials without registration)
 * was intentionally not implemented to avoid security risks of credentials in API requests.
 *
 * Correct workflow:
 * 1. Register datasource first (POST /api/v1/datasources)
 * 2. Test using sourceId (POST /api/v1/datasources/test with { sourceId })
 */
export interface TestConnectionRequest {
  config: DatasourceConfig;
}

export interface TestConnectionResponse {
  success: boolean;
  message: string;
  latency_ms: number;
  metadata?: Record<string, any>;
}

export interface DatasourceHealthResponse {
  status: HealthStatus;
  last_check: string;
  metrics: Record<string, number>;
  issues?: string[];
}

export interface SchemaInfo {
  schemas: SchemaDefinition[];
  total_tables: number;
  total_columns: number;
}

export interface SchemaDefinition {
  name: string;
  tables: TableDefinition[];
}

export interface TableDefinition {
  schema: string;
  name: string;
  columns: ColumnDefinition[];
  row_count?: number;
  size_bytes?: number;
  primary_keys?: string[];
  foreign_keys?: ForeignKeyDefinition[];
}

export interface ColumnDefinition {
  name: string;
  data_type: string;
  nullable: boolean;
  default_value?: string;
  is_primary_key?: boolean;
}

export interface ForeignKeyDefinition {
  column: string;
  referenced_schema: string;
  referenced_table: string;
  referenced_column: string;
}

export interface AvailablePlugin {
  name: string;
  source_type?: string;
  version: string;
  description: string;
  datasource_type: DatasourceType;
  capabilities: PluginCapabilities;
  config_schema: Record<string, any>;
}

export interface DatasourceStats {
  total_datasources: number;
  connected: number;
  disconnected: number;
  errors: number;
  by_type: Record<string, number>;
}

// ============================================================================
// Entity Fusion Types (New API)
// ============================================================================

export type FusionCandidateStatus = 'proposed' | 'approved' | 'rejected' | 'committed';

export interface FusionCandidate {
  candidate_id: string;
  entities: Array<{
    id: string;
    [key: string]: any;
  }>;
  match_rule: string;
  match_value: string;
  confidence: number;
  proposed_at: string;
  status: FusionCandidateStatus;
  reviewed_by?: string;
  reviewed_at?: string;
  review_notes?: string;
}

export interface ProposeFusionRequest {
  dataset: string;
  rule: string;
  min_confidence?: number;
}

export interface ProposeFusionResponse {
  candidates: FusionCandidate[];
  total_count: number;
}

export interface FusionCandidateQuery {
  status?: FusionCandidateStatus;
  limit?: number;
}

export interface FusionCandidateListResponse {
  candidates: FusionCandidate[];
  total_count: number;
}

export interface ReviewCandidateRequest {
  reviewer: string;
  notes?: string;
}

export interface ReviewCandidateResponse {
  candidate_id: string;
  status: FusionCandidateStatus;
  reviewed_by: string;
  reviewed_at: string;
}

export interface FusionResolveRequest {
  entities: Array<{ id: string; [key: string]: any }>;
  rule: string;
  confidence?: number;
}

export interface FusionResolveResponse {
  fusion_id: string;
  merged_entity_id: string;
  source_entity_ids: string[];
  rule: string;
  confidence: number;
  created_at: string;
}

export interface ReverseFusionRequest {
  reason?: string;
}

export interface ReverseFusionResponse {
  fusion_id: string;
  reversed: boolean;
  reversed_at: string;
  reason: string;
}

// ============================================================================
// File Library Types
// ============================================================================

// ============================================================================
// File Library Schema Types (matches backend DataFile)
// ============================================================================

export type FieldType = 'STRING' | 'INTEGER' | 'FLOAT' | 'BOOLEAN' | 'TIMESTAMP' | 'DATE';
export type PiiType = 'email' | 'phone' | 'ssn' | 'credit_card' | 'custom';

export interface SchemaField {
  name: string;
  type: FieldType;
  nullable: boolean;
  sample_values: string[];
  is_pii?: boolean;
  pii_type?: PiiType;
}

export interface FileSchema {
  fields: SchemaField[];
  total_rows: number;
  estimated_rows?: number;
  last_scanned: string; // ISO 8601 datetime
}

/**
 * Ontology mapping for a field
 * Maps a data field to an ontology concept
 */
export interface FieldOntologyMapping {
  field_name: string;
  ontology_id: string;
  concept_uri: string;
  concept_label: string;
  similarity: number;
  confidence: number;
  method: string; // e.g., "ExactMatch", "SemanticSimilarity"
  mapped_at: string; // ISO 8601 datetime
}

export interface FileMetadata {
  file_id: string;
  filename: string;
  original_filename: string;
  mime_type: string;
  size_bytes: number;
  checksum_sha256: string;
  uploaded_at: string;
  uploaded_by: string;
  folder_id?: string;
  tags: string[];
  custom_metadata?: Record<string, any>;
  access_count: number;
  last_accessed_at?: string;

  // Schema (matches backend DataFile.schema)
  schema?: FileSchema;

  // Ontology mappings (matches backend DataFile.ontology_mappings)
  ontology_mappings?: FieldOntologyMapping[];

  // Datasource integration fields
  datasource_id?: string;
  registration_status?: 'unregistered' | 'registered' | 'error';
  registered_at?: string;

  // DEPRECATED: Use 'schema' instead
  // Kept for backward compatibility with existing code
  inferred_schema?: {
    row_count: number;
    column_count: number;
    columns: Array<{
      name: string;
      type: string;
      nullable: boolean;
    }>;
  };
}

export interface FolderMetadata {
  folder_id: string;
  name: string;
  parent_folder_id: string | null; // null for root folders, string for subfolders
  created_at: string;
  file_count: number;
  total_size_bytes: number;
  default_ontology_id?: string; // Default ontology for files in this folder
}

export interface FileUploadRequest {
  file: File;
  folder_id?: string;
  tags?: string[];
  custom_metadata?: Record<string, any>;
  auto_profile?: boolean; // Phase 2.1: Auto-profile files on upload
}

export interface FileUploadResponse {
  file_id: string;
  filename: string;
  size_bytes: number;
  upload_url?: string; // For presigned uploads
}

export interface BulkImportRequest {
  files: FileUploadRequest[];
  folder_id?: string;
  common_tags?: string[];
}

export interface BulkImportResponse {
  job_id: string;
  total_files: number;
  status: 'pending' | 'processing' | 'completed' | 'failed';
}

export interface FileListParams {
  folder_id?: string;
  tags?: string[];
  mime_type?: string;
  search?: string;
  page?: number;
  page_size?: number;
  limit?: number; // Alias for page_size
  sort_by?: 'uploaded_at' | 'filename' | 'size_bytes' | 'access_count';
  sort_order?: 'asc' | 'desc';
}

export interface FileListResponse {
  files: FileMetadata[];
  total: number;
  page: number;
  page_size: number;
  total_pages?: number;
}

export interface FolderCreateRequest {
  name: string;
  parent_folder_id?: string;
  default_ontology_id?: string; // Default ontology for files in this folder
}

export interface TagStatistics {
  tag: string;
  file_count: number;
}

export interface FileLibraryStats {
  total_files: number;
  total_size_bytes: number;
  folder_count: number;
  total_tags: number;
  recent_uploads: number; // Last 24h
}

// File-to-Datasource Integration Types
export interface RegisterFileAsDatasourceRequest {
  datasource_name: string;
  connector_type: 'CSVFile' | 'ExcelFile' | 'TSVFile';
  parsing_config: {
    delimiter?: string;
    has_header?: boolean;
    encoding?: string;
    sheet_name?: string; // For Excel
  };
  import_to_catalogue?: boolean;
}

export interface RegisterFileAsDatasourceResponse {
  datasource_id: string;
  file_id: string;
  status: 'active' | 'error';
  schema?: SchemaInfo;
  dataset_id?: string; // If import_to_catalogue = true
}

export interface ValidateFileForRegistrationResponse {
  can_register: boolean;
  issues: string[];
  inferred_config: {
    connector_type: string;
    delimiter?: string;
    has_header?: boolean;
    row_count?: number;
    column_count?: number;
  };
}
