/**
 * Workflow Step Type Configuration
 * Visual design and metadata for each step type (ML/Fusion + ETL)
 */

import {
  Brain,
  Lightbulb,
  Code2,
  ShieldCheck,
  Scale,
  Sigma,
  GitBranch,
  Layers,
  Wand2,
  FileText,
  Database,
  Merge,
  Copy,
  Save,
  Upload,
  FileDown,
  Clock,
} from 'lucide-react';
import type { StepType } from '@/api/types';
import { ETL_STEP_TYPE_CONFIGS } from './workflow-etl-config';
import { getWorkflowCategoryColor, type WorkflowStepColor } from './workflow-colors';

export interface StepTypeConfig {
  id: StepType;
  label: string;
  icon: any;
  color: WorkflowStepColor;
  description: string;
  category: 'prediction' | 'logic' | 'aggregation' | 'routing' | 'transformation' | 'extract' | 'transform' | 'quality' | 'load' | 'orchestration';
  shape?: 'rectangle' | 'diamond' | 'hexagon'; // Node shape override
  inputs?: string[];
  outputs?: string[];
}

export const STEP_TYPE_CONFIGS: Record<StepType, StepTypeConfig> = {
  // Merge ETL configs with ML/Fusion configs
  ...ETL_STEP_TYPE_CONFIGS,

  ml_prediction: {
    id: 'ml_prediction',
    label: 'ML Prediction',
    icon: Brain,
    color: getWorkflowCategoryColor('prediction'),
    description: 'Invoke ML model for predictions',
    category: 'prediction',
  },
  heuristic_rule: {
    id: 'heuristic_rule',
    label: 'Heuristic Rule',
    icon: Lightbulb,
    color: getWorkflowCategoryColor('logic'),
    description: 'Apply business logic rules',
    category: 'logic',
  },
  wasm_rule: {
    id: 'wasm_rule',
    label: 'WASM Rule',
    icon: Code2,
    color: getWorkflowCategoryColor('logic'),
    description: 'Execute compiled WASM logic',
    category: 'logic',
  },
  confidence_gate: {
    id: 'confidence_gate',
    label: 'Confidence Gate',
    icon: ShieldCheck,
    color: getWorkflowCategoryColor('logic'),
    description: 'Filter by confidence threshold',
    category: 'logic',
  },
  weighted_vote: {
    id: 'weighted_vote',
    label: 'Weighted Vote',
    icon: Scale,
    color: getWorkflowCategoryColor('aggregation'),
    description: 'Combine results with weights',
    category: 'aggregation',
  },
  confidence_aggregate: {
    id: 'confidence_aggregate',
    label: 'Confidence Aggregate',
    icon: Sigma,
    color: getWorkflowCategoryColor('aggregation'),
    description: 'Aggregate confidence scores',
    category: 'aggregation',
  },
  conditional_router: {
    id: 'conditional_router',
    label: 'Conditional Router',
    icon: GitBranch,
    color: getWorkflowCategoryColor('routing'),
    description: 'Route based on conditions (if-then-else)',
    category: 'routing',
    shape: 'diamond',
  },
  field_mapper: {
    id: 'field_mapper',
    label: 'Field Mapper',
    icon: Layers,
    color: getWorkflowCategoryColor('transformation'),
    description: 'Map multiple sources to ontology fields with weighted voting',
    category: 'transformation',
  },
  data_transformer: {
    id: 'data_transformer',
    label: 'Data Transformer',
    icon: Wand2,
    color: getWorkflowCategoryColor('transformation'),
    description: 'Normalize, validate, and clean data',
    category: 'transformation',
  },
};

export function getStepTypeConfig(stepType: StepType): StepTypeConfig {
  return STEP_TYPE_CONFIGS[stepType];
}

export function getStepTypesByCategory(category: StepTypeConfig['category']): StepTypeConfig[] {
  return Object.values(STEP_TYPE_CONFIGS).filter(config => config.category === category);
}

export function getAllStepTypes(): StepTypeConfig[] {
  return Object.values(STEP_TYPE_CONFIGS);
}
