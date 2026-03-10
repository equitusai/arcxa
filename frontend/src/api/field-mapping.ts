/**
 * Field Mapping API Module
 *
 * Enables semantic field mapping from datasource fields to ontology terms.
 * Uses AI-powered suggestions with confidence scoring.
 *
 * @see FIELD_MAPPING_API_VALIDATION.md for backend implementation status
 */

import { api } from './client';

// ============================================================================
// Type Definitions
// ============================================================================

export type MappingSessionStatus =
  | 'draft'           // Initial creation
  | 'pending_review'  // AI analysis complete, awaiting user review
  | 'approved'        // User has approved mappings
  | 'applied'         // Mappings stored as RDF triples
  | 'active';         // Ready for data import

export type FieldApprovalStatus =
  | 'pending'         // Awaiting user decision
  | 'auto_approved'   // Auto-approved (confidence > threshold)
  | 'approved'        // User approved
  | 'rejected'        // User rejected
  | 'modified';       // User modified suggestion

export type MappingAction =
  | 'approve'         // Accept top suggestion
  | 'reject'          // Reject all suggestions
  | 'modify';         // Select alternative suggestion

// ============================================================================
// Request/Response Interfaces
// ============================================================================

export interface AnalyzeForMappingRequest {
  tables?: string[];              // Optional: specific tables to analyze
  fields?: Array<{                // Optional: field definitions (required by backend if not inferring)
    name: string;
    data_type: string;
    sample_values?: string[];
  }>;
  sample_size?: number;           // Optional: default 1000
  auto_approve_threshold?: number; // Optional: default 0.95 (95% confidence)
  min_confidence?: number;        // Optional: default 0.5
  max_candidates?: number;        // Optional: default 10
  user_id: string;                // Required: current user ID
  ontology_namespaces?: string[]; // Optional: filter to specific ontology namespaces
                                  // Example: ["http://schema.org/", "http://example.com/retail#"]
                                  // If omitted or empty, uses all active ontologies
}

export interface AnalyzeForMappingResponse {
  session_id: string;              // Use this for subsequent calls
  summary: MappingSessionSummary;
  status: MappingSessionStatus;
  processing_time_ms: number;
}

export interface MappingSessionSummary {
  total_fields: number;
  fields_with_candidates: number;
  auto_approved: number;          // Fields with confidence > 0.95
  pending_review: number;
  user_approved: number;
  rejected: number;
  modified: number;
}

export interface ConfidenceBreakdown {
  statistical: number;       // TF-IDF, n-gram matching (0.0 - 1.0)
  semantic?: number;         // Transformer embeddings (optional)
  graph?: number;            // GNN-based (future)
  symbolic?: number;         // SPARQL reasoning (future)
}

export interface OntologyCandidate {
  ontology_term_uri: string;  // e.g., "http://schema.org/email"
  confidence: number;          // 0.0 - 1.0
  explanation: string;         // Why this was suggested
  confidence_breakdown: ConfidenceBreakdown;
  transformation?: string;     // Optional: "lowercase", "trim", etc.
}

export interface SelectedMapping {
  ontology_term_uri: string;
  confidence: number;
  was_top_candidate: boolean;
  transformation?: string;
}

export interface FieldMapping {
  field_id: string;
  field_name: string;          // e.g., "customer_email"
  data_type: string;            // e.g., "VARCHAR"
  sample_values: string[];
  candidates: OntologyCandidate[];
  selected_mapping?: SelectedMapping;
  approval_status: FieldApprovalStatus;
  reviewed_by?: string;
  reviewed_at?: number;
  notes?: string;
}

export interface TableMapping {
  table_name: string;
  field_mappings: FieldMapping[];
}

export interface MappingSessionConfig {
  sample_size: number;
  auto_approve_threshold: number;
  min_confidence: number;
  max_candidates: number;
  ontology_namespaces?: string[];  // Ontology namespaces used for filtering
}

export interface MappingSession {
  session_id: string;
  source_id: string;
  status: MappingSessionStatus;
  tables: TableMapping[];
  created_by: string;
  created_at: number;
  reviewed_by?: string;
  reviewed_at?: number;
  applied_at?: number;
  config: MappingSessionConfig;
  summary: MappingSessionSummary;
}

export interface FieldMappingDecision {
  field_id: string;
  action: MappingAction;
  selected_mapping?: string;      // Required if action is 'modify'
  notes?: string;
}

