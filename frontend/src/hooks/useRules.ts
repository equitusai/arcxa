import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as rulesApi from '@/api/rules';

export function useLoadRule() {
  return useMutation({
    mutationFn: ({ ruleId, wasmCode }: { ruleId: string; wasmCode: ArrayBuffer }) =>
      rulesApi.loadRule(ruleId, wasmCode),
    onSuccess: () => {
      toast.success('Rule loaded successfully');
    },
    onError: (error: any) => {
      console.error('Rule loading failed:', error);
      toast.error('Failed to load rule');
    },
  });
}

export function useExecuteRule() {
  return useMutation({
    mutationFn: ({ ruleId, input }: { ruleId: string; input: any }) =>
      rulesApi.executeRule(ruleId, input),
    onError: (error: any) => {
      console.error('Rule execution failed:', error);
      toast.error('Failed to execute rule');
    },
  });
}

export function useClearRuleCache() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => rulesApi.clearRuleCache(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['rules'] });
      toast.success('Rule cache cleared successfully');
    },
    onError: (error: any) => {
      console.error('Failed to clear rule cache:', error);
      toast.error('Failed to clear rule cache');
    },
  });
}
