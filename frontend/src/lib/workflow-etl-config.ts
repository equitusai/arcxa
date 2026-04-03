/**
 * ETL Workflow Step Type Configuration
 * Visual design and metadata for ETL nodes with sophisticated UI
 */

import {
  FileText,
  Database,
  Layers,
  Wand2,
  Merge,
  Sigma,
  ShieldCheck,
  Copy,
  Save,
  Upload,
  FileDown,
  Clock,
  FolderInput,
} from 'lucide-react';

// ETL Step Types (extends existing StepType)
export type ETLStepType =
  // Extract
  | 'csv_source'
  | 'db_extract'
  | 'multi_source_input'
  // Transform
  | 'semantic_mapper'
  | 'field_transformer'
  | 'data_joiner'
  | 'aggregator'
  // Quality
  | 'data_validator'
  | 'deduplicator'
  // Load
  | 'rdf_loader'
  | 'db_loader'
  | 'csv_exporter'
  // Orchestration
  | 'scheduler';

export type ETLCategory = 'extract' | 'transform' | 'quality' | 'load' | 'orchestration';

export interface ETLStepTypeConfig {
  id: ETLStepType;
  label: string;
  icon: any;
  color: {
    base: string;
    subtle: string;
    border: string;
    text: string;
  };
  description: string;
  category: ETLCategory;
  inputs?: string[]; // Port labels for inputs
  outputs?: string[]; // Port labels for outputs
  collapsedByDefault?: boolean;
}

// ETL color palette (Oracle Redwood + Fluent inspired)
export const ETL_COLORS = {
  extract: {
    base: '#0078D4',
    subtle: 'rgba(0, 120, 212, 0.1)',
    border: 'rgb(0, 120, 212)',
    text: 'rgb(0, 120, 212)',
  },
  transform: {
    base: '#00CC6A',
    subtle: 'rgba(0, 204, 106, 0.1)',
    border: 'rgb(0, 204, 106)',
    text: 'rgb(0, 204, 106)',
  },
  quality: {
    base: '#E74856',
    subtle: 'rgba(231, 72, 86, 0.1)',
    border: 'rgb(231, 72, 86)',
    text: 'rgb(231, 72, 86)',
  },
  load: {
    base: '#8764B8',
    subtle: 'rgba(135, 100, 184, 0.1)',
    border: 'rgb(135, 100, 184)',
    text: 'rgb(135, 100, 184)',
  },
  orchestration: {
    base: '#FF8C00',
    subtle: 'rgba(255, 140, 0, 0.1)',
    border: 'rgb(255, 140, 0)',
    text: 'rgb(255, 140, 0)',
  },
};

