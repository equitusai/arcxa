/**
 * Backend API Adapter
 *
 * Centralized transformation layer between frontend and backend API contracts.
 * This provides:
 * - Type safety for API boundary
 * - Single source of truth for field name mappings
 * - Testable transformation functions
 * - Easy maintenance when backend contracts change
 */

// ============================================================================
// Backend Contract Types
// ============================================================================

/**
 * Backend request/response types match the Rust backend DTOs exactly.
 * These are prefixed with "Backend" to distinguish from frontend types.
 */

export interface BackendDatasourceImportRequest {
  source_id: string;
  table: string;
  schema?: string;
  name?: string;
  where_clause?: string;
  columns?: string[];
  limit?: number;
  description?: string;
  tags?: string[];
  profile?: boolean;
  async_mode?: boolean;
}

export interface BackendBatchTableImport {
  table: string;
  schema?: string;
  name?: string;
  where_clause?: string;
  columns?: string[];
  limit?: number;
}

export interface BackendBatchDatasourceImportRequest {
  source_id: string;
  tables: BackendBatchTableImport[];
  tags?: string[];
  profile?: boolean;
}

// ============================================================================
// Status Transformations
// ============================================================================

/**
 * Status mapping configuration
 * Maps backend PascalCase to frontend snake_case
 */
const STATUS_MAPPINGS = {
  // Mapping session statuses
  'Draft': 'draft',
  'PendingReview': 'pending_review',
  'Approved': 'approved',
  'Applied': 'applied',
  'Active': 'active',

  // Field approval statuses
  'Pending': 'pending',
  'AutoApproved': 'auto_approved',
  'Rejected': 'rejected',
  'Modified': 'modified',

  // Import statuses
  'Processing': 'processing',
  'Imported': 'imported',
  'Failed': 'failed',
} as const;

/**
 * Transform backend status string to frontend format
 *
 * @param backendStatus - PascalCase status from backend
 * @returns snake_case status for frontend
 */
export function normalizeStatus(backendStatus: string): string {
  // Check predefined mapping first
  if (backendStatus in STATUS_MAPPINGS) {
    return STATUS_MAPPINGS[backendStatus as keyof typeof STATUS_MAPPINGS];
  }

  // Fallback: convert to snake_case
  return backendStatus
    .replace(/([A-Z])/g, '_$1')
    .toLowerCase()
    .replace(/^_/, '')
    .replace(/ /g, '_');
}

/**
 * Transform backend status to frontend (reverse)
 * Used when sending status filters to backend
 *
 * @param frontendStatus - snake_case status from frontend
 * @returns PascalCase status for backend
 */
export function denormalizeStatus(frontendStatus: string): string {
  // Reverse lookup in mapping
  const entry = Object.entries(STATUS_MAPPINGS).find(
    ([_, frontend]) => frontend === frontendStatus
  );

  if (entry) {
    return entry[0];
  }

  // Fallback: convert snake_case to PascalCase
  return frontendStatus
    .split('_')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join('');
}

// ============================================================================
// Import Transformations
// ============================================================================

/**
 * Transform frontend datasource import request to backend format
 *
 * Frontend uses more descriptive field names for clarity.
 * Backend uses concise field names matching Rust conventions.
 *
 * @example
 * // Frontend:
 * { datasource_id: "ds-1", table_name: "users", dataset_name: "User Data" }
 *
 * // Backend:
 * { source_id: "ds-1", table: "users", name: "User Data" }
 */
export function transformDatasourceImportRequest(
  frontendRequest: {
    datasource_id: string;
    table_name: string;
    schema?: string;
    dataset_name: string;
    description?: string;
    tags?: string[];
    profile?: boolean;
    async_mode?: boolean;
  }
): BackendDatasourceImportRequest {
  return {
    source_id: frontendRequest.datasource_id,
    table: frontendRequest.table_name,
    schema: frontendRequest.schema,
    name: frontendRequest.dataset_name,
    description: frontendRequest.description,
    tags: frontendRequest.tags,
    profile: frontendRequest.profile,
    async_mode: frontendRequest.async_mode,
  };
}

