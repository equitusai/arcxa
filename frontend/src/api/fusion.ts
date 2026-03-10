/**
 * Entity Fusion API
 *
 * Handles entity resolution, candidate management, and fusion operations
 */

import { api } from './client';
import type {
  ProposeFusionRequest,
  ProposeFusionResponse,
  FusionCandidateQuery,
  FusionCandidateListResponse,
  ReviewCandidateRequest,
  ReviewCandidateResponse,
  FusionResolveRequest,
  FusionResolveResponse,
  ReverseFusionRequest,
  ReverseFusionResponse,
} from './types';

/**
 * Propose fusion candidates based on matching rules
 * Phase 1: AI-assisted candidate detection
 */
export async function proposeFusionCandidates(
  request: ProposeFusionRequest
): Promise<ProposeFusionResponse> {
  return api.post('/fusion/propose', request);
}

/**
 * List fusion candidates by status
 * Phase 2: Human review
 */
export async function listFusionCandidates(
  query?: FusionCandidateQuery
): Promise<FusionCandidateListResponse> {
  const params = new URLSearchParams();
  if (query?.status) params.append('status', query.status);
  if (query?.limit) params.append('limit', query.limit.toString());

  const queryString = params.toString();
  return api.get(`/fusion/candidates${queryString ? `?${queryString}` : ''}`);
}

/**
 * Approve a fusion candidate
 * Marks candidate as approved and ready for commit
 */
export async function approveFusionCandidate(
  candidateId: string,
  request: ReviewCandidateRequest
): Promise<ReviewCandidateResponse> {
  return api.post(`/fusion/candidates/${candidateId}/approve`, request);
}

/**
 * Reject a fusion candidate
 * Marks candidate as rejected (false positive)
 */
export async function rejectFusionCandidate(
  candidateId: string,
  request: ReviewCandidateRequest
): Promise<ReviewCandidateResponse> {
  return api.post(`/fusion/candidates/${candidateId}/reject`, request);
}

/**
 * Commit fusion operation
 * Phase 3: Execute the merge
 */
export async function resolveFusion(
  request: FusionResolveRequest
): Promise<FusionResolveResponse> {
  return api.post('/fusion/resolve', request);
}

/**
 * Get fusion details by ID
 */
export async function getFusion(fusionId: string): Promise<any> {
  return api.get(`/fusion/${fusionId}`);
}

/**
 * List all committed fusions (history)
 */
export async function listFusions(params?: {
  limit?: number;
  offset?: number;
  status?: string;
}): Promise<any> {
  const queryParams = new URLSearchParams();
  if (params?.limit) queryParams.append('limit', params.limit.toString());
  if (params?.offset) queryParams.append('offset', params.offset.toString());
  if (params?.status) queryParams.append('status', params.status);

  const queryString = queryParams.toString();
  return api.get(`/fusions${queryString ? `?${queryString}` : ''}`);
}

/**
 * Reverse a committed fusion
 * Phase 4: Undo operation
 */
export async function reverseFusion(
  fusionId: string,
  request: ReverseFusionRequest
): Promise<ReverseFusionResponse> {
  return api.post(`/fusion/${fusionId}/reverse`, request);
}