export const ETL_STEP_TYPE_CONFIGS: Record<ETLStepType, ETLStepTypeConfig> = {
  // ============================================================================
  // EXTRACT NODES
  // ============================================================================

  csv_source: {
    id: 'csv_source',
    label: 'CSV Source',
    icon: FileText,
    color: ETL_COLORS.extract,
    description: 'Import CSV files with auto-detection',
    category: 'extract',
    outputs: ['data'],
  },

  db_extract: {
    id: 'db_extract',
    label: 'Database Extract',
    icon: Database,
    color: ETL_COLORS.extract,
    description: 'Pull data from connected datasources',
    category: 'extract',
    outputs: ['data'],
  },

  multi_source_input: {
    id: 'multi_source_input',
    label: 'Multi-Source Input',
    icon: FolderInput,
    color: ETL_COLORS.extract,
    description: 'Select and join multiple sources from Data Catalogue',
    category: 'extract',
    outputs: ['merged_data'],
  },

  // ============================================================================
  // TRANSFORM NODES
  // ============================================================================

  semantic_mapper: {
    id: 'semantic_mapper',
    label: 'Semantic Mapper',
    icon: Layers,
    color: ETL_COLORS.transform,
    description: 'AI-powered field → ontology mapping',
    category: 'transform',
    inputs: ['data'],
    outputs: ['mapped'],
  },

  field_transformer: {
    id: 'field_transformer',
    label: 'Field Transformer',
    icon: Wand2,
    color: ETL_COLORS.transform,
    description: 'TRIM, LOWER, REGEX, CONCAT, etc.',
    category: 'transform',
    inputs: ['data'],
    outputs: ['transformed'],
  },

  data_joiner: {
    id: 'data_joiner',
    label: 'Data Joiner',
    icon: Merge,
    color: ETL_COLORS.transform,
    description: 'JOIN multiple sources',
    category: 'transform',
    inputs: ['left', 'right'],
    outputs: ['joined'],
  },

  aggregator: {
    id: 'aggregator',
    label: 'Aggregator',
    icon: Sigma,
    color: ETL_COLORS.transform,
    description: 'GROUP BY with SUM, AVG, COUNT',
    category: 'transform',
    inputs: ['data'],
    outputs: ['aggregated'],
  },

  // ============================================================================
  // QUALITY NODES
  // ============================================================================

  data_validator: {
    id: 'data_validator',
    label: 'Data Validator',
    icon: ShieldCheck,
    color: ETL_COLORS.quality,
    description: 'Enforce NOT NULL, REGEX, RANGE rules',
    category: 'quality',
    inputs: ['data'],
    outputs: ['valid', 'invalid'],
  },

  deduplicator: {
    id: 'deduplicator',
    label: 'Deduplicator',
    icon: Copy,
    color: ETL_COLORS.quality,
    description: 'Remove duplicates (exact/fuzzy/semantic)',
    category: 'quality',
    inputs: ['data'],
    outputs: ['unique'],
  },

  // ============================================================================
  // LOAD NODES
  // ============================================================================

  rdf_loader: {
    id: 'rdf_loader',
    label: 'RDF Loader',
    icon: Save,
    color: ETL_COLORS.load,
    description: 'Store entities in triple store with lineage',
    category: 'load',
    inputs: ['data'],
    outputs: ['loaded'],
  },

  db_loader: {
    id: 'db_loader',
    label: 'Database Loader',
    icon: Upload,
    color: ETL_COLORS.load,
    description: 'INSERT/UPSERT/MERGE to target database',
    category: 'load',
    inputs: ['data'],
  },

  csv_exporter: {
    id: 'csv_exporter',
    label: 'CSV Exporter',
    icon: FileDown,
    color: ETL_COLORS.load,
    description: 'Export results to CSV',
    category: 'load',
    inputs: ['data'],
  },

  // ============================================================================
  // ORCHESTRATION NODES
  // ============================================================================

  scheduler: {
    id: 'scheduler',
    label: 'Scheduler',
    icon: Clock,
    color: ETL_COLORS.orchestration,
    description: 'Cron-based recurring execution',
    category: 'orchestration',
    inputs: ['trigger'],
    outputs: ['scheduled'],
  },
};

// Helper functions
export function getETLStepTypeConfig(stepType: ETLStepType): ETLStepTypeConfig {
  return ETL_STEP_TYPE_CONFIGS[stepType];
}

export function getETLStepTypesByCategory(category: ETLCategory): ETLStepTypeConfig[] {
  return Object.values(ETL_STEP_TYPE_CONFIGS).filter(config => config.category === category);
}

export function getAllETLStepTypes(): ETLStepTypeConfig[] {
  return Object.values(ETL_STEP_TYPE_CONFIGS);
}

export function isETLStepType(stepType: string): stepType is ETLStepType {
  return stepType in ETL_STEP_TYPE_CONFIGS;
}

// Configuration schemas for each ETL node type
export interface CSVSourceConfig {
  file_id?: string;        // File Library file ID (primary)
  file_name?: string;      // Display name
  file_path: string;       // Legacy/fallback path
  delimiter?: string;
  has_header?: boolean;
  encoding?: string;
  skip_rows?: number;
  max_rows?: number;
  // Runtime state (populated after scanning)
  detected_fields?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
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
  last_scanned?: string; // ISO timestamp
}

// Multi-Source Input Configuration (Phase 2.1)
export interface WorkflowInputSource {
  sourceId: string;              // From Data Catalogue
  sourceName: string;            // Display name
  alias: string;                 // "customers", "orders", "products"
  isPrimary: boolean;            // Only one can be primary

  // Schema information from Data Catalogue
  schema?: Array<{
    name: string;
    type: string;
  }>;
  rowCount?: number;

