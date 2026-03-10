/**
 * Governance & SPARQL API
 */

import api from './client';
import {
  SparqlQueryRequest,
  SparqlQueryResponse,
  RdfStoreStats,
  Entity,
  EntityResponse,
  EntityAttributesResponse,
  EntityLineageResponse,
  AttributeTimeseriesResponse,
  EntityQueryParams,
} from './types';

export async function getModelLineageRdf(modelId: string): Promise<any> {
  return api.get(`/governance/model/${modelId}/lineage`);
}

export async function getRdfStoreStats(): Promise<RdfStoreStats> {
  return api.get<RdfStoreStats>('/governance/stats');
}

export async function executeSparqlQuery(
  request: SparqlQueryRequest
): Promise<SparqlQueryResponse> {
  return api.post<SparqlQueryResponse>('/governance/sparql', request);
}

// ============================================================================
// Entity API Functions (matching backend exactly)
// ============================================================================

/**
 * Get complete entity with properties and derived attributes
 */
export async function getEntity(entityId: string): Promise<EntityResponse> {
  return api.get<EntityResponse>(`/entities/${entityId}`);
}

/**
 * Get entity attributes only
 */
export async function getEntityAttributes(entityId: string): Promise<EntityAttributesResponse> {
  return api.get<EntityAttributesResponse>(`/entities/${entityId}/attributes`);
}

/**
 * Get entity lineage (W3C PROV provenance graph)
 */
export async function getEntityLineage(entityId: string): Promise<EntityLineageResponse> {
  return api.get<EntityLineageResponse>(`/entities/${entityId}/lineage`);
}

/**
 * Get attribute evolution over time
 */
export async function getAttributeTimeseries(
  entityId: string,
  attributeName: string
): Promise<AttributeTimeseriesResponse> {
  return api.get<AttributeTimeseriesResponse>(
    `/entities/${entityId}/attributes/${attributeName}/timeseries`
  );
}

/**
 * List all entities using new REST endpoint
 * Returns entities with fusion metadata (source_count, fusion details)
 */
export async function getEntities(params?: EntityQueryParams): Promise<Entity[]> {
  const response = await api.get<{ entities: Entity[]; total: number }>('/entities', {
    params: {
      limit: params?.limit,
      offset: params?.offset,
      min_confidence: params?.min_confidence,
    },
  });

  return response.entities || [];
}

/**
 * Query entities by domain using new REST endpoint
 */
export async function getEntitiesByDomain(domain: string, limit: number = 100): Promise<Entity[]> {
  const response = await api.get<{ entities: Entity[]; total: number }>('/entities', {
    params: {
      domain,
      limit,
    },
  });

  return response.entities || [];
}

// Helper functions
function extractEntityId(uri: string): string {
  const parts = uri.split('/');
  return parts[parts.length - 1] || uri;
}

function extractDomain(uri: string): string | undefined {
  // Try to extract domain from URI pattern: .../domain/entity-id
  const match = uri.match(/\/([^\/]+)\/[^\/]+$/);
  return match ? match[1] : undefined;
}
