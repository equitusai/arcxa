/**
 * Ontology Management API Client
 *
 * Provides methods to:
 * - List registered ontologies
 * - Register/update custom ontologies
 * - Get ontology tree structure for visualization
 * - Activate/deactivate ontologies
 * - Delete ontologies
 */

import api from './client';

/**
 * Ontology metadata
 */
export interface OntologyMetadata {
  id: string;
  name: string;
  description?: string;
  namespace: string;
  version: string;
  registered_at: string;
  updated_at: string;
  tags: string[];
  active: boolean;
}

/**
 * Full ontology with content
 */
export interface RegisteredOntology {
  metadata: OntologyMetadata;
  content: string;  // Turtle RDF format
  validation_status: ValidationStatus;
  stats: OntologyStatistics;
}

export interface ValidationStatus {
  valid: boolean;
  errors: string[];
  warnings: string[];
  validated_at?: string;
}

export interface OntologyStatistics {
  total_classes: number;
  total_properties: number;
  total_individuals: number;
  max_hierarchy_depth: number;
}

/**
 * Request to register a new ontology
 */
export interface RegisterOntologyRequest {
  id: string;
  name: string;
  description?: string;
  namespace: string;
  content: string;  // Turtle format
  tags?: string[];
  version?: string;
}

/**
 * Tree structure types for visualization
 */
export interface OntologyTreeResponse {
  namespace: string;
  metadata: OntologyMetadata;
  root_classes: ClassNode[];
  root_properties: PropertyNode[];
  stats: TreeStats;
}

export interface ClassNode {
  uri: string;
  label: string;
  comment?: string;
  parent_classes: string[];
  subclasses: ClassNode[];
  properties?: PropertyNode[];
  individuals?: IndividualNode[];
  depth: number;
  deprecated: boolean;
}

export interface PropertyNode {
  uri: string;
  label: string;
  comment?: string;
  property_type: PropertyType;
  domain: string[];
  range: string[];
  parent_properties: string[];
  subproperties: PropertyNode[];
  deprecated: boolean;
}

export interface IndividualNode {
  uri: string;
  label: string;
  comment?: string;
  types: string[];
}

export type PropertyType = 'object_property' | 'datatype_property' | 'annotation_property';

export interface TreeStats {
  total_classes: number;
  total_properties: number;
  total_individuals: number;
  max_depth: number;
}

/**
 * Response from list ontologies endpoint
 */
interface ListOntologiesResponse {
  ontologies: OntologyMetadata[];
  total: number;
  active_only: boolean;
}

/**
 * List all registered ontologies
 */
export async function listOntologies(activeOnly: boolean = false): Promise<OntologyMetadata[]> {
  const params = activeOnly ? '?active_only=true' : '';
  const response = await api.get<ListOntologiesResponse>(`/ontology${params}`);
  return response.ontologies; // api.get already unwraps response.data
}

/**
 * Get a specific ontology by ID
 */
export async function getOntology(id: string): Promise<RegisteredOntology> {
  return api.get<RegisteredOntology>(`/ontology/${id}`);
}

/**
 * Get ontology tree structure for visualization
 */
export async function getOntologyTree(
  id: string,
  options?: {
    maxDepth?: number;
    includeProperties?: boolean;
    includeIndividuals?: boolean;
  }
): Promise<OntologyTreeResponse> {
  const params = new URLSearchParams();
  if (options?.maxDepth !== undefined) params.append('max_depth', options.maxDepth.toString());
  if (options?.includeProperties !== undefined) params.append('include_properties', options.includeProperties.toString());
  if (options?.includeIndividuals !== undefined) params.append('include_individuals', options.includeIndividuals.toString());

  const queryString = params.toString() ? `?${params.toString()}` : '';
  return api.get<OntologyTreeResponse>(`/ontology/${id}/tree${queryString}`);
}

/**
 * Register a new custom ontology
 */
export async function registerOntology(request: RegisterOntologyRequest): Promise<RegisteredOntology> {
  return api.post<RegisteredOntology>('/ontology', request);
}

/**
 * Update an existing ontology
 */
export async function updateOntology(id: string, updates: Partial<RegisterOntologyRequest>): Promise<RegisteredOntology> {
  return api.put<RegisteredOntology>(`/ontology/${id}`, updates);
}

/**
 * Activate an ontology for use in field mapping
 */
export async function activateOntology(id: string): Promise<void> {
  await api.post(`/ontology/${id}/activate`);
}

/**
 * Deactivate an ontology
 */
export async function deactivateOntology(id: string): Promise<void> {
  await api.post(`/ontology/${id}/deactivate`);
}

/**
 * Delete an ontology
 *
 * @param id - Ontology ID
 * @param permanent - If true, permanently delete from database. If false (default), soft delete (deactivate)
 */
export async function deleteOntology(id: string, permanent: boolean = false): Promise<void> {
  const params = permanent ? '?permanent=true' : '';
  await api.delete(`/ontology/${id}${params}`);
}

/**
 * Get merged ontology (combines multiple ontologies)
 *
 * @param ontologyIds - Optional array of ontology IDs to merge. If empty, merges all active ontologies
 * @returns Merged ontology content in Turtle format
 */
export async function getMergedOntology(
  ontologyIds?: string[]
): Promise<{
  content: string;
  size_bytes: number;
  included_ontologies: string[];
}> {
  return api.post('/ontology/merge', {
    ontology_ids: ontologyIds || []
  });
}

/**
 * Validate ontology content without registering
 */
export async function validateOntology(content: string, format: string = 'turtle'): Promise<ValidationStatus> {
  return api.post<ValidationStatus>('/ontology/validate', { content, format });
}
