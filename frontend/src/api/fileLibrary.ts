/**
 * File Library API Client
 *
 * Backend endpoints: /api/v1/file-library/*
 * Handles all file, folder, and tag operations
 */

import { api } from './client';
import type {
  FileMetadata,
  FolderMetadata,
  FileUploadRequest,
  FileUploadResponse,
  BulkImportRequest,
  BulkImportResponse,
  FileListParams,
  FileListResponse,
  FolderCreateRequest,
  TagStatistics,
  FileLibraryStats,
  FieldOntologyMapping,
  RegisterFileAsDatasourceRequest,
  RegisterFileAsDatasourceResponse,
  ValidateFileForRegistrationResponse,
} from './types';

const BASE_PATH = '/file-library';

// ============================================================================
// File Operations
// ============================================================================

/**
 * Transform backend response to frontend FileMetadata format
 */
function transformFileResponse(backendFile: any): FileMetadata {
  // If already in correct format, return as-is
  if (backendFile.file_id && backendFile.original_filename) {
    return backendFile as FileMetadata;
  }

  // Infer MIME type from file extension
  const inferMimeType = (filename: string): string => {
    const ext = filename.toLowerCase().split('.').pop() || '';
    const mimeTypes: Record<string, string> = {
      'csv': 'text/csv',
      'tsv': 'text/tab-separated-values',
      'txt': 'text/plain',
      'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'xls': 'application/vnd.ms-excel',
      'json': 'application/json',
      'xml': 'application/xml',
      'pdf': 'application/pdf',
      'zip': 'application/zip',
    };
    return mimeTypes[ext] || 'application/octet-stream';
  };

  // Ensure all values are primitives, not objects
  const ensureString = (val: any): string => {
    if (typeof val === 'string') return val;
    if (val === null || val === undefined) return '';

    // If it's an object, try to extract a useful string
    if (typeof val === 'object') {
      // Try common field names
      if (val.name) return String(val.name);
      if (val.username) return String(val.username);
      if (val.email) return String(val.email);
      if (val.id) return String(val.id);
      // Fall back to empty string for complex objects
      return '';
    }

    return String(val);
  };

  const ensureNumber = (val: any): number => {
    if (typeof val === 'number') return val;
    if (val === null || val === undefined) return 0;

    // If it's an object, return 0
    if (typeof val === 'object') return 0;

    const parsed = Number(val);
    return isNaN(parsed) ? 0 : parsed;
  };

  // Get filename for MIME type inference
  const filename = ensureString(backendFile.name || backendFile.filename || backendFile.original_filename);

  // Determine MIME type: use provided value or infer from filename
  let mimeType = ensureString(backendFile.mime_type);
  if (!mimeType || mimeType === 'application/octet-stream') {
    mimeType = inferMimeType(filename);
  }

  // Transform from backend format to frontend format
  const transformed = {
    file_id: ensureString(backendFile.id || backendFile.file_id),
    filename: filename,
    original_filename: filename,
    mime_type: mimeType,
    size_bytes: ensureNumber(backendFile.size_bytes),
    checksum_sha256: ensureString(backendFile.checksum_sha256),
    uploaded_at: ensureString(backendFile.created_at || backendFile.uploaded_at || new Date().toISOString()),
    uploaded_by: ensureString(backendFile.owner || backendFile.uploaded_by || 'unknown'),
    folder_id: backendFile.folder_id ? ensureString(backendFile.folder_id) : undefined,
    tags: Array.isArray(backendFile.tags) ? backendFile.tags.map((t: any) => ensureString(t)) : [],
    custom_metadata: backendFile.metadata || backendFile.custom_metadata || {},
    access_count: ensureNumber(backendFile.access_count),
    last_accessed_at: backendFile.last_accessed || backendFile.last_accessed_at,
    datasource_id: backendFile.datasource_id ? ensureString(backendFile.datasource_id) : undefined,
    registration_status: (backendFile.registration_status || 'unregistered') as any,
    registered_at: backendFile.registered_at,
    schema: backendFile.schema, // New schema format
    ontology_mappings: backendFile.ontology_mappings, // Ontology mappings
    inferred_schema: backendFile.schema || backendFile.inferred_schema, // Deprecated format
  };

  // Debug logging for problematic fields
  if (typeof backendFile.owner === 'object' || typeof backendFile.uploaded_by === 'object') {
    console.log('[fileLibrary] Object detected in owner/uploaded_by:', {
      owner: backendFile.owner,
      uploaded_by: backendFile.uploaded_by,
      transformed_uploaded_by: transformed.uploaded_by
    });
  }
  if (!backendFile.mime_type && transformed.mime_type !== 'application/octet-stream') {
    console.log('[fileLibrary] MIME type inferred:', {
      filename: filename,
      inferred_mime_type: transformed.mime_type
    });
  }

  return transformed;
}

