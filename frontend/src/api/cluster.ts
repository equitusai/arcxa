/**
 * Cluster & Sharding Management API
 *
 * Provides access to cluster topology, health, statistics, and configuration
 */

import { apiClient } from './client';
import type {
  TopologyResponse,
  ClusterStatsResponse,
  ClusterHealthResponse,
  ClusterConfigResponse,
  ScaleOutRequest,
  ScaleOutResponse,
  ShardDetailResponse,
  ReplicationConfigResponse,
  ClusterMetadataResponse,
} from './types';

/**
 * Get current cluster topology and shard distribution
 */
export async function getClusterTopology(): Promise<TopologyResponse> {
  const response = await apiClient.get<TopologyResponse>('/api/v1/cluster/topology');
  return response.data;
}

/**
 * Get cluster-wide statistics
 */
export async function getClusterStats(): Promise<ClusterStatsResponse> {
  const response = await apiClient.get<ClusterStatsResponse>('/api/v1/cluster/stats');
  return response.data;
}

/**
 * Get overall cluster health
 */
export async function getClusterHealth(): Promise<ClusterHealthResponse> {
  const response = await apiClient.get<ClusterHealthResponse>('/api/v1/cluster/health');
  return response.data;
}

/**
 * Get cluster configuration
 */
export async function getClusterConfig(): Promise<ClusterConfigResponse> {
  const response = await apiClient.get<ClusterConfigResponse>('/api/v1/cluster/config');
  return response.data;
}

/**
 * Update cluster configuration (partial update)
 */
export async function updateClusterConfig(
  config: Partial<ClusterConfigResponse>
): Promise<ClusterConfigResponse> {
  const response = await apiClient.patch<ClusterConfigResponse>(
    '/api/v1/cluster/config',
    config
  );
  return response.data;
}

/**
 * Add new shards to the cluster (scale-out operation)
 */
export async function scaleOutCluster(
  request: ScaleOutRequest
): Promise<ScaleOutResponse> {
  const response = await apiClient.post<ScaleOutResponse>(
    '/api/v1/cluster/scale-out',
    request
  );
  return response.data;
}

/**
 * Get detailed shard information
 */
export async function getShardDetail(shardId: number): Promise<ShardDetailResponse> {
  const response = await apiClient.get<ShardDetailResponse>(
    `/api/v1/cluster/shards/${shardId}`
  );
  return response.data;
}

/**
 * Get current replication configuration
 */
export async function getReplicationConfig(): Promise<ReplicationConfigResponse> {
  const response = await apiClient.get<ReplicationConfigResponse>(
    '/api/v1/cluster/replication/config'
  );
  return response.data;
}

/**
 * Get cluster metadata and versioning
 */
export async function getClusterMetadata(): Promise<ClusterMetadataResponse> {
  const response = await apiClient.get<ClusterMetadataResponse>(
    '/api/v1/cluster/metadata'
  );
  return response.data;
}
