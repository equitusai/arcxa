import { useQuery, useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as governanceApi from '@/api/governance';
import { SparqlQueryRequest } from '@/api/types';

export function useModelLineageRdf(modelId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['governance', 'model-lineage', modelId],
    queryFn: () => governanceApi.getModelLineageRdf(modelId!),
    enabled: enabled && !!modelId,
    staleTime: 2 * 60 * 1000,
  });
}

export function useRdfStoreStats() {
  return useQuery({
    queryKey: ['governance', 'stats'],
    queryFn: () => governanceApi.getRdfStoreStats(),
    staleTime: 1 * 60 * 1000,
  });
}

export function useExecuteSparql() {
  return useMutation({
    mutationFn: (request: SparqlQueryRequest) => governanceApi.executeSparqlQuery(request),
    onError: (error: any) => {
      console.error('SPARQL query failed:', error);
      toast.error('SPARQL query failed');
    },
  });
}
