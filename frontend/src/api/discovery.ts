/**
 * Dataset Discovery API
 * For discovering tables and columns from datasources
 */

import { api } from './client';

export interface DiscoveredColumn {
  name: string;
  dataType: string;
  nullable: boolean;
  primaryKey: boolean;
  defaultValue?: string;
}

export interface DiscoveredTable {
  name: string;
  columns: DiscoveredColumn[];
  estimatedRows?: number;
}

export interface SchemaDiscoveryResponse {
  name: string; // schema name (e.g., "public")
  tables: DiscoveredTable[];
  inferredAt: string;
}

export interface SchemaDiscoveryRequest {
  sourceId: string;
  tableName?: string | null;
  sampleSize?: number;
}

/**
 * Discover tables and columns from a datasource
 */
export async function discoverSchema(datasourceId: string): Promise<SchemaDiscoveryResponse> {
  const request: SchemaDiscoveryRequest = {
    sourceId: datasourceId,
    tableName: null,
    sampleSize: 100,
  };

  return api.post(`/datasources/${datasourceId}/schema/infer`, request);
}

/**
 * Discover specific table schema
 */
export async function discoverTableSchema(
  datasourceId: string,
  tableName: string
): Promise<SchemaDiscoveryResponse> {
  const request: SchemaDiscoveryRequest = {
    sourceId: datasourceId,
    tableName,
    sampleSize: 100,
  };

  return api.post(`/datasources/${datasourceId}/schema/infer`, request);
}
