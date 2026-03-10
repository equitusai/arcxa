/**
 * Schema discovery types aligned with the coordinator API.
 */

// ===== Connection Configuration Types =====

export type DatabaseType = 'Oracle' | 'DB2' | 'PostgreSQL' | 'CSV';

export interface BaseConnectionConfig {
  host?: string;
  port?: number;
  username?: string;
  password?: string;
}

export interface OracleConnectionConfig extends BaseConnectionConfig {
  serviceName?: string;
  sid?: string;
  schema?: string;
}

export interface DB2ConnectionConfig extends BaseConnectionConfig {
  database: string;
  schema?: string;
}

export interface PostgreSQLConnectionConfig extends BaseConnectionConfig {
  database: string;
  schema?: string;
  sslMode?: 'disable' | 'prefer' | 'require' | 'verify-ca' | 'verify-full';
}

export interface CSVConnectionConfig {
  file?: File;
  has_header: boolean;
  delimiter: string;
  encoding?: string;
}

export type ConnectionConfig =
  | OracleConnectionConfig
  | DB2ConnectionConfig
  | PostgreSQLConnectionConfig
  | CSVConnectionConfig;

// ===== Discovery Request Types =====

export interface DiscoveryOptions {
  schema_filter?: string;
  table_filter?: string;
  sample_size: number;
  cache_ttl_secs: number;
}

export interface DiscoveryRequest {
  datasource_id: string;
  options: DiscoveryOptions;
}

// ===== Discovery Progress Types =====

export type DiscoveryStatus =
  | 'queued'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface DiscoveryProgress {
  discovery_id: string;
  datasource_id: string;
  status: DiscoveryStatus;
  current_step: string;
  tables_discovered: number;
  total_tables?: number;
  percent_complete: number;
  errors: string[];
  started_at: string;
  updated_at: string;
  completed_at?: string | null;
}

// ===== Discovery Result Types =====

export interface DetectedPattern {
  pattern_type: string;
  match_rate: number;
  example?: string | null;
}

export interface ColumnStatistics {
  distinct_count: number;
  null_fraction: number;
  sample_count: number;
  most_common_values?: string[] | null;
  avg_length?: number | null;
  min_value?: string | null;
  max_value?: string | null;
}

export interface DiscoveredColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  primary_key: boolean;
  semantic_type?: string | null;
  confidence: number;
  patterns: DetectedPattern[];
  statistics: ColumnStatistics;
  sample_values: string[];
}

export interface DiscoveredTable {
  name: string;
  columns: DiscoveredColumn[];
  row_count?: number | null;
}

export interface DiscoveryResult {
  discovery_id: string;
  tables: DiscoveredTable[];
  total: number;
  page: number;
  page_size: number;
  cached_at: string;
}

// ===== Wizard State Types =====

export interface DiscoveryWizardState {
  currentStep: number;
  discoveryOptions: DiscoveryOptions;
  testConnectionStatus: 'idle' | 'testing' | 'success' | 'error';
  testConnectionError?: string;
  discoveryId?: string;
  discoveryProgress?: DiscoveryProgress;
  discoveryResult?: DiscoveryResult;
}

// ===== Form Validation Types =====

export interface ValidationError {
  field: string;
  message: string;
}

export interface ConnectionTestResult {
  success: boolean;
  message?: string;
  error?: string;
}
