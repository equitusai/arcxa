/**
 * React Query hooks for datasource management
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as datasourceApi from '@/api/datasources';
import type {
  RegisterDatasourceRequest,
  UpdateDatasourceRequest,
} from '@/api/types';

function getErrorMessage(error: unknown): string {
  const apiError = error as {
    message?: string;
    response?: {
      data?: {
        error?: string;
      };
    };
  };

  return apiError.response?.data?.error || apiError.message || 'Request failed';
}

/**
 * Get all datasources
 */
export function useDatasources() {
  return useQuery({
    queryKey: ['datasources'],
    queryFn: datasourceApi.getDatasources,
    staleTime: 30 * 1000,
  });
}

/**
 * Get single datasource
 */
export function useDatasource(id: string | undefined) {
  return useQuery({
    queryKey: ['datasources', id],
    queryFn: () => datasourceApi.getDatasource(id!),
    enabled: !!id,
    staleTime: 30 * 1000,
  });
}

/**
 * Register new datasource
 */
export function useRegisterDatasource() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: RegisterDatasourceRequest) =>
      datasourceApi.registerDatasource(request),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ['datasources'] });
      queryClient.invalidateQueries({ queryKey: ['datasources', 'stats'] });
      toast.success('Data source registered', {
        description: `${data.name} has been registered successfully`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to register data source', {
        description: getErrorMessage(error),
      });
    },
  });
}

/**
 * Update datasource
 */
export function useUpdateDatasource() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, request }: { id: string; request: UpdateDatasourceRequest }) =>
      datasourceApi.updateDatasource(id, request),
    onSuccess: (data, variables) => {
      queryClient.invalidateQueries({ queryKey: ['datasources'] });
      queryClient.invalidateQueries({ queryKey: ['datasources', variables.id] });
      toast.success('Data source updated');
    },
    onError: (error: unknown) => {
      toast.error('Failed to update data source', {
        description: getErrorMessage(error),
      });
    },
  });
}

/**
 * Delete datasource
 */
export function useDeleteDatasource() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => datasourceApi.deleteDatasource(id),
    onSuccess: (_, id) => {
      queryClient.invalidateQueries({ queryKey: ['datasources'] });
      queryClient.invalidateQueries({ queryKey: ['datasources', 'stats'] });
      queryClient.removeQueries({ queryKey: ['datasources', id] });
      toast.success('Data source deleted');
    },
    onError: (error: unknown) => {
      toast.error('Failed to delete data source', {
        description: getErrorMessage(error),
      });
    },
  });
}

/**
 * Test datasource connection
 */
export function useTestConnection() {
  return useMutation({
    mutationFn: (id: string) => datasourceApi.testConnection(id),
    onSuccess: (data) => {
      if (data.success) {
        toast.success('Connection successful', {
          description: `Connected in ${data.latency_ms}ms`,
        });
      } else {
        toast.warning('Connection test failed', {
          description: data.message,
        });
      }
    },
    onError: (error: unknown) => {
      toast.error('Connection test failed', {
        description: getErrorMessage(error),
      });
    },
  });
}


/**
 * Get datasource health
 */
export function useDatasourceHealth(id: string | undefined) {
  return useQuery({
    queryKey: ['datasources', id, 'health'],
    queryFn: () => datasourceApi.getDatasourceHealth(id!),
    enabled: !!id,
    staleTime: 10 * 1000,
    refetchInterval: 30 * 1000,
  });
}

/**
 * Get datasource schema
 */
export function useDatasourceSchema(id: string | undefined) {
  return useQuery({
    queryKey: ['datasources', id, 'schema'],
    queryFn: () => datasourceApi.getDatasourceSchema(id!),
    enabled: !!id,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}

/**
 * Get available plugins
 */
export function useAvailablePlugins() {
  return useQuery({
    queryKey: ['datasources', 'plugins'],
    queryFn: datasourceApi.getAvailablePlugins,
    staleTime: 60 * 60 * 1000, // 1 hour
  });
}

/**
 * Get datasource statistics
 */
export function useDatasourceStats() {
  return useQuery({
    queryKey: ['datasources', 'stats'],
    queryFn: datasourceApi.getDatasourceStats,
    staleTime: 30 * 1000,
  });
}

/**
 * Enable datasource
 */
export function useEnableDatasource() {
  return useMutation({
    mutationFn: (id: string) => datasourceApi.enableDatasource(id),
    onError: (error: unknown) => {
      toast.error('Data source toggle unavailable', {
        description: getErrorMessage(error),
      });
    },
  });
}

/**
 * Disable datasource
 */
export function useDisableDatasource() {
  return useMutation({
    mutationFn: (id: string) => datasourceApi.disableDatasource(id),
    onError: (error: unknown) => {
      toast.error('Data source toggle unavailable', {
        description: getErrorMessage(error),
      });
    },
  });
}

/**
 * Refresh datasource schema
 */
export function useRefreshSchema() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => datasourceApi.refreshSchema(id),
    onSuccess: (data, id) => {
      queryClient.invalidateQueries({ queryKey: ['datasources', id, 'schema'] });
      toast.success('Schema refreshed', {
        description: `Found ${data.total_tables} tables, ${data.total_columns} columns`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to refresh schema', {
        description: getErrorMessage(error),
      });
    },
  });
}
