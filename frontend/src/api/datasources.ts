/**
 * Datasource/Connector API
 *
 * Handles datasource catalog and configuration management
 * Backend uses "connectors" terminology
 *
 * Uses the backend adapter layer for type-safe transformations.
 * @see src/api/adapters/backend-adapter.ts
 */

import { api } from './client';
import type {
  Datasource,
  RegisterDatasourceRequest,
  UpdateDatasourceRequest,
  TestConnectionResponse,
  DatasourceHealthResponse,
  SchemaInfo,
  AvailablePlugin,
  DatasourceStats,
  ConnectionStatus,
} from './types';
import { transformSchemaDiscoveryRequest } from './adapters/backend-adapter';

interface BackendConnector {
  name: string;
  source_type: string;
  version?: string;
  description?: string;
  capabilities?: {
    streaming?: boolean;
    max_batch_size?: number;
    parameterized_queries?: boolean;
    transactions?: boolean;
    schema_inference?: boolean;
  };
  required_credentials?: ConnectorFieldDefinition[];
  optional_config?: ConnectorFieldDefinition[];
}

interface ConnectorFieldDefinition {
  name: string;
  description?: string;
  field_type?: string;
  required?: boolean;
  sensitive?: boolean;
  default_value?: string | number | boolean;
}

interface ConnectorFieldSchema {
  type: 'string' | 'number' | 'boolean';
  label: string;
  required: boolean;
  placeholder?: string;
  description?: string;
  secret?: boolean;
  credential: boolean;
}

interface BackendDatasourceCapabilities {
  canTest: boolean;
  canInferSchema: boolean;
  canQuery: boolean;
  canReadWorkflow: boolean;
  canWriteWorkflow: boolean;
  supportsParameters: boolean;
  supportsTls: boolean;
  supportsIncremental: boolean;
  supportsCancellation: boolean;
}

interface BackendConnectionTestResult {
  success: boolean;
  durationMs: number;
  error?: string;
  metadata?: Record<string, unknown>;
  testedAt?: string;
}

interface BackendDatasourceResponse {
  '@id': string;
  title: string;
  description?: string;
  sourceType: string;
  connection?: {
    config?: Record<string, unknown>;
  };
  schemaRef?: string;
  tags?: string[];
  metadata?: Record<string, string>;
  createdAt?: string;
  updatedAt?: string;
  status: string;
  lastTestResult?: BackendConnectionTestResult;
  capabilities?: BackendDatasourceCapabilities;
}

interface BackendDatasourceListResponse {
  sources: BackendDatasourceResponse[];
  total: number;
}

interface BackendColumnDefinition {
  name: string;
  dataType: string;
  nullable: boolean;
  primaryKey?: boolean;
}

interface BackendTableDefinition {
  name: string;
  columns: BackendColumnDefinition[];
  estimatedRows?: number;
}

interface BackendSchemaDefinition {
  name: string;
  tables: BackendTableDefinition[];
  inferredAt: string;
}

interface BackendQueryResult {
  rows: Array<Record<string, unknown>>;
  rowCount: number;
  executionTimeMs: number;
  truncated: boolean;
  columns?: BackendColumnDefinition[];
}

export interface WorkflowSchemaField {
  name: string;
  type: string;
  nullable?: boolean;
  primary_key?: boolean;
}

export interface WorkflowTableSchema {
  name: string;
  columns: WorkflowSchemaField[];
  estimated_rows?: number;
}

export interface WorkflowDatasourceSchema {
  name: string;
  tables: WorkflowTableSchema[];
  inferred_at: string;
}

export interface DatasourceQueryPreview {
  rows: Array<Record<string, unknown>>;
  row_count: number;
  execution_time_ms: number;
  truncated: boolean;
  columns: WorkflowSchemaField[];
}

export type DatasourceOperation =
  | 'schemaInference'
  | 'query'
  | 'workflowRead'
  | 'workflowWrite';

async function getConnectorMap(): Promise<Map<string, BackendConnector>> {
  try {
    const response = await api.get<{ connectors: BackendConnector[] }>('/connectors');
    return new Map(
      (response.connectors || []).map((connector) => [connector.source_type, connector])
    );
  } catch {
    return new Map();
  }
}

/**
 * Get all registered datasources
 */
export async function getDatasources(): Promise<Datasource[]> {
  const [datasourcesResp, connectorMap] = await Promise.all([
    api.get<BackendDatasourceListResponse>('/datasources'),
    getConnectorMap(),
  ]);

  return (datasourcesResp.sources || []).map((source) =>
    mapDatasourceResponse(source, connectorMap.get(source.sourceType))
  );
}

