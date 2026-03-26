/**
 * Data Catalogue API Client
 *
 * Provides a unified view of all data sources:
 * - Files from File Library
 * - External Datasource connections
 *
 * This is the Phase 1 unified API layer.
 */

import { api } from './client';
import {
  bulkDeleteFiles,
  deleteFile,
  downloadFile,
  listFiles,
} from './fileLibrary';
import { deleteDatasource, getDatasource, getDatasources } from './datasources';
import type { FileMetadata, Datasource } from './types';

// ============================================================================
// Unified Data Source Type
// ============================================================================

/**
 * Source type discriminator
 */
export type DataSourceType = 'file' | 'datasource';

/**
 * Unified representation of any data source (file or datasource)
 */
export interface UnifiedDataSource {
  // Core identification
  id: string;
  name: string;
  type: DataSourceType;

  // Metadata
  description?: string;
  size_bytes?: number;
  created_at: string;
  updated_at?: string;
  tags: string[];

  // Status & Health
  status: 'active' | 'inactive' | 'error' | 'registered' | 'unregistered';
  connection_status?: 'Connected' | 'Disconnected' | 'Connecting' | 'Unverified' | 'Error';

  // Classification
  datasource_category?: string; // 'Relational', 'Document', 'ObjectStorage', etc.
  mime_type?: string; // For files: 'text/csv', 'application/json', etc.
  file_type?: string; // Human-readable: 'CSV', 'Excel', 'JSON', etc.

  // Access & Usage
  access_count?: number;
  last_accessed_at?: string;

  // Origin metadata
  uploaded_by?: string; // For files
  plugin_name?: string; // For datasources

  // Schema information
  has_schema?: boolean;
  schema_info?: {
    row_count?: number;
    column_count?: number;
    table_count?: number;
    columns?: string[]; // Column names array
  };

  // Integration fields
  datasource_id?: string; // If file is registered as datasource
  file_id?: string; // If datasource is file-based

  // Custom metadata
  custom_metadata?: Record<string, any>;
}

/**
 * Statistics for the unified data catalogue
 */
export interface DataCatalogueStats {
  total_sources: number;
  files: number;
  datasources: number;
  recent_additions: number; // Last 24 hours
  by_type: Record<string, number>;
  total_size_bytes: number;
}

/**
 * Filters for listing unified sources
 */
export interface UnifiedSourceFilters {
  type?: DataSourceType; // Filter by file or datasource
  tags?: string[];
  status?: string;
  search?: string; // Search in name and description
  mime_type?: string; // For files
  datasource_category?: string; // For datasources
  sort_by?: 'name' | 'created_at' | 'size_bytes' | 'access_count';
  sort_order?: 'asc' | 'desc';
  limit?: number;
  offset?: number;
}

// ============================================================================
// Transformation Functions
// ============================================================================

/**
 * Transform FileMetadata to UnifiedDataSource
 */
function fileToUnifiedSource(file: FileMetadata): UnifiedDataSource {
  // Determine registration status
  let status: UnifiedDataSource['status'] = 'active';
  if (file.registration_status === 'registered') {
    status = 'registered';
  } else if (file.registration_status === 'error') {
    status = 'error';
  } else {
    status = 'unregistered';
  }

  // Extract file extension for classification
  const extension = file.original_filename?.split('.').pop()?.toUpperCase() || 'FILE';

  // Map MIME type to human-readable file type
  const getFileType = (mimeType: string): string => {
    if (mimeType.includes('csv')) return 'CSV';
    if (mimeType.includes('excel') || mimeType.includes('spreadsheet')) return 'Excel';
    if (mimeType.includes('json')) return 'JSON';
    if (mimeType.includes('xml')) return 'XML';
    if (mimeType.includes('parquet')) return 'Parquet';
    if (mimeType.includes('pdf')) return 'PDF';
    return extension;
  };

  return {
    id: file.file_id,
    name: file.original_filename || file.filename,
    type: 'file',
    size_bytes: file.size_bytes,
    created_at: file.uploaded_at,
    tags: file.tags || [],
    status,
    mime_type: file.mime_type,
    file_type: getFileType(file.mime_type),
    access_count: file.access_count,
    last_accessed_at: file.last_accessed_at,
    uploaded_by: file.uploaded_by,
    datasource_id: file.datasource_id,
    has_schema: !!file.inferred_schema,
    schema_info: file.inferred_schema ? {
      row_count: file.inferred_schema.row_count,
      column_count: file.inferred_schema.column_count,
      columns: file.inferred_schema.columns?.map(col => col.name) || [],
    } : undefined,
    custom_metadata: file.custom_metadata,
  };
}

