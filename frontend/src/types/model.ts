// Model registry types

export interface Model {
  id: string;
  model_id: string;
  name: string;
  version: string;
  model_type: 'classification' | 'regression' | 'clustering' | 'entity_resolution' | 'quality_assessment';
  output_schema: string[];
  deployed_at: string;
  retired_at?: string;
  status: 'active' | 'deprecated' | 'retired';
  metadata: Record<string, any>;
  performance_metrics?: ModelPerformance;
  training_data?: TrainingDataReference;
}

export interface ModelPerformance {
  accuracy?: number;
  precision?: number;
  recall?: number;
  f1_score?: number;
  auc_roc?: number;
  custom_metrics?: Record<string, number>;
  evaluated_at: string;
}

export interface TrainingDataReference {
  dataset_id: string;
  version: string;
  record_count: number;
  features_used: string[];
  training_started_at: string;
  training_completed_at: string;
}

export interface RegisterGovernanceModelRequest {
  model_id: string;
  name: string;
  version: string;
  model_type: Model['model_type'];
  output_schema: string[];
  metadata?: Record<string, any>;
}

export interface ModelImpact {
  model_id: string;
  version: string;
  affected_entities: number;
  affected_datasets: string[];
  downstream_models: string[];
  last_prediction_at?: string;
}

export interface ModelPrediction {
  entity_id: string;
  model_id: string;
  model_version: string;
  attribute_name: string;
  value: any;
  confidence: number;
  timestamp: string;
  explanation?: Record<string, number>;
}