/**
 * Map backend status to frontend ConnectionStatus
 */
function mapBackendStatusToConnectionStatus(
  status: string,
  lastTestResult?: BackendConnectionTestResult
): ConnectionStatus {
  if (status === 'testing') {
    return 'Connecting';
  }

  if (status === 'unverified') {
    return 'Unverified';
  }

  if (status === 'disabled') {
    return 'Disconnected';
  }

  if (status === 'error') {
    return { Error: lastTestResult?.error || 'Connection test failed' };
  }

  if (status === 'active' && lastTestResult?.success === false) {
    return { Error: lastTestResult.error || 'Connection test failed' };
  }

  if (status === 'active') {
    return 'Connected';
  }

  return 'Disconnected';
}

function getOperationCapability(
  datasource: Datasource,
  operation: DatasourceOperation
): boolean {
  const capabilities = datasource.instance_capabilities;
  switch (operation) {
    case 'schemaInference':
      return capabilities?.canInferSchema ?? false;
    case 'query':
      return capabilities?.canQuery ?? false;
    case 'workflowRead':
      return capabilities?.canReadWorkflow ?? false;
    case 'workflowWrite':
      return capabilities?.canWriteWorkflow ?? false;
    default:
      return false;
  }
}

function getOperationLabel(operation: DatasourceOperation): string {
  switch (operation) {
    case 'schemaInference':
      return 'infer schema';
    case 'query':
      return 'preview or query data';
    case 'workflowRead':
      return 'use this datasource as a workflow source';
    case 'workflowWrite':
      return 'use this datasource as a workflow target';
    default:
      return 'use this datasource';
  }
}

export function getDatasourceStatusLabel(status: ConnectionStatus): string {
  if (status === 'Connected') {
    return 'Connected';
  }
  if (status === 'Connecting') {
    return 'Connecting';
  }
  if (status === 'Disconnected') {
    return 'Disconnected';
  }
  if (status === 'Unverified') {
    return 'Unverified';
  }
  if (typeof status === 'object' && 'Degraded' in status) {
    return 'Degraded';
  }
  if (typeof status === 'object' && 'Error' in status) {
    return 'Error';
  }
  return 'Unknown';
}

export function isDatasourceReadyForOperation(
  datasource: Datasource,
  operation: DatasourceOperation
): boolean {
  if (!datasource.enabled) {
    return false;
  }

  if (datasource.status !== 'Connected') {
    return false;
  }

  return getOperationCapability(datasource, operation);
}

export function getDatasourceReadinessMessage(
  datasource: Datasource,
  operation: DatasourceOperation
): string {
  const action = getOperationLabel(operation);

  if (!datasource.enabled) {
    return `${datasource.name} is disabled. Re-enable it before you ${action}.`;
  }

  if (typeof datasource.status === 'object' && 'Error' in datasource.status) {
    return datasource.status.Error;
  }

  if (typeof datasource.status === 'object' && 'Degraded' in datasource.status) {
    return datasource.status.Degraded;
  }

  if (datasource.status === 'Unverified') {
    return `Run a successful connection test before you ${action}.`;
  }

  if (datasource.status === 'Connecting') {
    return `A connection test is still in progress for ${datasource.name}.`;
  }

  if (datasource.status === 'Disconnected') {
    return `${datasource.name} is not currently operational.`;
  }

  if (!getOperationCapability(datasource, operation)) {
    return `${datasource.name} is not currently enabled to ${action}.`;
  }

  return `${datasource.name} is ready to ${action}.`;
}

/**
 * Get datasource by ID
 */
export async function getDatasource(id: string): Promise<Datasource> {
  const [source, connectorMap] = await Promise.all([
    api.get<BackendDatasourceResponse>(`/datasources/${id}`),
    getConnectorMap(),
  ]);

  return mapDatasourceResponse(source, connectorMap.get(source.sourceType));
}

/**
 * Register a new datasource
 * Backend API format:
 * {
 *   "title": "postgresql-j6ds",
 *   "sourceType": "PostgreSQL",
 *   "connection": {
 *     "secretRef": "vault://datasources/postgresql-j6ds/credentials",
 *     "config": {
 *       "type": "PostgreSQL",
 *       "host": "localhost",
 *       "port": 5434,
 *       "database": "crm_db"
 *     },
 *     "encryptionEnabled": false
 *   }
 * }
 */
