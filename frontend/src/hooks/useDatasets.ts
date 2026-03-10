/**
 * React Query hooks for Dataset operations
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as datasetsApi from '@/api/datasets';

/**
 * List datasets with optional filters
 */
export function useDatasets(filters?: {
  source?: string;
  minQuality?: number;
  status?: string;
  datasetType?: datasetsApi.DatasetListFilters['datasetType'];
  datasetScope?: datasetsApi.DatasetListFilters['datasetScope'];
}) {
  return useQuery({
    queryKey: ['datasets', filters],
    queryFn: () => datasetsApi.listDatasets(filters),
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get a single dataset by ID
 */
export function useDataset(datasetId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['datasets', datasetId],
    queryFn: () => datasetsApi.getDataset(datasetId!),
    enabled: enabled && !!datasetId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get dataset statistics including quality metrics, fusion operations, and workflows
 */
export function useDatasetStats(datasetId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['datasets', datasetId, 'stats'],
    queryFn: () => datasetsApi.getDatasetStats(datasetId!),
    enabled: enabled && !!datasetId,
    staleTime: 1 * 60 * 1000, // 1 minute (stats change more frequently)
  });
}

/**
 * Profile dataset - Run data profiling analysis
 */
export function useProfileDataset() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (datasetId: string) => datasetsApi.profileDataset(datasetId),
    onSuccess: (data, datasetId) => {
      toast.success('Data profiling started successfully');
      // Invalidate dataset stats to reflect new profiling data
      queryClient.invalidateQueries({ queryKey: ['datasets', datasetId, 'stats'] });
    },
    onError: (error: any) => {
      toast.error(error?.message || 'Failed to start data profiling');
    },
  });
}

/**
 * Export dataset metadata
 */
export function useExportDatasetMetadata() {
  return useMutation({
    mutationFn: ({ datasetId, format }: { datasetId: string; format?: 'json' | 'csv' }) =>
      datasetsApi.exportDatasetMetadata(datasetId, format),
    onSuccess: (data) => {
      toast.success('Metadata export ready');
      // Open export URL in new tab
      window.open(data.export_url, '_blank');
    },
    onError: (error: any) => {
      toast.error(error?.message || 'Failed to export metadata');
    },
  });
}

/**
 * Clone dataset
 */
export function useCloneDataset() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ datasetId, newName }: { datasetId: string; newName: string }) =>
      datasetsApi.cloneDataset(datasetId, newName),
    onSuccess: (data) => {
      toast.success(`Dataset cloned successfully as "${data.name}"`);
      // Invalidate datasets list to show new clone
      queryClient.invalidateQueries({ queryKey: ['datasets'] });
    },
    onError: (error: any) => {
      toast.error(error?.message || 'Failed to clone dataset');
    },
  });
}

/**
 * Refresh dataset schema
 */
export function useRefreshDatasetSchema() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (datasetId: string) => datasetsApi.refreshDatasetSchema(datasetId),
    onSuccess: (data, datasetId) => {
      toast.success('Schema refreshed successfully');
      // Invalidate dataset to reflect new schema
      queryClient.invalidateQueries({ queryKey: ['datasets', datasetId] });
    },
    onError: (error: any) => {
      toast.error(error?.message || 'Failed to refresh schema');
    },
  });
}

/**
 * Archive dataset
 */
export function useArchiveDataset() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (datasetId: string) => datasetsApi.archiveDataset(datasetId),
    onSuccess: (data, datasetId) => {
      toast.success('Dataset archived successfully');
      // Invalidate datasets list to remove archived dataset
      queryClient.invalidateQueries({ queryKey: ['datasets'] });
      queryClient.invalidateQueries({ queryKey: ['datasets', datasetId] });
    },
    onError: (error: any) => {
      toast.error(error?.message || 'Failed to archive dataset');
    },
  });
}
