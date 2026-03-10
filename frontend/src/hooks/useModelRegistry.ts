import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as modelRegistryApi from '@/api/modelRegistry';
import { RegisterModelRequest } from '@/api/types';

export function useMlModels() {
  return useQuery({
    queryKey: ['ml-models'],
    queryFn: () => modelRegistryApi.listMlModels(),
    staleTime: 2 * 60 * 1000,
  });
}

export function useMlModel(modelId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['ml-models', modelId],
    queryFn: () => modelRegistryApi.getMlModel(modelId!),
    enabled: enabled && !!modelId,
    staleTime: 2 * 60 * 1000,
  });
}

export function useRegisterMlModel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: RegisterModelRequest) => modelRegistryApi.registerMlModel(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ml-models'] });
      toast.success('ML model registered successfully');
    },
    onError: (error: any) => {
      console.error('ML model registration failed:', error);
      toast.error('Failed to register ML model');
    },
  });
}

export function useDeleteMlModel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (modelId: string) => modelRegistryApi.deleteMlModel(modelId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ml-models'] });
      toast.success('ML model deleted successfully');
    },
    onError: (error: any) => {
      console.error('ML model deletion failed:', error);
      toast.error('Failed to delete ML model');
    },
  });
}
