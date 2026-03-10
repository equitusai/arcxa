/**
 * React Query hooks for Entity Fusion operations
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import {
  proposeFusionCandidates,
  listFusionCandidates,
  approveFusionCandidate,
  rejectFusionCandidate,
  resolveFusion,
  reverseFusion,
  getFusion,
  listFusions,
} from '@/api/fusion';
import type {
  ProposeFusionRequest,
  FusionCandidateQuery,
  ReviewCandidateRequest,
  FusionResolveRequest,
  ReverseFusionRequest,
} from '@/api/types';

/**
 * List fusion candidates
 */
export function useFusionCandidates(query?: FusionCandidateQuery) {
  return useQuery({
    queryKey: ['fusion-candidates', query],
    queryFn: () => listFusionCandidates(query),
  });
}

/**
 * Propose fusion candidates
 */
export function useProposeFusion() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: ProposeFusionRequest) => proposeFusionCandidates(request),
    onSuccess: (data) => {
      toast.success(
        `Found ${data.total_count} fusion ${data.total_count === 1 ? 'candidate' : 'candidates'}`,
        {
          description: `Based on ${data.candidates[0]?.match_rule || 'matching'} rule`,
        }
      );
      // Invalidate candidates list
      queryClient.invalidateQueries({ queryKey: ['fusion-candidates'] });
    },
    onError: (error: any) => {
      toast.error('Failed to propose fusion candidates', {
        description: error?.message || 'Unknown error occurred',
      });
    },
  });
}

/**
 * Approve fusion candidate
 */
export function useApproveFusionCandidate() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      candidateId,
      request,
    }: {
      candidateId: string;
      request: ReviewCandidateRequest;
    }) => approveFusionCandidate(candidateId, request),
    onSuccess: () => {
      toast.success('Candidate approved', {
        description: 'Ready to commit fusion',
      });
      queryClient.invalidateQueries({ queryKey: ['fusion-candidates'] });
    },
    onError: (error: any) => {
      toast.error('Failed to approve candidate', {
        description: error?.message || 'Unknown error occurred',
      });
    },
  });
}

/**
 * Reject fusion candidate
 */
export function useRejectFusionCandidate() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      candidateId,
      request,
    }: {
      candidateId: string;
      request: ReviewCandidateRequest;
    }) => rejectFusionCandidate(candidateId, request),
    onSuccess: () => {
      toast.success('Candidate rejected', {
        description: 'Marked as false positive',
      });
      queryClient.invalidateQueries({ queryKey: ['fusion-candidates'] });
    },
    onError: (error: any) => {
      toast.error('Failed to reject candidate', {
        description: error?.message || 'Unknown error occurred',
      });
    },
  });
}

/**
 * Commit fusion (resolve entities)
 */
export function useResolveFusion() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: FusionResolveRequest) => resolveFusion(request),
    onSuccess: (data) => {
      toast.success('Fusion committed successfully', {
        description: `Merged ${data.source_entity_ids.length} entities into ${data.merged_entity_id}`,
      });
      // Invalidate relevant queries
      queryClient.invalidateQueries({ queryKey: ['fusion-candidates'] });
      queryClient.invalidateQueries({ queryKey: ['entities'] });
    },
    onError: (error: any) => {
      toast.error('Failed to commit fusion', {
        description: error?.message || 'Unknown error occurred',
      });
    },
  });
}

/**
 * Get fusion by ID
 */
export function useFusion(fusionId: string) {
  return useQuery({
    queryKey: ['fusion', fusionId],
    queryFn: () => getFusion(fusionId),
    enabled: !!fusionId,
  });
}

/**
 * List fusion history
 */
export function useFusionHistory(params?: { limit?: number; offset?: number; status?: string }) {
  return useQuery({
    queryKey: ['fusion-history', params],
    queryFn: () => listFusions(params),
  });
}

/**
 * Reverse fusion operation
 */
export function useReverseFusion() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ fusionId, request }: { fusionId: string; request: ReverseFusionRequest }) =>
      reverseFusion(fusionId, request),
    onSuccess: (data) => {
      toast.success('Fusion reversed', {
        description: data.reason || 'Operation undone successfully',
      });
      queryClient.invalidateQueries({ queryKey: ['fusion-candidates'] });
      queryClient.invalidateQueries({ queryKey: ['fusion-history'] });
      queryClient.invalidateQueries({ queryKey: ['entities'] });
    },
    onError: (error: any) => {
      toast.error('Failed to reverse fusion', {
        description: error?.message || 'Unknown error occurred',
      });
    },
  });
}
