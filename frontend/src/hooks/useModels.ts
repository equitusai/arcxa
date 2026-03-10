import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as modelsApi from '@/api/models';
import type { RegisterModelRequest, ModelInvocationRequest } from '@/api/types';

// ============================================================================
// Model Registration & Management
// ============================================================================

export function useModels() {
  return useQuery({
    queryKey: ['models', 'list'],
    queryFn: () => modelsApi.listModels(),
    staleTime: 60 * 1000, // 1 minute
  });
}

export function useModel(modelId: string | undefined) {
  return useQuery({
    queryKey: ['models', modelId],
    queryFn: () => modelsApi.getModel(modelId!),
    enabled: !!modelId,
    staleTime: 30 * 1000,
  });
}

export function useRegisterModel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: RegisterModelRequest) => modelsApi.registerModel(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['models'] });
      toast.success('Model registered successfully');
    },
    onError: (error: any) => {
      console.error('Failed to register model:', error);
      toast.error(error?.response?.data?.message || 'Failed to register model');
    },
  });
}

export function useUpdateModel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ modelId, request }: { modelId: string; request: Partial<RegisterModelRequest> }) =>
      modelsApi.updateModel(modelId, request),
    onSuccess: (_, variables) => {
      queryClient.invalidateQueries({ queryKey: ['models'] });
      queryClient.invalidateQueries({ queryKey: ['models', variables.modelId] });
      toast.success('Model updated successfully');
    },
    onError: (error: any) => {
      console.error('Failed to update model:', error);
      toast.error('Failed to update model');
    },
  });
}

export function useDeleteModel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (modelId: string) => modelsApi.deleteModel(modelId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['models'] });
      toast.success('Model deleted successfully');
    },
    onError: (error: any) => {
      console.error('Failed to delete model:', error);
      toast.error('Failed to delete model');
    },
  });
}

// ============================================================================
// Model Operations
// ============================================================================

export function useInvokeModel() {
  return useMutation({
    mutationFn: (request: ModelInvocationRequest) => modelsApi.invokeModel(request),
    onError: (error: any) => {
      console.error('Model invocation failed:', error);
      toast.error('Model invocation failed');
    },
  });
}

export function useTestEndpoint() {
  return useMutation({
    mutationFn: ({ url, protocol }: { url: string; protocol: string }) =>
      modelsApi.testModelEndpoint(url, protocol),
  });
}

// ============================================================================
// Circuit Breaker & Cache
// ============================================================================

export function useCircuitBreakerStatus(modelId: string | undefined) {
  return useQuery({
    queryKey: ['models', modelId, 'circuit-breaker'],
    queryFn: () => modelsApi.getCircuitBreakerStatus(modelId!),
    enabled: !!modelId,
    refetchInterval: 5000, // Refresh every 5 seconds
    staleTime: 3000,
  });
}

export function useResetCircuitBreaker() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (modelId: string) => modelsApi.resetCircuitBreaker(modelId),
    onSuccess: (_, modelId) => {
      queryClient.invalidateQueries({ queryKey: ['models', modelId, 'circuit-breaker'] });
      toast.success('Circuit breaker reset successfully');
    },
    onError: (error: any) => {
      console.error('Failed to reset circuit breaker:', error);
      toast.error('Failed to reset circuit breaker');
    },
  });
}

export function useCacheStats() {
  return useQuery({
    queryKey: ['models', 'cache', 'stats'],
    queryFn: () => modelsApi.getCacheStats(),
    refetchInterval: 10000, // Refresh every 10 seconds
    staleTime: 5000,
  });
}

export function useClearCache() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => modelsApi.clearCache(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['models', 'cache'] });
      toast.success('Cache cleared successfully');
    },
    onError: (error: any) => {
      console.error('Failed to clear cache:', error);
      toast.error('Failed to clear cache');
    },
  });
}