export async function registerDatasource(
  request: RegisterDatasourceRequest
): Promise<Datasource> {
  const [response, connectorMap] = await Promise.all([
    api.post<BackendDatasourceResponse>('/datasources', request),
    getConnectorMap(),
  ]);
  return mapDatasourceResponse(response, connectorMap.get(response.sourceType));
}

/**
 * Update datasource configuration
 */
export async function updateDatasource(
  id: string,
  request: UpdateDatasourceRequest
): Promise<Datasource> {
  const [response, connectorMap] = await Promise.all([
    api.put<BackendDatasourceResponse>(`/datasources/${id}`, request),
    getConnectorMap(),
  ]);
  return mapDatasourceResponse(response, connectorMap.get(response.sourceType));
}

/**
 * Delete datasource
 */
export async function deleteDatasource(id: string): Promise<void> {
  return api.delete(`/datasources/${id}`);
}

/**
 * Test datasource connection
 * IMPORTANT: Datasource must be registered first for security reasons.
 * Credentials are stored in vault, not passed in HTTP requests.
 * Backend endpoint: POST /api/v1/datasources/test with sourceId in body
 */
export async function testConnection(
  sourceId: string
): Promise<TestConnectionResponse> {
  const response = await api.post<BackendConnectionTestResult>('/datasources/test', {
    sourceId,
  });

  return {
    success: response.success,
    message:
      response.error || (response.success ? 'Connection successful' : 'Connection failed'),
    latency_ms: response.durationMs,
    metadata: response.metadata,
  };
}

/**
 * Get datasource health status
 * Note: Backend doesn't have a dedicated health endpoint, using test connection instead
 */
export async function getDatasourceHealth(
  id: string
): Promise<DatasourceHealthResponse> {
  try {
    const testResult = await testConnection(id);
    return {
      status: testResult.success ? 'Healthy' : 'Unhealthy',
      last_check: new Date().toISOString(),
      metrics: {
        latency_ms: testResult.latency_ms,
      },
      issues: testResult.success ? undefined : [testResult.message || 'Connection test failed'],
    };
  } catch (error) {
    return {
      status: 'Unhealthy',
      last_check: new Date().toISOString(),
      metrics: {},
      issues: [error instanceof Error ? error.message : 'Health check failed'],
    };
  }
}

/**
 * Get datasource schema information
 * Backend endpoint: POST /api/v1/datasources/:id/schema/infer
 *
 * @param id - Datasource identifier
 * @returns Schema information with tables and columns
 */
export async function getDatasourceSchema(id: string): Promise<SchemaInfo> {
  const request = transformSchemaDiscoveryRequest(id);
  return api.post(`/datasources/${id}/schema/infer`, request);
}

function mapWorkflowSchemaField(column: BackendColumnDefinition): WorkflowSchemaField {
  return {
    name: column.name,
    type: column.dataType,
    nullable: column.nullable,
    primary_key: column.primaryKey ?? false,
  };
}

export async function inferDatasourceSchemaForWorkflow(
  id: string,
  options?: { tableName?: string; sampleSize?: number }
): Promise<WorkflowDatasourceSchema> {
  const response = await api.post<BackendSchemaDefinition>(
    `/datasources/${id}/schema/infer`,
    {
      sourceId: id,
      tableName: options?.tableName,
      sampleSize: options?.sampleSize ?? 1000,
    }
  );

  return {
    name: response.name,
    inferred_at: response.inferredAt,
    tables: (response.tables || []).map((table) => ({
      name: table.name,
      estimated_rows: table.estimatedRows,
      columns: (table.columns || []).map(mapWorkflowSchemaField),
    })),
  };
}

export async function previewDatasourceQuery(
  id: string,
  request: {
    query: string;
    parameters?: Record<string, unknown>;
    limit?: number;
    timeout?: number;
  }
): Promise<DatasourceQueryPreview> {
  const response = await api.post<BackendQueryResult>(`/datasources/${id}/query`, {
    sourceId: id,
    query: request.query,
    parameters: request.parameters || {},
    limit: request.limit ?? 25,
    timeout: request.timeout ?? 30,
  });

  return {
    rows: response.rows || [],
    row_count: response.rowCount,
    execution_time_ms: response.executionTimeMs,
    truncated: response.truncated,
    columns: (response.columns || []).map(mapWorkflowSchemaField),
  };
}

