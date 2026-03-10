// API response types and utilities

export interface PaginatedResponse<T> {
  data: T[];
  pagination: {
    page: number;
    limit: number;
    total: number;
    total_pages: number;
  };
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: ApiError;
}

export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, any>;
  status?: number;
}

export interface HealthStatus {
  status: 'alive' | 'ready' | 'degraded';
  version: string;
  timestamp: string;
  components?: Record<string, ComponentHealth>;
}

export interface ComponentHealth {
  status: string;
  message?: string;
}

export interface SparqlQuery {
  query: string;
  timeout?: number;
}

export interface SparqlResult {
  head: {
    vars: string[];
  };
  results: {
    bindings: Array<Record<string, { type: string; value: string }>>;
  };
}

export interface FusionOperation {
  fusion_id: string;
  merged_entity_id: string;
  source_entity_ids: string[];
  rule_id: string;
  method: 'manual' | 'automatic' | 'ml_based';
  confidence: number;
  timestamp: string;
  reversed_at?: string;
  user?: string;
}

export interface QualityScorecard {
  dataset: string;
  overall_score: number;
  period_start: string;
  period_end: string;
  dimension_scores: {
    completeness?: number;
    accuracy?: number;
    consistency?: number;
    timeliness?: number;
    uniqueness?: number;
    validity?: number;
  };
  violations?: QualityViolation[];
}

export interface QualityViolation {
  rule_id: string;
  rule_name: string;
  severity: 'critical' | 'major' | 'minor' | 'warning';
  affected_records: number;
  sample_records?: string[];
  detected_at: string;
}

export interface LogEntry {
  timestamp: string;
  level: 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | 'TRACE';
  component: string;
  message: string;
  metadata?: Record<string, any>;
}

export interface WebSocketMessage {
  type: 'log' | 'entity_update' | 'model_prediction' | 'fusion_event';
  payload: any;
  timestamp: string;
}