export interface ReviewMappingsRequest {
  field_mappings: FieldMappingDecision[];
  reviewed_by: string;              // Current user ID
  finalize: boolean;                // true = move to Approved status
}

export interface ReviewMappingsResponse {
  status: MappingSessionStatus;
  summary: MappingSessionSummary;
  approved_mappings: number;
  ready_to_apply: boolean;
}

export interface ApplyMappingsRequest {
  create_default_import: boolean;  // true = generate import config
}

export interface ApplyMappingsResponse {
  status: 'active';
  rdf_triples_stored: number;
  ready_for_import: boolean;
  default_import_config?: any;
}

export interface ImportDataRequest {
  batch_size?: number;           // Default: 1000
  target_graph?: string;         // Optional: custom named graph URI
  tables?: string[];             // Optional: specific tables to import
  limit?: number;                // Optional: max rows (for testing)
  user_id: string;
}

export interface ImportError {
  table: string;
  row?: number;
  field?: string;
  message: string;
}

export interface ImportStats {
  rows_processed: number;
  entities_created: number;
  triples_stored: number;
  tables_imported: number;
  fields_mapped: number;
  errors: ImportError[];
}

export interface ImportDataResponse {
  import_id: string;
  session_id: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  stats: ImportStats;
  processing_time_ms: number;
  target_graph: string;
}

// ============================================================================
// API Functions
// ============================================================================

// ============================================================================
// Helper Functions for Backend Response Transformation
// ============================================================================

/**
 * Normalize backend status to frontend format
 * Backend uses PascalCase (e.g., "PendingReview"), frontend uses snake_case
 */
function normalizeSessionStatus(backendStatus: string): MappingSessionStatus {
  const mapping: Record<string, MappingSessionStatus> = {
    'Draft': 'draft',
    'PendingReview': 'pending_review',
    'Approved': 'approved',
    'Applied': 'applied',
    'Active': 'active',
  };
  return mapping[backendStatus] || (backendStatus.toLowerCase().replace(/ /g, '_') as MappingSessionStatus);
}

/**
 * Normalize backend approval status to frontend format
 */
function normalizeApprovalStatus(backendStatus: string): FieldApprovalStatus {
  const mapping: Record<string, FieldApprovalStatus> = {
    'Pending': 'pending',
    'AutoApproved': 'auto_approved',
    'Approved': 'approved',
    'Rejected': 'rejected',
    'Modified': 'modified',
  };
  return mapping[backendStatus] || (backendStatus.toLowerCase().replace(/ /g, '_') as FieldApprovalStatus);
}

/**
 * Transform backend session response to frontend format
 */
function transformSessionResponse(backendSession: any): MappingSession {
  return {
    ...backendSession,
    status: normalizeSessionStatus(backendSession.status),
    tables: backendSession.tables?.map((table: any) => ({
      ...table,
      field_mappings: table.field_mappings?.map((fm: any) => ({
        ...fm,
        approval_status: normalizeApprovalStatus(fm.approval_status),
      })) || [],
    })) || [],
  };
}

/**
 * Start a new field mapping session and analyze datasource fields
 *
 * @param datasourceId - Datasource identifier
 * @param request - Analysis configuration
 * @returns Session summary with initial analysis results
 */
export async function analyzeForMapping(
  datasourceId: string,
  request: AnalyzeForMappingRequest
): Promise<AnalyzeForMappingResponse> {
  // Transform frontend request to match backend API structure
  // Backend expects: /api/v1/mapping/analyze with {source_id, table_name, fields, ...}
  const backendRequest = {
    source_id: datasourceId,
    table_name: request.tables?.[0] || 'default_table',
    fields: request.fields || [],
    ontology_namespaces: request.ontology_namespaces || [],
    sample_size: request.sample_size,
    auto_approve_threshold: request.auto_approve_threshold,
    min_confidence: request.min_confidence,
    max_candidates: request.max_candidates,
  };

  const response = await api.post(`/mapping/analyze`, backendRequest);
  return {
    ...response,
    status: normalizeSessionStatus(response.status),
  };
}

/**
 * Get full mapping session details with AI suggestions
 *
 * @param sessionId - Mapping session identifier
 * @returns Complete session with field mappings and candidates
 */
export async function getMappingSession(
  sessionId: string
): Promise<MappingSession> {
  const response = await api.get(`/mapping/sessions/${sessionId}`);
  return transformSessionResponse(response);
}

