/**
 * Dataset Import API
 * For importing tables from datasources into the catalogue
 *
 * This module uses the backend adapter layer for type-safe transformations.
 * @see src/api/adapters/backend-adapter.ts
 */

import { api } from './client';
import { Dataset } from './types';
import {
  transformDatasourceImportRequest,
  transformBatchImportRequest,
  validateImportRequest,
  transformErrorMessage,
} from './adapters/backend-adapter';

// Import job statuses
export type ImportStatus = 'pending' | 'processing' | 'imported' | 'completed_with_errors' | 'failed';

// Import job interfaces
export interface ImportJob {
  import_id: string;
  datasource_id: string;
  table_name: string;
  dataset_name: string;
  status: ImportStatus;
  progress: number; // 0-100
  started_at: string;
  completed_at?: string;
  error?: string;
  error_code?: string;
  dataset_id?: string; // Available after successful import
}

export interface DatasourceImportRequest {
  datasource_id: string;  // Will be mapped to source_id for backend
  table_name: string;
  schema?: string;
  dataset_name: string;
  description?: string;
  tags?: string[];
  profile?: boolean; // Enable quality profiling
  async_mode?: boolean; // Force async mode
}

export interface DatasourceImportResponse {
  dataset_id: string;
  name: string;
  status: ImportStatus;
  record_count: number;
  file_size_bytes: number;
  schema: {
    primary_key?: string | null;
    columns: Array<{
      name: string;
      data_type: string;
      nullable: boolean;
    }>;
  };
  lineage: {
    import_method: string;
    source_file: string;
    imported_by: string;
    imported_at: string;
    import_id: string;
  };
  storage: {
    format: string;
    path: string;
    compressed: boolean;
  };
}

export interface BatchImportRequest {
  datasource_id: string;
  tables: Array<{
    table_name: string;
    schema?: string;
    dataset_name: string;
    description?: string;
    tags?: string[];
  }>;
  profile?: boolean;
}

export interface BatchImportResponse {
  batch_id: string;
  import_ids: string[];
  message: string;
}

export interface ListImportsRequest {
  datasource_id?: string;
  status?: ImportStatus;
  limit?: number;
  offset?: number;
}

export interface ListImportsResponse {
  imports: ImportJob[];
  total: number;
}

/**
 * Import a table from a datasource as a dataset
 *
 * @param request - Frontend import request
 * @returns Backend import response with dataset metadata and lineage
 * @throws {Error} if validation fails or request is malformed
 *
 * @example
 * ```typescript
 * const response = await importDatasourceTable({
 *   datasource_id: "ds-123",
 *   table_name: "users",
 *   dataset_name: "User Data",
 *   description: "Production user records",
 *   tags: ["users", "production"],
 *   profile: true,
 * });
 * ```
 */
export async function importDatasourceTable(
  request: DatasourceImportRequest
): Promise<DatasourceImportResponse> {
  try {
    // Validate request
    validateImportRequest(request);

    // Transform to backend format using adapter
    const backendRequest = transformDatasourceImportRequest(request);

    // Make API call
    return await api.post('/datasets/import-datasource', backendRequest);
  } catch (error) {
    // Transform error to user-friendly message
    const message = transformErrorMessage(error);
    throw new Error(message);
  }
}

/**
 * Import multiple tables from a datasource in batch
 *
 * @param request - Batch import request
 * @returns Batch import response with job IDs
 * @throws {Error} if validation fails
 */
export async function importDatasourceBatch(
  request: BatchImportRequest
): Promise<BatchImportResponse> {
  try {
    // Transform to backend format using adapter
    const backendRequest = transformBatchImportRequest(request);

    // Make API call
    return await api.post('/datasets/import-batch', backendRequest);
  } catch (error) {
    const message = transformErrorMessage(error);
    throw new Error(message);
  }
}

/**
 * Get status of a specific import job
 */
export async function getImportStatus(importId: string): Promise<ImportJob> {
  return api.get(`/datasets/imports/${importId}`);
}

/**
 * List all import jobs with optional filtering
 */
export async function listImports(
  params?: ListImportsRequest
): Promise<ListImportsResponse> {
  const queryParams = new URLSearchParams();
  if (params?.datasource_id) queryParams.set('datasource_id', params.datasource_id);
  if (params?.status) queryParams.set('status', params.status);
  if (params?.limit) queryParams.set('limit', params.limit.toString());
  if (params?.offset) queryParams.set('offset', params.offset.toString());

  const queryString = queryParams.toString();
  return api.get(`/datasets/imports${queryString ? `?${queryString}` : ''}`);
}

/**
 * Poll import status until completion
 * Returns the completed job or throws on error
 */
export async function pollImportUntilComplete(
  importId: string,
  onProgress?: (progress: number) => void,
  pollIntervalMs: number = 1000,
  timeoutMs: number = 300000 // 5 minutes
): Promise<ImportJob> {
  const startTime = Date.now();

  while (Date.now() - startTime <= timeoutMs) {
    const job = await getImportStatus(importId);

    // Update progress callback
    if (onProgress) {
      onProgress(job.progress);
    }

    // Check for completion
    if (job.status === 'imported') {
      return job;
    }

    if (job.status === 'failed') {
      throw new Error(job.error || 'Import failed');
    }

    // Wait before next poll
    await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
  }

  throw new Error('Import polling timed out');
}

/**
 * Get the dataset created by a completed import
 */
export async function getImportedDataset(importId: string): Promise<Dataset | null> {
  const job = await getImportStatus(importId);

  if (job.status !== 'imported' || !job.dataset_id) {
    return null;
  }

  return api.get(`/datasets/${job.dataset_id}`);
}