/**
 * Get available connectors (from backend connector registry)
 */
export async function getAvailablePlugins(): Promise<AvailablePlugin[]> {
  const response = await api.get<{ connectors: BackendConnector[] }>('/connectors');

  const plugins = response.connectors.map((connector) => ({
    name: connector.name || getSourceDisplayName(connector.source_type),
    source_type: connector.source_type,
    version: connector.version || '1.0.0',
    description: connector.description || `${getSourceDisplayName(connector.source_type)} connector`,
    datasource_type: mapSourceTypeToCategory(connector.source_type),
    capabilities: mapConnectorCapabilities(connector),
    config_schema: buildConfigSchema(connector),
  }));

  // ✅ Add EDB (EnterpriseDB) as a visual variant of PostgreSQL
  const postgresConnector = response.connectors.find(
    (connector) =>
      connector.source_type === 'PostgreSQL' || connector.name === 'PostgreSQL'
  );

  if (postgresConnector) {
    plugins.push({
      name: 'EnterpriseDB',
      source_type: 'PostgreSQL',
      version: postgresConnector.version || '1.0.0',
      description: 'PostgreSQL-based database with enterprise features and Oracle compatibility',
      datasource_type: 'Relational',
      capabilities: mapConnectorCapabilities(postgresConnector),
      config_schema: buildConfigSchema(postgresConnector),
    });
  }

  return plugins;
}

/**
 * Get datasource statistics (from actual datasources, not connector types)
 */
export async function getDatasourceStats(): Promise<DatasourceStats> {
  const response = await api.get<BackendDatasourceListResponse>('/datasources');

  const sources = response.sources || [];
  const connected = sources.filter(
    (source) => source.status === 'active' && source.lastTestResult?.success !== false
  ).length;
  const disconnected = sources.filter(
    (source) => source.status === 'disabled' || source.status === 'unverified'
  ).length;
  const errors = sources.filter(
    (source) => source.status === 'error' || source.lastTestResult?.success === false
  ).length;

  const by_type: Record<string, number> = {};
  sources.forEach((source) => {
    const type = getSourceDisplayName(source.sourceType || 'Unknown');
    by_type[type] = (by_type[type] || 0) + 1;
  });

  return {
    total_datasources: response.total || 0,
    connected,
    disconnected,
    errors,
    by_type,
  };
}

/**
 * Per-datasource enable/disable is not currently supported by the coordinator API.
 */
export async function enableDatasource(id: string): Promise<Datasource> {
  throw new Error(
    `Datasource '${id}' cannot be enabled individually. Coordinator only exposes connector-registry enable/disable operations.`
  );
}

/**
 * Per-datasource enable/disable is not currently supported by the coordinator API.
 */
export async function disableDatasource(id: string): Promise<Datasource> {
  throw new Error(
    `Datasource '${id}' cannot be disabled individually. Coordinator only exposes connector-registry enable/disable operations.`
  );
}

/**
 * Refresh datasource schema
 * Backend endpoint: POST /api/v1/datasources/:id/schema/infer-enhanced
 *
 * @param id - Datasource identifier
 * @returns Enhanced schema information
 */
export async function refreshSchema(id: string): Promise<SchemaInfo> {
  const request = transformSchemaDiscoveryRequest(id);
  const response = await api.post(`/datasources/${id}/schema/infer-enhanced`, request);
  // Backend returns enhanced response with schema nested inside
  return response.schema || response;
}

// Helper functions

/**
 * Map frontend plugin display name to backend enum type
 * This is needed because the UI shows user-friendly names like "IBM DB2"
 * but the backend expects the enum variant name like "DB2"
 */
export function mapPluginNameToBackendType(pluginName: string): string {
  const mapping: Record<string, string> = {
    'IBM DB2': 'DB2',
    'EnterpriseDB': 'PostgreSQL',
    'EDB': 'PostgreSQL',
    'SAP HANA': 'SAPHANA',
    'S3 Parquet': 'S3Parquet',
    'CSV File': 'CsvFile',
    'RDF N-Triples': 'RDFNTriples',
  };
  return mapping[pluginName] || pluginName;
}