/**
 * Transform frontend batch import request to backend format
 */
export function transformBatchImportRequest(
  frontendRequest: {
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
): BackendBatchDatasourceImportRequest {
  return {
    source_id: frontendRequest.datasource_id,
    tables: frontendRequest.tables.map(table => ({
      table: table.table_name,
      schema: table.schema,
      name: table.dataset_name,
      // Note: Backend doesn't have description/tags per table in batch mode
    })),
    profile: frontendRequest.profile,
  };
}

// ============================================================================
// Schema Discovery Transformations
// ============================================================================

/**
 * Transform frontend schema discovery request to backend format
 *
 * Backend requires sourceId in request body even though it's in the URL path.
 * This is a quirk of the current backend implementation.
 */
export function transformSchemaDiscoveryRequest(
  datasourceId: string,
  options?: {
    table_name?: string | null;
    sample_size?: number;
  }
): {
  sourceId: string;
  tableName: string | null;
  sampleSize: number;
} {
  return {
    sourceId: datasourceId,
    tableName: options?.table_name ?? null,
    sampleSize: options?.sample_size ?? 100,
  };
}

// ============================================================================
// Response Transformations
// ============================================================================

/**
 * Transform backend nested response with status fields
 *
 * Recursively transforms all status fields in a response object.
 * This handles complex nested structures like mapping sessions.
 *
 * @param response - Backend response object
 * @param statusFields - Array of field names that contain status values
 * @returns Transformed response with normalized status fields
 */
export function transformResponseStatuses(
  response: any,
  statusFields: string[] = ['status', 'approval_status']
): any {
  const transformed = { ...response };

  for (const field of statusFields) {
    if (field in transformed && typeof transformed[field] === 'string') {
      transformed[field] = normalizeStatus(transformed[field]);
    }
  }

  // Recursively transform nested objects
  if (Array.isArray(transformed.tables)) {
    transformed.tables = transformed.tables.map((table: any) => {
      const transformedTable = { ...table };

      if (Array.isArray(transformedTable.field_mappings)) {
        transformedTable.field_mappings = transformedTable.field_mappings.map((fm: any) => ({
          ...fm,
          approval_status: fm.approval_status ? normalizeStatus(fm.approval_status) : fm.approval_status,
        }));
      }

      return transformedTable;
    });
  }

  return transformed;
}

// ============================================================================
// Validation Utilities
// ============================================================================

/**
 * Validate that required fields are present in request
 *
 * @throws {Error} if validation fails
 */
export function validateImportRequest(request: {
  datasource_id?: string;
  table_name?: string;
  dataset_name?: string;
}): void {
  const errors: string[] = [];

  if (!request.datasource_id?.trim()) {
    errors.push('datasource_id is required');
  }

  if (!request.table_name?.trim()) {
    errors.push('table_name is required');
  }

  if (!request.dataset_name?.trim()) {
    errors.push('dataset_name is required');
  }

  if (errors.length > 0) {
    throw new Error(`Validation failed: ${errors.join(', ')}`);
  }
}

// ============================================================================
// Error Handling
// ============================================================================

/**
 * Transform backend error response to user-friendly message
 */
export function transformErrorMessage(error: any): string {
  if (error.response?.data?.message) {
    return error.response.data.message;
  }

  if (error.response?.data?.error) {
    return error.response.data.error;
  }

  if (error.message) {
    return error.message;
  }

  return 'An unknown error occurred';
}

/**
 * Extract field-level errors from backend validation response
 */
export function extractFieldErrors(error: any): Record<string, string> {
  const fieldErrors: Record<string, string> = {};

  if (error.response?.data?.details) {
    const details = error.response.data.details;

    for (const [field, message] of Object.entries(details)) {
      if (typeof message === 'string') {
        fieldErrors[field] = message;
      }
    }
  }

  return fieldErrors;
}