  // Join configuration (for non-primary sources)
  join?: {
    type: 'LEFT' | 'INNER' | 'OUTER';
    localField: string;          // Field from primary source
    foreignField: string;        // Field from this source
    aggregations?: Array<{
      field: string;
      operation: 'COUNT' | 'SUM' | 'AVG' | 'MIN' | 'MAX';
      alias: string;
    }>;
  };
}

export interface MultiSourceInputConfig {
  sources: WorkflowInputSource[];
  // Merged schema preview (computed)
  mergedSchema?: Array<{
    name: string;
    type: string;
    sourceAlias: string;
  }>;
}

export interface DetectedField {
  name: string;
  type: string;
  sample_values?: string[];
  nullable?: boolean;
  primary_key?: boolean;
}

export interface DBExtractConfig {
  datasource_id: string;
  table_name?: string;
  schema_table?: string;
  query?: string;
  incremental?: boolean;
  incremental_column?: string;
  last_value?: any;
  batch_size?: number;
  columns?: string[];
  include_schema?: boolean;
  schema_sample_size?: number;
  detected_fields?: DetectedField[];
}

export interface SemanticMapperConfig {
  target_ontology: string[];
  auto_approve_threshold: number;
  mapping_mode: 'auto' | 'manual' | 'hybrid';
  mapping_session_id?: string;
  field_mappings?: Array<{
    source_field: string;
    ontology_term: string;
    confidence: number;
    transformation?: string;
  }>;
}

export interface FieldTransformation {
  field: string;
  operations: Array<{
    type: 'TRIM' | 'LOWER' | 'UPPER' | 'ROUND' | 'REGEX' | 'CONCAT' | 'SPLIT' | 'CUSTOM';
    params?: Record<string, any>;
  }>;
}

export interface FieldTransformerConfig {
  transformations: FieldTransformation[];
}

export interface DataJoinerConfig {
  join_type: 'inner' | 'left' | 'right' | 'full';
  left_key: string[];
  right_key: string[];
  output_columns?: string[];
}

export interface AggregatorConfig {
  group_by: string[];
  aggregations: Array<{
    field: string;
    function: 'SUM' | 'AVG' | 'COUNT' | 'MIN' | 'MAX' | 'STDDEV';
    alias?: string;
  }>;
}

export interface ValidationRule {
  field: string;
  rule_type: 'NOT_NULL' | 'REGEX' | 'RANGE' | 'IN_SET' | 'UNIQUE' | 'CUSTOM';
  params?: Record<string, any>;
  severity: 'error' | 'warning';
}

export interface DataValidatorConfig {
  rules: ValidationRule[];
  fail_on_error: boolean;
}

export interface DeduplicatorConfig {
  method: 'exact' | 'fuzzy' | 'semantic';
  key_fields: string[];
  threshold?: number; // for fuzzy/semantic matching
  keep: 'first' | 'last' | 'merge';
}

export interface RDFLoaderConfig {
  target_graph?: string;
  entity_type: string;
  id_field: string;
  batch_size: number;
  capture_lineage: boolean;
}

export interface DBLoaderConfig {
  datasource_id: string;
  table_name: string;
  mode: 'insert' | 'upsert' | 'replace';
  key_fields?: string[];
  batch_size: number;
}

export interface CSVExporterConfig {
  output_path: string;
  delimiter?: string;
  include_header?: boolean;
  encoding?: string;
}

export interface SchedulerConfig {
  cron_expression?: string;
  interval_seconds?: number;
  scheduled_at?: string;
  enabled: boolean;
}

// Union type for all ETL config types
export type ETLStepConfig =
  | { type: 'csv_source'; config: CSVSourceConfig }
  | { type: 'db_extract'; config: DBExtractConfig }
  | { type: 'multi_source_input'; config: MultiSourceInputConfig }
  | { type: 'semantic_mapper'; config: SemanticMapperConfig }
  | { type: 'field_transformer'; config: FieldTransformerConfig }
  | { type: 'data_joiner'; config: DataJoinerConfig }
  | { type: 'aggregator'; config: AggregatorConfig }
  | { type: 'data_validator'; config: DataValidatorConfig }
  | { type: 'deduplicator'; config: DeduplicatorConfig }
  | { type: 'rdf_loader'; config: RDFLoaderConfig }
  | { type: 'db_loader'; config: DBLoaderConfig }
  | { type: 'csv_exporter'; config: CSVExporterConfig }
  | { type: 'scheduler'; config: SchedulerConfig };
