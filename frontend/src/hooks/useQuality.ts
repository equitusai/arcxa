/**
 * Quality React Query hooks
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as qualityApi from '@/api/quality';
import { CreateQualityRuleRequest, PaginationParams } from '@/api/types';

export function useQualityScorecard(dataset: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['quality', 'scorecard', dataset],
    queryFn: () => qualityApi.getQualityScorecard(dataset!),
    enabled: enabled && !!dataset,
    staleTime: 1 * 60 * 1000, // 1 minute
  });
}

export function useQualityViolations(
  params?: PaginationParams & { dataset?: string; severity?: string; resolved?: boolean }
) {
  return useQuery({
    queryKey: ['quality', 'violations', params],
    queryFn: () => qualityApi.listQualityViolations(params),
    staleTime: 30 * 1000, // 30 seconds
  });
}

export function useQualityRule(ruleId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['quality', 'rule', ruleId],
    queryFn: () => qualityApi.getQualityRule(ruleId!),
    enabled: enabled && !!ruleId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

export function useCreateQualityRule() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateQualityRuleRequest) => qualityApi.createQualityRule(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['quality'] });
      toast.success('Quality rule created successfully');
    },
    onError: (error: any) => {
      console.error('Failed to create quality rule:', error);
      toast.error('Failed to create quality rule');
    },
  });
}
