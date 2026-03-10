// Lineage and provenance types (W3C PROV-based)

export interface LineageNode {
  id: string;
  type: 'entity' | 'model' | 'activity' | 'source' | 'dataset';
  label: string;
  metadata: Record<string, any>;
  timestamp?: string;
  position?: { x: number; y: number };
}

export interface LineageEdge {
  id?: string;
  source: string;
  target: string;
  relation: 'wasGeneratedBy' | 'used' | 'wasAssociatedWith' | 'wasDerivedFrom' | 'wasAttributedTo';
  label?: string;
  metadata?: Record<string, any>;
}

export interface LineageGraph {
  nodes: LineageNode[];
  edges: LineageEdge[];
  root_node?: string;
  depth?: number;
}

export interface LineageEvent {
  id: string;
  record_id: string;
  dataset: string;
  timestamp: string;
  operation: 'create' | 'update' | 'transform' | 'merge' | 'quality_check';
  model_refs?: ModelReference[];
  source_refs?: DataReference[];
  run_id?: string;
  metadata?: Record<string, any>;
}

export interface ModelReference {
  model_id: string;
  version: string;
  confidence?: number;
}

export interface DataReference {
  system: string;
  path: string;
  version?: string;
  extracted_at: string;
  cdc_position?: string;
}

export interface LineageQueryParams {
  entity_id?: string;
  model_id?: string;
  dataset?: string;
  start_time?: string;
  end_time?: string;
  depth?: number;
  include_models?: boolean;
  as_of?: string;  // Time travel parameter
}

export interface ImpactAnalysis {
  source: DataReference;
  impact_type: 'forward' | 'backward';
  affected_entities: string[];
  affected_models: string[];
  confidence_levels: Record<string, number>;
  analysis_timestamp: string;
}

export interface ProposedChange {
  change_type: 'schema' | 'model' | 'data_quality_rule';
  target: string;
  description: string;
  estimated_impact?: ImpactAnalysis;
}