/**
 * Transform Datasource to UnifiedDataSource
 */
function datasourceToUnifiedSource(datasource: Datasource): UnifiedDataSource {
  // Map connection status to unified status
  let status: UnifiedDataSource['status'] = 'active';
  let connectionStatus: UnifiedDataSource['connection_status'] = 'Disconnected';

  if (typeof datasource.status === 'string') {
    connectionStatus = datasource.status as UnifiedDataSource['connection_status'];
    if (datasource.status === 'Connected') {
      status = 'active';
    } else if (datasource.status === 'Unverified') {
      status = 'registered';
    } else {
      status = 'inactive';
    }
  } else if (datasource.status && typeof datasource.status === 'object') {
    if ('Error' in datasource.status) {
      status = 'error';
      connectionStatus = 'Error';
    } else if ('Degraded' in datasource.status) {
      status = 'active';
      connectionStatus = 'Error';
    }
  }

  // Extract datasource category
  let category = 'Unknown';
  if (typeof datasource.metadata.datasource_type === 'string') {
    category = datasource.metadata.datasource_type;
  } else if (datasource.metadata.datasource_type && typeof datasource.metadata.datasource_type === 'object') {
    if ('Custom' in datasource.metadata.datasource_type) {
      category = (datasource.metadata.datasource_type as any).Custom;
    }
  }

  return {
    id: datasource.id,
    name: datasource.name,
    type: 'datasource',
    description: datasource.metadata.description,
    created_at: datasource.created_at,
    updated_at: datasource.updated_at,
    tags: [], // Datasources don't have tags yet
    status,
    connection_status: connectionStatus,
    datasource_category: category,
    plugin_name: datasource.plugin_name,
    file_id: datasource.file_id,
    has_schema: datasource.capabilities.profiling,
    custom_metadata: {
      version: datasource.version,
      capabilities: datasource.capabilities,
    },
  };
}

// ============================================================================
// API Functions
// ============================================================================

/**
 * Get unified list of all data sources (files + datasources)
 *
 * This is the primary function for the Data Catalogue view.
 * It fetches from both APIs and merges the results.
 */