/**
 * List files with filters and pagination
 */
export async function listFiles(params?: FileListParams): Promise<FileListResponse> {
  const response = await api.get<any>(`${BASE_PATH}/files`, { params });

  console.log('[fileLibrary] Raw API response:', response);

  // Handle different response formats
  let files: any[] = [];

  if (Array.isArray(response)) {
    // Response is directly an array
    files = response;
  } else if (response.files && Array.isArray(response.files)) {
    // Response is an object with files property
    files = response.files;
  } else if (response.data && Array.isArray(response.data)) {
    // Response has data property
    files = response.data;
  }

  // Transform all files
  const transformedFiles = files.map(transformFileResponse);

  console.log('[fileLibrary] Transformed files:', transformedFiles);

  return {
    files: transformedFiles,
    total: response.total || transformedFiles.length,
    page: response.page || 1,
    page_size: response.page_size || transformedFiles.length,
    total_pages: response.total_pages || 1,
  };
}

/**
 * Get single file metadata
 */
export async function getFileMetadata(fileId: string): Promise<FileMetadata> {
  const response = await api.get<any>(`${BASE_PATH}/files/${fileId}`);
  return transformFileResponse(response);
}

/**
 * Upload single file
 * Uses FormData for multipart upload
 */
export async function uploadFile(request: FileUploadRequest): Promise<FileUploadResponse> {
  const formData = new FormData();
  formData.append('file', request.file);

  if (request.folder_id) formData.append('folder_id', request.folder_id);
  // Backend expects comma-separated string, not JSON
  if (request.tags) formData.append('tags', request.tags.join(','));
  // Note: custom_metadata not yet supported by backend
  if (request.custom_metadata) {
    formData.append('custom_metadata', JSON.stringify(request.custom_metadata));
  }
  // Backend expects 'auto_scan', not 'auto_profile'
  if (request.auto_profile !== undefined) {
    formData.append('auto_scan', String(request.auto_profile));
  }

  // Don't set Content-Type manually for FormData - browser sets it with boundary
  const response = await api.post<any>(`${BASE_PATH}/files`, formData);

  // Transform response
  return transformFileResponse(response) as any;
}

/**
 * Download file (returns blob URL)
 */
export async function downloadFile(fileId: string): Promise<string> {
  const blob = await api.get<Blob>(`${BASE_PATH}/files/${fileId}/download`, {
    responseType: 'blob',
  });

  return URL.createObjectURL(blob);
}

/**
 * Delete file
 */
export async function deleteFile(fileId: string): Promise<void> {
  return api.delete<void>(`${BASE_PATH}/files/${fileId}`);
}

/**
 * Update file metadata (tags, folder, custom metadata)
 */
export async function updateFileMetadata(
  fileId: string,
  updates: {
    folder_id?: string;
    tags?: string[];
    custom_metadata?: Record<string, any>;
  }
): Promise<FileMetadata> {
  return api.put<FileMetadata>(`${BASE_PATH}/files/${fileId}`, updates);
}

// ============================================================================
// Bulk Operations
// ============================================================================

/**
 * Upload multiple files
 * Returns job ID for tracking progress
 */
export async function bulkUploadFiles(request: BulkImportRequest): Promise<BulkImportResponse> {
  const formData = new FormData();

  // Append each file
  // Note: Backend doesn't support per-file folder_id or tags yet
  // All files go to the same folder with the same tags
  request.files.forEach((fileReq) => {
    formData.append(`files`, fileReq.file);
  });

  // Common settings (backend expects comma-separated tags, not JSON)
  if (request.folder_id) formData.append('folder_id', request.folder_id);
  if (request.common_tags) formData.append('tags', request.common_tags.join(','));

  // Don't set Content-Type manually for FormData - browser sets it with boundary
  return api.post<BulkImportResponse>(`${BASE_PATH}/files/bulk-upload`, formData);
}

/**
 * Get bulk import job status
 */
