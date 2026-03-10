/**
 * Dataset Management API
 */

import api from './client';
import { Dataset, DatasetListResponse, DatasetStats } from './types';

// Toggle for mock data (set to false when backend is ready)
const USE_MOCK_DATA = false;

// Mock data for frontend development
const mockDatasets: Dataset[] = [
  {
    id: 'customers',
    name: 'Customer Master',
    description: 'Primary customer database from PostgreSQL',
    record_count: 1248000,
    entity_count: 1248000,
    size_bytes: 524288000,
    source: 'postgresql-prod',
    source_name: 'PostgreSQL_Prod',
    quality_score: 87,
    quality_breakdown: {
      completeness: 89,
      validity: 94,
      uniqueness: 100,
      timeliness: 78,
    },
    fusion_candidates: 234,
    status: 'active',
    created_at: '2024-10-01T10:00:00Z',
    updated_at: new Date(Date.now() - 120000).toISOString(), // 2 min ago
    last_updated: new Date(Date.now() - 120000).toISOString(),
  },
  {
    id: 'vendors',
    name: 'Vendor Database',
    description: 'Vendor records from SAP HANA',
    record_count: 45000,
    entity_count: 45000,
    size_bytes: 18874368,
    source: 'sap-hana-prod',
    source_name: 'SAP_HANA_Prod',
    quality_score: 62,
    quality_breakdown: {
      completeness: 58,
      validity: 71,
      uniqueness: 95,
      timeliness: 85,
    },
    fusion_candidates: 45,
    status: 'active',
    created_at: '2024-09-15T14:30:00Z',
    updated_at: new Date(Date.now() - 3600000).toISOString(), // 1 hour ago
    last_updated: new Date(Date.now() - 3600000).toISOString(),
  },
  {
    id: 'products',
    name: 'Product Catalog',
    description: 'Product inventory from Snowflake',
    record_count: 8492,
    entity_count: 8492,
    size_bytes: 3538944,
    source: 'snowflake-warehouse',
    source_name: 'Snowflake_Warehouse',
    quality_score: 92,
    quality_breakdown: {
      completeness: 95,
      validity: 97,
      uniqueness: 100,
      timeliness: 92,
    },
    fusion_candidates: 3,
    status: 'active',
    created_at: '2024-09-20T09:15:00Z',
    updated_at: new Date(Date.now() - 300000).toISOString(), // 5 min ago
    last_updated: new Date(Date.now() - 300000).toISOString(),
  },
];

export interface DatasetListFilters {
  source?: string;
  minQuality?: number;
  status?: string;
  datasetType?: Dataset['dataset_type'];
  datasetScope?: 'materialized' | 'source_assets' | 'all';
}

export async function listDatasets(filters?: DatasetListFilters): Promise<DatasetListResponse> {
  if (USE_MOCK_DATA) {
    // Filter mock data based on criteria
    let filtered = [...mockDatasets];

    const datasetScope = filters?.datasetScope ?? 'materialized';
    if (datasetScope === 'materialized') {
      filtered = filtered.filter((d) => d.asset_kind !== 'source_asset' && d.dataset_type !== 'source');
    } else if (datasetScope === 'source_assets') {
      filtered = filtered.filter((d) => d.asset_kind === 'source_asset' || d.dataset_type === 'source');
    }
    if (filters?.datasetType) {
      filtered = filtered.filter((d) => d.dataset_type === filters.datasetType);
    }
    if (filters?.source) {
      filtered = filtered.filter((d) => d.source === filters.source);
    }
    if (filters?.minQuality) {
      filtered = filtered.filter((d) => (d.quality_score || 0) >= filters.minQuality!);
    }
    if (filters?.status) {
      filtered = filtered.filter((d) => d.status === filters.status);
    }

    return Promise.resolve({
      datasets: filtered,
      total: filtered.length,
    });
  }

  const params = new URLSearchParams();
  if (filters?.source) params.set('source', filters.source);
  if (filters?.minQuality) params.set('min_quality', filters.minQuality.toString());
  if (filters?.status) params.set('status', filters.status);
  if (filters?.datasetType) params.set('dataset_type', filters.datasetType);
  params.set('dataset_scope', filters?.datasetScope ?? 'materialized');

  const queryString = params.toString();
  const response = await api.get<DatasetListResponse>(
    `/datasets${queryString ? `?${queryString}` : ''}`
  );

  return response;
}

export async function getDataset(datasetId: string): Promise<Dataset> {
  if (USE_MOCK_DATA) {
    const dataset = mockDatasets.find((d) => d.id === datasetId);
    if (!dataset) {
      throw new Error(`Dataset not found: ${datasetId}`);
    }
    return Promise.resolve(dataset);
  }

  return api.get<Dataset>(`/datasets/${datasetId}`);
}

export async function getDatasetStats(datasetId: string): Promise<DatasetStats> {
  if (USE_MOCK_DATA) {
    // Mock stats data
    const mockStats: DatasetStats = {
      total_entities: mockDatasets.find((d) => d.id === datasetId)?.entity_count || 0,
      avg_confidence: 0.87,
      fusion_operations: {
        total_committed: datasetId === 'customers' ? 248 : 12,
        pending_candidates: mockDatasets.find((d) => d.id === datasetId)?.fusion_candidates || 0,
        last_fusion_at: new Date(Date.now() - 86400000).toISOString(), // 1 day ago
      },
      workflows: {
        active_count: datasetId === 'customers' ? 3 : 1,
        total_executions: datasetId === 'customers' ? 156 : 42,
        last_execution_at: new Date(Date.now() - 7200000).toISOString(), // 2 hours ago
      },
      quality_metrics: mockDatasets.find((d) => d.id === datasetId)?.quality_breakdown,
    };
    return Promise.resolve(mockStats);
  }

  return api.get<DatasetStats>(`/datasets/${datasetId}/stats`);
}

/**
 * Profile dataset - Run data profiling analysis
 * Backend endpoint: POST /api/v1/datasets/:id/profile
 */
export async function profileDataset(datasetId: string): Promise<{
  job_id: string;
  status: 'started' | 'completed' | 'failed';
  message: string;
}> {
  return api.post(`/datasets/${datasetId}/profile`, {});
}

/**
 * Export dataset metadata
 * Backend endpoint: POST /api/v1/datasets/:id/export
 */
export async function exportDatasetMetadata(
  datasetId: string,
  format: 'json' | 'csv' = 'json'
): Promise<{
  export_url: string;
  expires_at: string;
}> {
  return api.post(`/datasets/${datasetId}/export`, { format });
}

/**
 * Clone dataset
 * Backend endpoint: POST /api/v1/datasets/:id/clone
 */
export async function cloneDataset(
  datasetId: string,
  newName: string
): Promise<{
  dataset_id: string;
  name: string;
  created_at: string;
}> {
  return api.post(`/datasets/${datasetId}/clone`, { name: newName });
}

/**
 * Refresh dataset schema
 * Backend endpoint: POST /api/v1/datasets/:id/schema/refresh
 */
export async function refreshDatasetSchema(datasetId: string): Promise<{
  success: boolean;
  message: string;
  schema?: Record<string, unknown>;
}> {
  return api.post(`/datasets/${datasetId}/schema/refresh`, {});
}

/**
 * Archive dataset
 * Backend endpoint: POST /api/v1/datasets/:id/archive
 */
export async function archiveDataset(datasetId: string): Promise<{
  success: boolean;
  message: string;
  archived_at: string;
}> {
  return api.post(`/datasets/${datasetId}/archive`, {});
}