export async function listUnifiedSources(
  filters?: UnifiedSourceFilters
): Promise<{ sources: UnifiedDataSource[]; total: number }> {
  try {
    // Fetch from both APIs in parallel
    const [filesResponse, datasources] = await Promise.all([
      listFiles({
        tags: filters?.tags,
        mime_type: filters?.mime_type,
        search: filters?.search,
        page: filters?.offset ? Math.floor(filters.offset / (filters.limit || 20)) + 1 : 1,
        page_size: filters?.limit || 100,
        sort_by: filters?.sort_by === 'name' ? 'filename' : filters?.sort_by as any,
        sort_order: filters?.sort_order,
      }),
      getDatasources(),
    ]);

    console.log('[dataCatalogue] Raw responses:', {
      files: filesResponse.files.length,
      datasources: datasources.length,
    });

    // Transform to unified format
    let unifiedSources: UnifiedDataSource[] = [];

    // Add files if not filtering for datasources only
    if (!filters?.type || filters.type === 'file') {
      const filesSources = filesResponse.files.map(fileToUnifiedSource);
      unifiedSources = [...unifiedSources, ...filesSources];
    }

    // Add datasources if not filtering for files only
    if (!filters?.type || filters.type === 'datasource') {
      const datasourceSources = datasources.map(datasourceToUnifiedSource);
      unifiedSources = [...unifiedSources, ...datasourceSources];
    }

    // Apply additional filters
    let filteredSources = unifiedSources;

    if (filters?.status) {
      filteredSources = filteredSources.filter(s => s.status === filters.status);
    }

    if (filters?.datasource_category) {
      filteredSources = filteredSources.filter(
        s => s.datasource_category === filters.datasource_category
      );
    }

    if (filters?.search) {
      const query = filters.search.toLowerCase();
      filteredSources = filteredSources.filter(s =>
        s.name.toLowerCase().includes(query) ||
        s.description?.toLowerCase().includes(query)
      );
    }

    // Apply sorting
    if (filters?.sort_by) {
      const field = filters.sort_by;
      const order = filters.sort_order === 'desc' ? -1 : 1;

      filteredSources.sort((a, b) => {
        const aVal = a[field] || '';
        const bVal = b[field] || '';

        if (typeof aVal === 'number' && typeof bVal === 'number') {
          return (aVal - bVal) * order;
        }

        return String(aVal).localeCompare(String(bVal)) * order;
      });
    }

    console.log('[dataCatalogue] Unified sources:', filteredSources.length);

    return {
      sources: filteredSources,
      total: filteredSources.length,
    };
  } catch (error) {
    console.error('[dataCatalogue] Error fetching unified sources:', error);
    throw error;
  }
}

/**
 * Get statistics for the unified data catalogue
 */
export async function getDataCatalogueStats(): Promise<DataCatalogueStats> {
  try {
    // Fetch both to get counts
    const [filesResponse, datasources] = await Promise.all([
      listFiles({ page_size: 1 }), // Just need the total
      getDatasources(),
    ]);

    const fileCount = filesResponse.total || 0;
    const datasourceCount = datasources.length || 0;

    // Count by type
    const byType: Record<string, number> = {};

    // Count files by MIME type
    const allFiles = await listFiles({ page_size: 1000 }); // Get all files for type counting
    allFiles.files.forEach(file => {
      const fileType = file.mime_type.split('/')[0] || 'other';
      byType[fileType] = (byType[fileType] || 0) + 1;
    });

    // Count datasources by category
    datasources.forEach(ds => {
      const category = typeof ds.metadata.datasource_type === 'string'
        ? ds.metadata.datasource_type
        : 'Custom';
      byType[category] = (byType[category] || 0) + 1;
    });

    // Calculate total size (files only for now)
    const totalSize = allFiles.files.reduce((sum, f) => sum + (f.size_bytes || 0), 0);

    // Recent additions (last 24 hours)
    const now = new Date();
    const yesterday = new Date(now.getTime() - 24 * 60 * 60 * 1000);

    const recentFiles = allFiles.files.filter(f =>
      new Date(f.uploaded_at) > yesterday
    ).length;

    const recentDatasources = datasources.filter(ds =>
      new Date(ds.created_at) > yesterday
    ).length;

    return {
      total_sources: fileCount + datasourceCount,
      files: fileCount,
      datasources: datasourceCount,
      recent_additions: recentFiles + recentDatasources,
      by_type: byType,
      total_size_bytes: totalSize,
    };
  } catch (error) {
    console.error('[dataCatalogue] Error fetching stats:', error);
    // Return empty stats on error
    return {
      total_sources: 0,
      files: 0,
      datasources: 0,
      recent_additions: 0,
      by_type: {},
      total_size_bytes: 0,
    };
  }
}

/**
 * Get a single unified data source by ID
 * Automatically determines if it's a file or datasource based on ID format
 */