export async function getBulkImportStatus(jobId: string): Promise<{
  job_id: string;
  status: 'pending' | 'processing' | 'completed' | 'failed';
  total_files: number;
  processed_files: number;
  successful_files: number;
  failed_files: number;
  progress_percent: number;
}> {
  return api.get(`${BASE_PATH}/jobs/${jobId}`);
}

/**
 * Bulk update file metadata
 */
export async function bulkUpdateFiles(
  fileIds: string[],
  updates: {
    folder_id?: string;
    tags?: { action: 'add' | 'remove' | 'set'; values: string[] };
  }
): Promise<{ success: boolean; updated_count: number }> {
  return api.put(`${BASE_PATH}/files/bulk-update`, {
    file_ids: fileIds,
    updates,
  });
}

/**
 * Bulk delete files
 */
export async function bulkDeleteFiles(fileIds: string[]): Promise<{
  success: boolean;
  deleted_count: number;
}> {
  return api.delete(`${BASE_PATH}/files/bulk-delete`, {
    data: { file_ids: fileIds },
  });
}

// ============================================================================
// Folder Operations
// ============================================================================

/**
 * List all folders (hierarchical tree)
 */
export async function listFolders(): Promise<{ folders: FolderMetadata[] }> {
  const response = await api.get<{ folders: any[] }>(`${BASE_PATH}/folders`);

  // Transform backend folder format to frontend format
  const folders = response.folders.map((folder) => ({
    folder_id: folder.id,
    name: folder.name,
    parent_folder_id: folder.parent_id ?? null, // Keep null for root folders
    created_at: folder.created_at,
    file_count: folder.file_count || 0,
    total_size_bytes: folder.total_size_bytes || 0,
    default_ontology_id: folder.default_ontology_id,
  }));

  return { folders };
}

/**
 * Create new folder
 */
export async function createFolder(request: FolderCreateRequest): Promise<FolderMetadata> {
  const response = await api.post<any>(`${BASE_PATH}/folders`, request);

  // Transform backend folder format to frontend format
  return {
    folder_id: response.id,
    name: response.name,
    parent_folder_id: response.parent_id ?? null, // Keep null for root folders
    created_at: response.created_at,
    file_count: response.file_count || 0,
    total_size_bytes: response.total_size_bytes || 0,
    default_ontology_id: response.default_ontology_id,
  };
}

/**
 * Update folder
 */
export async function updateFolder(
  folderId: string,
  updates: {
    name?: string;
    parent_folder_id?: string;
  }
): Promise<FolderMetadata> {
  const response = await api.put<any>(`${BASE_PATH}/folders/${folderId}`, updates);

  // Transform backend folder format to frontend format
  return {
    folder_id: response.id,
    name: response.name,
    parent_folder_id: response.parent_id ?? null, // Keep null for root folders
    created_at: response.created_at,
    file_count: response.file_count || 0,
    total_size_bytes: response.total_size_bytes || 0,
    default_ontology_id: response.default_ontology_id,
  };
}

/**
 * Delete folder
 */
export async function deleteFolder(folderId: string, force = false): Promise<void> {
  return api.delete(`${BASE_PATH}/folders/${folderId}`, {
    params: { force },
  });
}

// ============================================================================
// Tag Operations
// ============================================================================

/**
 * Get all tags with usage counts
 */
export async function listTags(): Promise<{ tags: TagStatistics[] }> {
  return api.get(`${BASE_PATH}/tags`);
}

// ============================================================================
// Search Operations
// ============================================================================

/**
 * Advanced search across files
 */
export async function searchFiles(query: {
  query: string;
  filters?: {
    folder_ids?: string[];
    tags?: string[];
    mime_types?: string[];
    date_range?: { from: string; to: string };
    min_size_bytes?: number;
    max_size_bytes?: number;
  };
  sort?: { field: string; order: 'asc' | 'desc' };
  limit?: number;
  offset?: number;
}): Promise<FileListResponse & {
  facets?: {
    tags: Record<string, number>;
    folders: Record<string, number>;
    mime_types: Record<string, number>;
  };
}> {
  return api.post(`${BASE_PATH}/search`, query);
}

// ============================================================================
// Statistics Operations
// ============================================================================

/**
 * Get library-wide statistics
 */
