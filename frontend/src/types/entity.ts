// Entity types for RDF-based data governance

export interface DerivedAttribute {
  id: string;
  name: string;
  value: any;
  confidence: number;
  model_id: string;
  model_version?: string;
  generated_at: string;
  explanation?: Record<string, number>;
}

export interface Entity {
  id: string;
  domain: string;
  attributes: DerivedAttribute[];
  created_at: string;
  updated_at: string;
  source_systems?: string[];
  tags?: string[];
  quality_score?: number;
}

export interface EntityCreateRequest {
  domain: string;
  attributes?: Omit<DerivedAttribute, 'id'>[];
  source_systems?: string[];
  tags?: string[];
}

export interface EntityUpdateRequest {
  domain?: string;
  attributes?: DerivedAttribute[];
  tags?: string[];
}

export interface EntityListParams {
  page?: number;
  limit?: number;
  domain?: string;
  search?: string;
  sort_by?: 'created_at' | 'updated_at' | 'domain';
  sort_order?: 'asc' | 'desc';
}

export interface EntityRelation {
  source_id: string;
  target_id: string;
  relation_type: 'derived_from' | 'similar_to' | 'parent_of' | 'associated_with';
  confidence?: number;
  metadata?: Record<string, any>;
}

export interface FusionCandidate {
  entity_1: Entity;
  entity_2: Entity;
  similarity_score: number;
  matching_attributes: string[];
  fusion_rule_id?: string;
}