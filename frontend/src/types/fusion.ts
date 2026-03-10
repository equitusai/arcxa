/**
 * Entity Fusion Type Definitions
 *
 * Complete type system for the 4-phase fusion workflow:
 * Propose → Review → Commit → Reverse
 */

// ============================================================================
// Core Types
// ============================================================================

export type FusionMatchRule = 'email' | 'phone' | 'ssn' | 'name';

export type FusionCandidateStatus = 'proposed' | 'approved' | 'rejected' | 'committed';

export interface FusionCandidate {
  candidate_id: string;
  entities: Array<{
    id: string;
    [key: string]: any;
  }>;
  match_rule: FusionMatchRule;
  match_value: string;
  confidence: number; // 0.0 - 1.0
  proposed_at: string; // ISO 8601 timestamp
  status: FusionCandidateStatus;
  reviewed_by?: string; // User ID
  reviewed_at?: string; // ISO 8601 timestamp
  review_notes?: string;
  conflicts?: number; // Count of conflicting attributes
  matching_attrs?: number; // Count of matching attributes
}

export interface FusionHistory {
  fusion_id: string;
  merged_entity_id: string;
  source_entity_ids: string[];
  match_rule: FusionMatchRule;
  confidence: number;
  committed_at: string;
  committed_by: string;
  status: 'success' | 'reverted';
  reversal_reason?: string;
  reversed_at?: string;
  reversed_by?: string;
}

// ============================================================================
// API Request/Response Types
// ============================================================================

export interface ProposeCandidatesRequest {
  dataset: string;
  rule: FusionMatchRule;
  min_confidence: number;
}

export interface ProposeCandidatesResponse {
  candidates: FusionCandidate[];
  total: number;
  execution_time_ms: number;
}

export interface GetCandidatesParams {
  status?: FusionCandidateStatus;
  limit?: number;
  min_confidence?: number;
}

export interface ApproveCandidateRequest {
  reviewer: string; // User ID
  notes?: string;
}

export interface RejectCandidateRequest {
  reviewer: string; // User ID
  notes?: string;
}

export interface CommitFusionRequest {
  entities: string[]; // Entity IDs to merge
  rule: FusionMatchRule;
  confidence: number;
}

export interface CommitFusionResponse {
  fusion_id: string;
  merged_entity_id: string;
  source_entity_ids: string[];
}

export interface ReverseFusionRequest {
  reason: string;
}

// ============================================================================
// UI State Types
// ============================================================================

export interface FusionFilterState {
  status: FusionCandidateStatus | 'all';
  minConfidence: number;
  maxConfidence: number;
  matchRule?: FusionMatchRule;
  searchQuery?: string;
}

export type FusionSortBy = 'confidence' | 'date' | 'entities' | 'conflicts';
export type FusionSortOrder = 'asc' | 'desc';

export interface FusionSortState {
  sortBy: FusionSortBy;
  sortOrder: FusionSortOrder;
}

export interface FusionSelectionState {
  selectedCandidateIds: Set<string>;
  lastSelectedIndex: number | null;
}

export type FusionDialogType = 'none' | 'comparison' | 'review-notes' | 'commit' | 'reverse';

export interface FusionUIState {
  activeDialog: FusionDialogType;
  comparisonCandidateId: string | null;
  commitCandidateIds: string[];
  reverseFusionId: string | null;
}

// ============================================================================
// Configuration Types
// ============================================================================

export interface FusionRuleConfig {
  rule: FusionMatchRule;
  defaultConfidence: number;
  description: string;
  icon: string;
  enabled: boolean;
}

export const FUSION_RULE_CONFIGS: Record<FusionMatchRule, FusionRuleConfig> = {
  email: {
    rule: 'email',
    defaultConfidence: 0.95,
    description: 'Exact email address match',
    icon: 'Mail',
    enabled: true
  },
  phone: {
    rule: 'phone',
    defaultConfidence: 0.90,
    description: 'Normalized phone number match',
    icon: 'Phone',
    enabled: true
  },
  ssn: {
    rule: 'ssn',
    defaultConfidence: 0.99,
    description: 'Social Security Number match',
    icon: 'ShieldCheck',
    enabled: true
  },
  name: {
    rule: 'name',
    defaultConfidence: 0.70,
    description: 'Fuzzy name similarity match',
    icon: 'User',
    enabled: true
  }
};

// ============================================================================
// Entity Comparison Types
// ============================================================================

export interface EntityFieldComparison {
  field: string;
  entity1Value: any;
  entity2Value: any;
  matchStatus: 'match' | 'conflict' | 'missing' | 'different';
  isMatchedField: boolean; // Whether this field was used for matching
  canonicalValue?: any; // Selected value for merged entity
}

export interface EntityComparison {
  candidate: FusionCandidate;
  fieldComparisons: EntityFieldComparison[];
  matchedFields: string[];
  conflictedFields: string[];
  missingFields: string[];
  canonicalEntityId: string; // Which entity survives
}

// ============================================================================
// Utility Types
// ============================================================================

export interface ConfidenceBucket {
  min: number;
  max: number;
  label: string;
  color: 'success' | 'warning' | 'error';
  variant: 'high' | 'medium' | 'low';
}

export const CONFIDENCE_BUCKETS: ConfidenceBucket[] = [
  {
    min: 0.90,
    max: 1.0,
    label: 'High Confidence',
    color: 'success',
    variant: 'high'
  },
  {
    min: 0.75,
    max: 0.89,
    label: 'Medium Confidence',
    color: 'warning',
    variant: 'medium'
  },
  {
    min: 0.0,
    max: 0.74,
    label: 'Low Confidence',
    color: 'error',
    variant: 'low'
  }
];

export function getConfidenceBucket(confidence: number): ConfidenceBucket {
  return CONFIDENCE_BUCKETS.find(
    bucket => confidence >= bucket.min && confidence <= bucket.max
  ) || CONFIDENCE_BUCKETS[2]; // Default to low confidence
}

// ============================================================================
// Validation Types
// ============================================================================

export interface FusionValidationWarning {
  type: 'low_confidence' | 'conflicts' | 'missing_data' | 'already_merged';
  message: string;
  candidateId?: string;
  severity: 'error' | 'warning' | 'info';
}

export interface FusionValidationResult {
  valid: boolean;
  warnings: FusionValidationWarning[];
  errors: FusionValidationWarning[];
}