export async function getLibraryStats(): Promise<FileLibraryStats> {
  const response = await api.get<any>(`${BASE_PATH}/stats`);
  console.log('[fileLibrary] Raw stats response:', response);

  // Ensure all numeric fields are properly converted
  const ensureNumber = (val: any): number => {
    if (typeof val === 'number' && !isNaN(val)) return val;
    if (val === null || val === undefined) return 0;
    if (typeof val === 'object') return 0;
    const parsed = Number(val);
    return isNaN(parsed) ? 0 : parsed;
  };

  // Handle recent_uploads which might be an array or a number
  let recentUploadsCount = 0;
  if (Array.isArray(response.recent_uploads)) {
    recentUploadsCount = response.recent_uploads.length;
  } else if (typeof response.recent_uploads === 'number') {
    recentUploadsCount = response.recent_uploads;
  } else if (response.uploads_24h !== undefined) {
    recentUploadsCount = ensureNumber(response.uploads_24h);
  }

  const transformed = {
    total_files: ensureNumber(response.total_files),
    total_size_bytes: ensureNumber(response.total_size_bytes),
    folder_count: ensureNumber(response.folder_count || response.folders_count),
    recent_uploads: recentUploadsCount,
  };

  console.log('[fileLibrary] Transformed stats:', transformed);
  return transformed as FileLibraryStats;
}

/**
 * Get file usage statistics
 */
export async function getFileUsageStats(fileId: string): Promise<{
  file_id: string;
  times_used: number;
  workflows_count: number;
  last_accessed: string;
  access_count_30d: number;
  top_users: Array<{ user: string; count: number }>;
}> {
  return api.get(`${BASE_PATH}/files/${fileId}/usage-stats`);
}

// ============================================================================
// Lineage Operations
// ============================================================================

/**
 * Get lineage graph for file
 */
export async function getFileLineage(fileId: string): Promise<{
  file_id: string;
  downstream: Array<{
    type: 'workflow' | 'dataset' | 'model';
    id: string;
    name: string;
    status?: string;
  }>;
  upstream: Array<{
    type: 'datasource' | 'workflow';
    id: string;
    name: string;
  }>;
}> {
  return api.get(`${BASE_PATH}/files/${fileId}/lineage`);
}

/**
 * Analyze impact of deleting/modifying file
 */