export async function getUnifiedSource(id: string): Promise<UnifiedDataSource | null> {
  try {
    // Try file first (file IDs typically start with 'file_')
    if (id.startsWith('file_')) {
      const response = await listFiles({ page_size: 1000 });
      const file = response.files.find(f => f.file_id === id);
      if (file) {
        return fileToUnifiedSource(file);
      }
    }

    // Try datasource
    try {
      const datasource = await getDatasource(id);
      return datasourceToUnifiedSource(datasource);
    } catch {
      // Not a datasource
    }

    // Not found
    return null;
  } catch (error) {
    console.error('[dataCatalogue] Error fetching source:', error);
    return null;
  }
}

// ============================================================================
// Mutation Functions
// ============================================================================

/**
 * Delete unified sources (files and/or datasources)
 * Handles deletion based on source type
 */
export async function deleteUnifiedSources(
  sources: UnifiedDataSource[]
): Promise<{ success: boolean; deleted: number; failed: number; errors: string[] }> {
  const results = {
    success: true,
    deleted: 0,
    failed: 0,
    errors: [] as string[],
  };

  // Separate files and datasources
  const files = sources.filter(s => s.type === 'file');
  const datasources = sources.filter(s => s.type === 'datasource');

  // Delete files (use bulk delete if available)
  if (files.length > 0) {
    try {
      if (files.length === 1) {
        // Single delete
        await deleteFile(files[0].id);
        results.deleted++;
      } else {
        // Bulk delete
        const fileIds = files.map(f => f.id);
        const response = await bulkDeleteFiles(fileIds);
        results.deleted += response.deleted_count;
        if (response.deleted_count < fileIds.length) {
          results.failed += fileIds.length - response.deleted_count;
          results.errors.push(`Failed to delete ${fileIds.length - response.deleted_count} file(s)`);
        }
      }
    } catch (error) {
      results.success = false;
      results.failed += files.length;
      results.errors.push(`File deletion error: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  }

  // Delete datasources (one by one, no bulk API)
  for (const datasource of datasources) {
    try {
      await deleteDatasource(datasource.id);
      results.deleted++;
    } catch (error) {
      results.success = false;
      results.failed++;
      results.errors.push(
        `Failed to delete datasource "${datasource.name}": ${
          error instanceof Error ? error.message : 'Unknown error'
        }`
      );
    }
  }

  return results;
}

/**
 * Download a file source
 * Only works for file type sources
 */
export async function downloadUnifiedSource(source: UnifiedDataSource): Promise<string | null> {
  if (source.type !== 'file') {
    throw new Error('Only file sources can be downloaded');
  }

  try {
    return await downloadFile(source.id);
  } catch (error) {
    console.error('[dataCatalogue] Download error:', error);
    throw error;
  }
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Format bytes to human-readable size
 */
export function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B';

  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${units[i]}`;
}

/**
 * Get icon for data source type
 */
export function getSourceIcon(source: UnifiedDataSource): string {
  if (source.type === 'file') {
    // Use file type icons
    if (source.mime_type?.includes('csv')) return '📈';
    if (source.mime_type?.includes('excel') || source.mime_type?.includes('spreadsheet')) return '📊';
    if (source.mime_type?.includes('json')) return '📋';
    if (source.mime_type?.includes('xml')) return '📄';
    if (source.mime_type?.includes('pdf')) return '📕';
    if (source.mime_type?.includes('image')) return '🖼️';
    return '📁';
  } else {
    // Use datasource category icons
    const category = source.datasource_category?.toLowerCase() || '';
    if (category.includes('relational')) return '🗄️';
    if (category.includes('document')) return '📚';
    if (category.includes('search')) return '🔍';
    if (category.includes('storage')) return '💾';
    if (category.includes('stream')) return '🌊';
    if (category.includes('graph')) return '🕸️';
    return '🔌';
  }
}

/**
 * Get status badge color
 */
export function getStatusColor(status: UnifiedDataSource['status']): string {
  switch (status) {
    case 'active':
    case 'registered':
      return 'green';
    case 'inactive':
      return 'gray';
    case 'error':
      return 'red';
    case 'unregistered':
      return 'yellow';
    default:
      return 'gray';
  }
}