function mapSourceTypeToCategory(sourceType: string): import('./types').DatasourceType {
  const mapping: Record<string, import('./types').DatasourceType> = {
    'PostgreSQL': 'Relational',
    'EnterpriseDB': 'Relational',
    'EDB': 'Relational',
    'MySQL': 'Relational',
    'Snowflake': 'Relational',
    'Oracle': 'Relational',
    'IBM DB2': 'Relational',
    'DB2': 'Relational',
    'SAPHANA': 'Relational',
    'SAP HANA': 'Relational',
    'Databricks': 'Relational',
    'S3Parquet': 'ObjectStorage',
    'S3 Parquet': 'ObjectStorage',
    'CsvFile': { Custom: 'CSV File' },
    'CSV File': { Custom: 'CSV File' },
    'RDFNTriples': 'Graph',
    'RDF N-Triples': 'Graph',
  };
  return mapping[sourceType] || { Custom: sourceType };
}

function getSourceDisplayName(sourceType: string): string {
  const names: Record<string, string> = {
    DB2: 'IBM DB2',
    SAPHANA: 'SAP HANA',
    S3Parquet: 'S3 Parquet',
    CsvFile: 'CSV File',
    RDFNTriples: 'RDF N-Triples',
  };
  return names[sourceType] || sourceType;
}

function mapConnectorCapabilities(
  connector?: BackendConnector,
  instanceCapabilities?: BackendDatasourceCapabilities
): import('./types').PluginCapabilities {
  const maxBatchSize = connector?.capabilities?.max_batch_size || 0;
  const supportsTransactions =
    connector?.capabilities?.transactions || instanceCapabilities?.canWriteWorkflow || false;
  const supportsSchemaInference =
    connector?.capabilities?.schema_inference || instanceCapabilities?.canInferSchema || false;

  return {
    cdc: connector?.capabilities?.streaming || instanceCapabilities?.supportsIncremental || false,
    batch_read:
      maxBatchSize > 0 ||
      connector?.capabilities?.parameterized_queries ||
      instanceCapabilities?.canQuery ||
      false,
    batch_write: supportsTransactions,
    profiling: supportsSchemaInference,
    lineage_discovery: false,
    schema_evolution: false,
    transactions: supportsTransactions,
  };
}

function mapDatasourceResponse(
  source: BackendDatasourceResponse,
  connector?: BackendConnector
): Datasource {
  return {
    id: source['@id'],
    name: source.title,
    plugin_name: connector?.name || getSourceDisplayName(source.sourceType),
    source_type: source.sourceType,
    version: connector?.version || '1.0.0',
    enabled: source.status !== 'disabled',
    description: source.description,
    tags: source.tags || [],
    metadata: {
      name: connector?.name || getSourceDisplayName(source.sourceType),
      version: connector?.version || '1.0.0',
      author: 'Graphica',
      description:
        source.description ||
        connector?.description ||
        `${getSourceDisplayName(source.sourceType)} datasource`,
      datasource_type: mapSourceTypeToCategory(source.sourceType),
    },
    capabilities: mapConnectorCapabilities(connector, source.capabilities),
    instance_capabilities: source.capabilities,
    status: mapBackendStatusToConnectionStatus(source.status, source.lastTestResult),
    config: {
      connection: source.connection?.config || {},
      cdc: source.capabilities?.supportsIncremental ? { enabled: true } : undefined,
    },
    created_at: source.createdAt || new Date().toISOString(),
    updated_at: source.updatedAt || source.createdAt || new Date().toISOString(),
  };
}

function buildConfigSchema(connector: BackendConnector): Record<string, ConnectorFieldSchema> {
  const schema: Record<string, ConnectorFieldSchema> = {};

  // Map backend FieldType to frontend input types
  const mapFieldType = (
    backendType?: string
  ): ConnectorFieldSchema['type'] => {
    const type = backendType?.toLowerCase() || 'string';
    // Map backend types to frontend input types
    switch (type) {
      case 'integer':
      case 'port':
        return 'number';
      case 'boolean':
        return 'boolean';
      case 'string':
      case 'url':
      case 'hostname':
      case 'filepath':
      default:
        return 'string';
    }
  };

  // Add required credentials
  connector.required_credentials?.forEach((cred) => {
    schema[cred.name] = {
      type: mapFieldType(cred.field_type),
      label: cred.description || cred.name,
      required: cred.required ?? false,
      secret: cred.sensitive,
      description: cred.description,
      credential: true,
    };
  });

  // Add optional config
  connector.optional_config?.forEach((config) => {
    schema[config.name] = {
      type: mapFieldType(config.field_type),
      label: config.description || config.name,
      required: false,
      placeholder:
        config.default_value === undefined ? undefined : String(config.default_value),
      description: config.description,
      credential: false,
    };
  });

  return schema;
}