export async function getFileImpactAnalysis(fileId: string): Promise<{
  can_delete: boolean;
  can_modify: boolean;
  impact: {
    workflows_affected: number;
    datasets_affected: number;
    critical_workflows: string[];
    recommendations: string[];
  };
}> {
  return api.get(`${BASE_PATH}/files/${fileId}/impact-analysis`);
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format file size for display
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${units[i]}`;
}

/**
 * Get file icon based on MIME type
 */
export function getFileIcon(mimeType: string): string {
  if (mimeType.startsWith('image/')) return '🖼️';
  if (mimeType.startsWith('video/')) return '🎥';
  if (mimeType.startsWith('audio/')) return '🎵';
  if (mimeType.includes('pdf')) return '📕';
  if (mimeType.includes('spreadsheet') || mimeType.includes('excel')) return '📊';
  if (mimeType.includes('document') || mimeType.includes('word')) return '📄';
  if (mimeType.includes('zip') || mimeType.includes('archive')) return '📦';
  if (mimeType.includes('json') || mimeType.includes('xml')) return '📋';
  if (mimeType.includes('csv') || mimeType.includes('tsv')) return '📈';

  return '📁';
}

// ============================================================================
// Schema Helper Functions
// ============================================================================

/**
 * Get schema from FileMetadata, handling both old and new field names
 * Provides backward compatibility during migration
 */
export function getFileSchema(file: FileMetadata): { fields: any[]; total_rows: number } | null {
  // Prefer new 'schema' field
  if (file.schema) {
    return {
      fields: file.schema.fields,
      total_rows: file.schema.total_rows
    };
  }

  // Fallback to deprecated 'inferred_schema' field
  if (file.inferred_schema) {
    return {
      fields: file.inferred_schema.columns,
      total_rows: file.inferred_schema.row_count
    };
  }

  return null;
}

/**
 * Check if file has a schema (either format)
 */
export function hasFileSchema(file: FileMetadata): boolean {
  return !!(file.schema || file.inferred_schema);
}

/**
 * Get field count from file schema
 */
export function getSchemaFieldCount(file: FileMetadata): number {
  const schema = getFileSchema(file);
  return schema?.fields?.length || 0;
}

// ============================================================================
// Schema Scanning/Profiling Operations (QW2, QW5)
// ============================================================================

/**
 * Scan parameters for file schema detection
 */
export interface FileScanParams {
  delimiter?: string;
  encoding?: string;
  has_header?: boolean;
  sample_rows?: number;
  auto_save?: boolean; // ✨ Auto-persist schema to file
  map_to_ontology?: boolean; // ✨ Enable ontology mapping
  ontology_id?: string; // ✨ Specific ontology to use for mapping
}

/**
 * Result from file scanning
 */
export interface ScanResult {
  detected_fields: Array<{
    name: string;
    type: string;
    nullable: boolean;
    sample_values: string[];
    is_pii?: boolean;
    pii_type?: string;
  }>;
  total_rows?: number;
  estimated_rows?: number;
  delimiter_detected?: string;
  encoding_detected?: string;
  has_header_detected?: boolean;
  scan_timestamp: string;
  warnings: string[];
  errors: string[];
  ontology_mappings?: FieldOntologyMapping[];
}

/**
 * Scan a single file to infer schema
 * QW2: One-click profiling with auto-save
 *
 * Backend endpoint: POST /file-library/files/{fileId}/scan
 *
 * Workflow:
 * 1. Call scan with auto_save: true
 * 2. Backend persists schema to file
 * 3. Refetch file to get updated schema
 */
export async function profileFile(
  fileId: string,
  params?: FileScanParams
): Promise<FileMetadata> {
  // Step 1: Scan file with auto_save enabled
  await api.post<ScanResult>(
    `${BASE_PATH}/files/${fileId}/scan`,
    { ...params, auto_save: true }
  );

  // Step 2: Refetch file to get persisted schema
  const updatedFile = await getFileMetadata(fileId);

  return updatedFile;
}

/**
 * Bulk scan job status (backend serializes to lowercase)
 */
export type BulkScanJobStatus = 'processing' | 'completed' | 'partial' | 'failed';

/**
 * Individual file scan result in bulk job
 */
export interface BulkScanFileResult {
  file_name: string;
  file_id?: string;
  status: 'success' | 'error' | 'warning'; // Lowercase from backend
  error?: string;
  scan_result?: ScanResult;
}

/**
 * Bulk scan job response
 */
export interface BulkScanJobResponse {
  job_id: string;
  status: BulkScanJobStatus;
  total_files: number;
  processed_files: number;
  successful_files: number;
  failed_files: number;
  progress_percent: number;
  results: BulkScanFileResult[];
  started_at: string;
  completed_at?: string;
  duration_ms?: number;
}

/**
 * Scan multiple files in bulk
 * QW5: Bulk profiling operation with auto-save
 *
 * Backend endpoint: POST /file-library/files/bulk-scan
 */
export async function bulkProfileFiles(
  fileIds: string[],
  params?: FileScanParams
): Promise<{
  job_id: string;
  status: BulkScanJobStatus;
  total_files: number;
}> {
  return api.post(`${BASE_PATH}/files/bulk-scan`, {
    file_ids: fileIds,
    auto_save: true, // ✨ Auto-persist schemas to files
    map_to_ontology: params?.map_to_ontology,
    ontology_id: params?.ontology_id,
    sample_rows: params?.sample_rows,
  });
}

/**
 * Get profiling/scanning job status
 *
 * Backend endpoint: GET /file-library/scan-jobs/{jobId}
 * (Also works: GET /file-library/jobs/{jobId})
 */
export async function getProfilingJobStatus(jobId: string): Promise<BulkScanJobResponse> {
  return api.get(`${BASE_PATH}/scan-jobs/${jobId}`);
}

// ============================================================================
// File-to-Datasource Integration Operations
// ============================================================================

/**
 * Register file as a datasource
 * Creates a datasource entry for a file in the library
 */
export async function registerFileAsDatasource(
  fileId: string,
  request: RegisterFileAsDatasourceRequest
): Promise<RegisterFileAsDatasourceResponse> {
  return api.post<RegisterFileAsDatasourceResponse>(
    `${BASE_PATH}/files/${fileId}/register-datasource`,
    request
  );
}

/**
 * Check if file can be registered as datasource (validation)
 * Performs schema inference and checks compatibility
 */
export async function validateFileForRegistration(
  fileId: string
): Promise<ValidateFileForRegistrationResponse> {
  return api.get<ValidateFileForRegistrationResponse>(
    `${BASE_PATH}/files/${fileId}/validate-registration`
  );
}
