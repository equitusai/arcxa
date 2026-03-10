import { api } from './client';
import { Entity, EntityCreateRequest, EntityUpdateRequest, EntityListParams, FusionCandidate } from '@/types/entity';
import { PaginatedResponse } from '@/types/api';
import { LineageGraph, LineageQueryParams } from '@/types/lineage';

// Entity CRUD operations
export const entityApi = {
  // List entities with pagination and filters
  async list(params?: EntityListParams) {
    const queryParams = new URLSearchParams();
    if (params?.page) queryParams.append('page', params.page.toString());
    if (params?.limit) queryParams.append('limit', params.limit.toString());
    if (params?.domain) queryParams.append('domain', params.domain);
    if (params?.search) queryParams.append('search', params.search);
    if (params?.sort_by) queryParams.append('sort_by', params.sort_by);
    if (params?.sort_order) queryParams.append('sort_order', params.sort_order);

    return api.get<PaginatedResponse<Entity>>(`/entities?${queryParams}`);
  },

  // Get single entity by ID
  async get(id: string) {
    return api.get<Entity>(`/entities/${id}`);
  },

  // Create new entity
  async create(entity: EntityCreateRequest) {
    return api.post<Entity>('/entities', entity);
  },

  // Update entity
  async update(id: string, entity: EntityUpdateRequest) {
    return api.put<Entity>(`/entities/${id}`, entity);
  },

  // Delete entity
  async delete(id: string) {
    return api.delete(`/entities/${id}`);
  },

  // Get entity lineage
  async getLineage(id: string, params?: LineageQueryParams) {
    const queryParams = new URLSearchParams();
    if (params?.depth) queryParams.append('depth', params.depth.toString());
    if (params?.include_models) queryParams.append('include_models', params.include_models.toString());
    if (params?.as_of) queryParams.append('as_of', params.as_of);

    return api.get<LineageGraph>(`/entities/${id}/lineage?${queryParams}`);
  },

  // Time-travel query: Get entity as it existed at a specific timestamp
  async getAsOf(id: string, timestamp: string) {
    return api.get<Entity>(`/entities/${id}/as-of?timestamp=${timestamp}`);
  },

  // Find fusion candidates
  async findFusionCandidates(entityId: string, threshold?: number) {
    const queryParams = new URLSearchParams();
    if (threshold) queryParams.append('threshold', threshold.toString());

    return api.post<FusionCandidate[]>('/fusion/candidates', {
      entity_id: entityId,
      threshold: threshold || 0.7
    });
  },

  // Resolve entity fusion
  async resolveFusion(entityIds: string[], rule: string) {
    return api.post('/fusion/resolve', {
      entity_ids: entityIds,
      rule
    });
  },

  // Reverse fusion operation
  async reverseFusion(fusionId: string) {
    return api.post(`/fusion/${fusionId}/reverse`);
  }
};