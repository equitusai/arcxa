/**
 * Cluster Management Hooks
 *
 * React Query hooks for cluster topology, health, statistics, and configuration
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as clusterApi from '@/api/cluster';
import type {
  ClusterConfigResponse,
  ScaleOutRequest,
} from '@/api/types';

/**
 * Fetch cluster topology with optional auto-refresh
 */
export function useClusterTopology(refetchInterval?: number) {
  return useQuery({
    queryKey: ['cluster', 'topology'],
    queryFn: clusterApi.getClusterTopology,
    refetchInterval: refetchInterval || false,
    staleTime: 5000, // 5 seconds
    refetchOnWindowFocus: false, // Prevent refetch on tab focus for smoother UX
    placeholderData: (previousData) => previousData, // Keep showing old data during refetch
  });
}

/**
 * Fetch cluster statistics with optional auto-refresh
 * Enhanced with smooth UX options - keeps old data visible during refresh
 */
export function useClusterStats(refetchInterval?: number) {
  return useQuery({
    queryKey: ['cluster', 'stats'],
    queryFn: clusterApi.getClusterStats,
    refetchInterval: refetchInterval || false,
    staleTime: 5000,
    refetchOnWindowFocus: false, // Prevent refetch on tab focus
    placeholderData: (previousData) => previousData, // Keep previous data during refresh
  });
}

/**
 * Fetch cluster health with auto-refresh (default 5s)
 * Enhanced with smooth UX options - no jarring refreshes
 */
export function useClusterHealth(refetchInterval: number = 5000) {
  return useQuery({
    queryKey: ['cluster', 'health'],
    queryFn: clusterApi.getClusterHealth,
    refetchInterval,
    staleTime: 3000,
    refetchOnWindowFocus: false, // Don't refetch on tab focus - prevents double refresh
    placeholderData: (previousData) => previousData, // Keep showing old data while fetching new
    // This prevents the component from unmounting during refresh
  });
}

/**
 * Fetch cluster configuration
 */
export function useClusterConfig() {
  return useQuery({
    queryKey: ['cluster', 'config'],
    queryFn: clusterApi.getClusterConfig,
    staleTime: 30000, // 30 seconds - config doesn't change often
  });
}

/**
 * Fetch shard detail by ID
 */
export function useShardDetail(shardId: number | null) {
  return useQuery({
    queryKey: ['cluster', 'shard', shardId],
    queryFn: () => clusterApi.getShardDetail(shardId!),
    enabled: shardId !== null,
    staleTime: 5000,
  });
}

/**
 * Fetch replication configuration
 */
export function useReplicationConfig() {
  return useQuery({
    queryKey: ['cluster', 'replication', 'config'],
    queryFn: clusterApi.getReplicationConfig,
    staleTime: 30000,
  });
}

/**
 * Fetch cluster metadata
 */
export function useClusterMetadata() {
  return useQuery({
    queryKey: ['cluster', 'metadata'],
    queryFn: clusterApi.getClusterMetadata,
    staleTime: 60000, // 1 minute - metadata rarely changes
  });
}

/**
 * Update cluster configuration mutation
 */
export function useUpdateClusterConfig() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (config: Partial<ClusterConfigResponse>) =>
      clusterApi.updateClusterConfig(config),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['cluster', 'config'] });
      toast.success('✅ Cluster configuration updated successfully');
    },
    onError: (error: Error) => {
      toast.error('❌ Failed to update cluster configuration', {
        description: error.message,
      });
    },
  });
}

/**
 * Scale-out cluster mutation
 */
export function useScaleOutCluster() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: ScaleOutRequest) =>
      clusterApi.scaleOutCluster(request),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ['cluster', 'topology'] });
      queryClient.invalidateQueries({ queryKey: ['cluster', 'stats'] });

      if (data.status === 'not_supported') {
        toast.warning('⚠️ Scale-out not available', {
          description: data.message,
        });
      } else {
        toast.success('✅ Scale-out operation initiated', {
          description: `Operation ID: ${data.operation_id}`,
        });
      }
    },
    onError: (error: Error) => {
      toast.error('❌ Failed to scale out cluster', {
        description: error.message,
      });
    },
  });
}

/**
 * Combined hook for cluster overview data
 * Fetches health, stats, and topology in parallel
 */
export function useClusterOverview(refetchInterval: number = 5000) {
  const health = useClusterHealth(refetchInterval);
  const stats = useClusterStats(refetchInterval);
  const topology = useClusterTopology(refetchInterval);
  const config = useClusterConfig();

  return {
    health,
    stats,
    topology,
    config,
    isLoading: health.isLoading || stats.isLoading || topology.isLoading || config.isLoading,
    isError: health.isError || stats.isError || topology.isError || config.isError,
  };
}
