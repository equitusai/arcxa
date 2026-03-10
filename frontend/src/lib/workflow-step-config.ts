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

export interface StepTypeConfig {
  id: StepType;
  label: string;
  icon: any;
  color: {
    base: string;
    subtle: string;
    border: string;
    text: string;
  };
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
    color: {
      base: '#0078D4',
      subtle: 'rgba(0, 120, 212, 0.1)',
      border: 'rgb(0, 120, 212)',
      text: 'rgb(0, 120, 212)',
    },
    description: 'Invoke ML model for predictions',
    category: 'prediction',
  },
  heuristic_rule: {
    id: 'heuristic_rule',
    label: 'Heuristic Rule',
    icon: Lightbulb,
    color: {
      base: '#FFB900',
      subtle: 'rgba(255, 185, 0, 0.1)',
      border: 'rgb(255, 185, 0)',
      text: 'rgb(255, 185, 0)',
    },
    description: 'Apply business logic rules',
    category: 'logic',
  },
  wasm_rule: {
    id: 'wasm_rule',
    label: 'WASM Rule',
    icon: Code2,
    color: {
      base: '#5C2E91',
      subtle: 'rgba(92, 46, 145, 0.1)',
      border: 'rgb(92, 46, 145)',
      text: 'rgb(92, 46, 145)',
    },
    description: 'Execute compiled WASM logic',
    category: 'logic',
  },
  confidence_gate: {
    id: 'confidence_gate',
    label: 'Confidence Gate',
    icon: ShieldCheck,
    color: {
      base: '#107C10',
      subtle: 'rgba(16, 124, 16, 0.1)',
      border: 'rgb(16, 124, 16)',
      text: 'rgb(16, 124, 16)',
    },
    description: 'Filter by confidence threshold',
    category: 'logic',
  },
  weighted_vote: {
    id: 'weighted_vote',
    label: 'Weighted Vote',
    icon: Scale,
    color: {
      base: '#00BCF2',
      subtle: 'rgba(0, 188, 242, 0.1)',
      border: 'rgb(0, 188, 242)',
      text: 'rgb(0, 188, 242)',
    },
    description: 'Combine results with weights',
    category: 'aggregation',
  },
  confidence_aggregate: {
    id: 'confidence_aggregate',
    label: 'Confidence Aggregate',
    icon: Sigma,
    color: {
      base: '#E74856',
      subtle: 'rgba(231, 72, 86, 0.1)',
      border: 'rgb(231, 72, 86)',
      text: 'rgb(231, 72, 86)',
    },
    description: 'Aggregate confidence scores',
    category: 'aggregation',
  },
  conditional_router: {
    id: 'conditional_router',
    label: 'Conditional Router',
    icon: GitBranch,
    color: {
      base: '#8764B8',
      subtle: 'rgba(135, 100, 184, 0.1)',
      border: 'rgb(135, 100, 184)',
      text: 'rgb(135, 100, 184)',
    },
    description: 'Route based on conditions (if-then-else)',
    category: 'routing',
    shape: 'diamond',
  },
  field_mapper: {
    id: 'field_mapper',
    label: 'Field Mapper',
    icon: Layers,
    color: {
      base: '#00CC6A',
      subtle: 'rgba(0, 204, 106, 0.1)',
      border: 'rgb(0, 204, 106)',
      text: 'rgb(0, 204, 106)',
    },
    description: 'Map multiple sources to ontology fields with weighted voting',
    category: 'transformation',
  },
  data_transformer: {
    id: 'data_transformer',
    label: 'Data Transformer',
    icon: Wand2,
    color: {
      base: '#FF8C00',
      subtle: 'rgba(255, 140, 0, 0.1)',
      border: 'rgb(255, 140, 0)',
      text: 'rgb(255, 140, 0)',
    },
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
