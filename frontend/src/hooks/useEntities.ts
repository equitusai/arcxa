import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as governanceApi from '@/api/governance';
import type { EntityQueryParams } from '@/api/types';

// List entities with optional filters
export function useEntities(params?: EntityQueryParams) {
  return useQuery({
    queryKey: ['entities', 'list', params],
    queryFn: () => governanceApi.getEntities(params),
    staleTime: 60 * 1000, // 1 minute
  });
}

// Get single entity details
export function useEntity(entityId: string | undefined) {
  return useQuery({
    queryKey: ['entities', entityId],
    queryFn: () => governanceApi.getEntity(entityId!),
    enabled: !!entityId,
    staleTime: 30 * 1000,
  });
}

// Get entity attributes
export function useEntityAttributes(entityId: string | undefined) {
  return useQuery({
    queryKey: ['entities', entityId, 'attributes'],
    queryFn: () => governanceApi.getEntityAttributes(entityId!),
    enabled: !!entityId,
    staleTime: 30 * 1000,
  });
}

// Get entity lineage
export function useEntityLineage(entityId: string | undefined) {
  return useQuery({
    queryKey: ['entities', entityId, 'lineage'],
    queryFn: () => governanceApi.getEntityLineage(entityId!),
    enabled: !!entityId,
    staleTime: 60 * 1000,
  });
}

// Get attribute timeseries
export function useAttributeTimeseries(
  entityId: string | undefined,
  attributeName: string | undefined
) {
  return useQuery({
    queryKey: ['entities', entityId, 'attributes', attributeName, 'timeseries'],
    queryFn: () => governanceApi.getAttributeTimeseries(entityId!, attributeName!),
    enabled: !!entityId && !!attributeName,
    staleTime: 60 * 1000,
  });
}

// Get entities by domain
export function useEntitiesByDomain(domain: string | undefined, limit?: number) {
  return useQuery({
    queryKey: ['entities', 'domain', domain, limit],
    queryFn: () => governanceApi.getEntitiesByDomain(domain!, limit),
    enabled: !!domain,
    staleTime: 60 * 1000,
  });
}