/**
 * Submit user review decisions for field mappings
 *
 * @param sessionId - Mapping session identifier
 * @param request - User decisions (approve/reject/modify)
 * @returns Updated session summary
 */
export async function reviewMappings(
  sessionId: string,
  request: ReviewMappingsRequest
): Promise<ReviewMappingsResponse> {
  const response = await api.post(`/mapping/sessions/${sessionId}/review`, request);
  return {
    ...response,
    status: normalizeSessionStatus(response.status),
  };
}

/**
 * Apply approved mappings to RDF store
 *
 * @param sessionId - Mapping session identifier
 * @param request - Application configuration
 * @returns RDF storage confirmation
 */
export async function applyMappings(
  sessionId: string,
  request: ApplyMappingsRequest
): Promise<ApplyMappingsResponse> {
  return api.post(`/mapping/sessions/${sessionId}/apply`, request);
}

/**
 * Import data using approved field mappings
 *
 * @param sessionId - Mapping session identifier
 * @param request - Import configuration
 * @returns Import job status and statistics
 */
export async function importDataWithMappings(
  sessionId: string,
  request: ImportDataRequest
): Promise<ImportDataResponse> {
  return api.post(`/mapping/sessions/${sessionId}/import`, request);
}

/**
 * List all mapping sessions for a datasource
 *
 * @param datasourceId - Datasource identifier
 * @returns Array of mapping sessions
 */
export async function listMappingSessions(
  datasourceId: string
): Promise<MappingSession[]> {
  const response = await api.get(`/mapping/sessions?source_id=${datasourceId}`);
  return (Array.isArray(response) ? response : []).map(transformSessionResponse);
}

/**
 * Delete a mapping session
 *
 * @param sessionId - Mapping session identifier
 */
export async function deleteMappingSession(
  sessionId: string
): Promise<void> {
  return api.delete(`/mapping/sessions/${sessionId}`);
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Get confidence level category
 */
export function getConfidenceLevel(confidence: number): 'high' | 'medium' | 'low' {
  if (confidence >= 0.9) return 'high';
  if (confidence >= 0.7) return 'medium';
  return 'low';
}

/**
 * Get confidence color for UI
 */
export function getConfidenceColor(confidence: number): string {
  const level = getConfidenceLevel(confidence);
  switch (level) {
    case 'high': return 'green';
    case 'medium': return 'yellow';
    case 'low': return 'red';
  }
}

/**
 * Format confidence as percentage
 */
export function formatConfidence(confidence: number): string {
  return `${Math.round(confidence * 100)}%`;
}

/**
 * Check if field needs user review
 */
export function needsReview(field: FieldMapping): boolean {
  return field.approval_status === 'pending' &&
         field.candidates.length > 0 &&
         (field.candidates[0]?.confidence || 0) < 0.95;
}

/**
 * Get primary candidate (top suggestion)
 */
export function getPrimaryCandidate(field: FieldMapping): OntologyCandidate | null {
  return field.candidates[0] || null;
}

/**
 * Check if session is ready for next action
 */
export function canReviewSession(session: MappingSession): boolean {
  return session.status === 'pending_review' || session.status === 'draft';
}

export function canApplySession(session: MappingSession): boolean {
  return session.status === 'approved';
}

export function canImportSession(session: MappingSession): boolean {
  return session.status === 'active';
}

/**
 * Calculate completion percentage
 */
export function getCompletionPercentage(summary: MappingSessionSummary): number {
  const { total_fields, auto_approved, user_approved, rejected } = summary;
  if (total_fields === 0) return 0;

  const completed = auto_approved + user_approved + rejected;
  return Math.round((completed / total_fields) * 100);
}

/**
 * Get human-readable status
 */
export function getStatusLabel(status: MappingSessionStatus): string {
  switch (status) {
    case 'draft': return 'Draft';
    case 'pending_review': return 'Pending Review';
    case 'approved': return 'Approved';
    case 'applied': return 'Applied to RDF';
    case 'active': return 'Active';
  }
}

/**
 * Get status color for UI
 */
export function getStatusColor(status: MappingSessionStatus): string {
  switch (status) {
    case 'draft': return 'gray';
    case 'pending_review': return 'yellow';
    case 'approved': return 'blue';
    case 'applied': return 'purple';
    case 'active': return 'green';
  }
}
