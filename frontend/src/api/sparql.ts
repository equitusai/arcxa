/**
 * SPARQL Query API
 *
 * Execute SPARQL queries against the RDF governance brain
 */

import { apiClient } from './client';
import type {
  SparqlQueryRequest,
  SparqlQueryResponse,
  SparqlTemplate,
  SavedSparqlQuery,
} from './types';

/**
 * Execute SPARQL query
 */
export async function executeSparqlQuery(
  sparql: string
): Promise<SparqlQueryResponse> {
  const response = await apiClient.post<SparqlQueryResponse>(
    '/api/v1/governance/sparql',
    { sparql, format: 'json' }  // Backend expects "sparql" field and "format"
  );
  return response.data;
}

/**
 * Get SPARQL query templates
 * Returns pre-built templates with parameter definitions
 */
export async function getSparqlTemplates(): Promise<SparqlTemplate[]> {
  // Templates are defined client-side to match backend implementation
  // In future, could fetch from /api/v1/governance/sparql/templates
  return Promise.resolve(SPARQL_TEMPLATES);
}

/**
 * Validate SPARQL query syntax (SPARQL 1.1 compliant)
 */
export async function validateSparqlQuery(sparql: string): Promise<{
  valid: boolean;
  errors: string[];
  warnings: string[];
}> {
  // Client-side validation
  const errors: string[] = [];
  const warnings: string[] = [];

  // Check for empty query
  if (sparql.trim().length === 0) {
    errors.push('Query cannot be empty');
    return { valid: false, errors, warnings };
  }

  // Check length
  if (sparql.length > 50000) {
    errors.push('Query too long (max 50,000 characters)');
  }

  // Remove comments and normalize whitespace
  const cleanedValue = sparql
    .split('\n')
    .map(line => {
      const commentIndex = line.indexOf('#');
      return commentIndex >= 0 ? line.substring(0, commentIndex) : line;
    })
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();

  const upperQuery = cleanedValue.toUpperCase();

  // Skip validation if only PREFIX declarations or empty
  const withoutPrefixes = upperQuery
    .replace(/PREFIX\s+\w+:\s*<[^>]+>/gi, '')
    .replace(/BASE\s+<[^>]+>/gi, '')
    .trim();

  if (withoutPrefixes.length > 0) {
    // SPARQL 1.1 Query types
    const queryTypes = ['SELECT', 'CONSTRUCT', 'ASK', 'DESCRIBE'];

    // SPARQL 1.1 Update operations (note: these are typically not allowed in query endpoints)
    const updateOps = [
      'INSERT DATA', 'DELETE DATA', 'DELETE WHERE', 'INSERT',
      'DELETE', 'LOAD', 'CLEAR', 'DROP', 'CREATE',
      'COPY', 'MOVE', 'ADD'
    ];

    const hasQueryType = queryTypes.some(type => withoutPrefixes.startsWith(type));
    const hasUpdateOp = updateOps.some(op => upperQuery.includes(op));

    // Check for destructive operations in query endpoint
    if (hasUpdateOp) {
      errors.push('Update operations (INSERT, DELETE, etc.) not allowed in queries. Use UPDATE endpoint instead.');
    } else if (!hasQueryType) {
      errors.push('Query must start with SELECT, CONSTRUCT, ASK, or DESCRIBE');
    }

    // Check for balanced braces
    const openBraces = (sparql.match(/\{/g) || []).length;
    const closeBraces = (sparql.match(/\}/g) || []).length;
    if (openBraces !== closeBraces) {
      errors.push('Unbalanced braces { }');
    }

    // Check for balanced parentheses
    const openParens = (sparql.match(/\(/g) || []).length;
    const closeParens = (sparql.match(/\)/g) || []).length;
    if (openParens !== closeParens) {
      errors.push('Unbalanced parentheses ( )');
    }

    // Warn if no LIMIT (for SELECT queries only)
    if (upperQuery.includes('SELECT') && !upperQuery.includes('LIMIT') && !upperQuery.includes('ASK')) {
      warnings.push('Consider adding LIMIT clause for better performance');
    }
  }

  return {
    valid: errors.length === 0,
    errors,
    warnings,
  };
}

/**
 * Get saved queries for current user
 */
export async function getSavedQueries(): Promise<SavedSparqlQuery[]> {
  // Future: Backend endpoint
  // For now, use localStorage
  const saved = localStorage.getItem('sparql_saved_queries');
  return saved ? JSON.parse(saved) : [];
}

/**
 * Save SPARQL query
 */
export async function saveSparqlQuery(
  query: Omit<SavedSparqlQuery, 'id' | 'created_at' | 'updated_at'>
): Promise<SavedSparqlQuery> {
  const savedQuery: SavedSparqlQuery = {
    ...query,
    id: crypto.randomUUID(),
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };

  const existing = await getSavedQueries();
  const updated = [...existing, savedQuery];
  localStorage.setItem('sparql_saved_queries', JSON.stringify(updated));

  return savedQuery;
}

/**
 * Delete saved query
 */
export async function deleteSavedQuery(id: string): Promise<void> {
  const existing = await getSavedQueries();
  const filtered = existing.filter(q => q.id !== id);
  localStorage.setItem('sparql_saved_queries', JSON.stringify(filtered));
}

/**
 * Update saved query
 */
export async function updateSavedQuery(
  id: string,
  updates: Partial<SavedSparqlQuery>
): Promise<SavedSparqlQuery> {
  const existing = await getSavedQueries();
  const index = existing.findIndex(q => q.id === id);

  if (index === -1) {
    throw new Error('Query not found');
  }

  const updated = {
    ...existing[index],
    ...updates,
    updated_at: new Date().toISOString(),
  };

  existing[index] = updated;
  localStorage.setItem('sparql_saved_queries', JSON.stringify(existing));

  return updated;
}

// ============================================================================
// SPARQL Templates (matching backend implementation)
// ============================================================================

const GRAPHICA_NS = 'http://graphica.io/ontology#';
const ML_NS = 'http://graphica.io/ml#';
const PROV_NS = 'http://www.w3.org/ns/prov#';

export const SPARQL_TEMPLATES: SparqlTemplate[] = [
  {
    id: 'entity_attributes',
    name: 'Entity Attributes',
    description: 'Get all derived attributes for a specific entity, including ML predictions, confidence scores, and generating models.',
    category: 'Entity Queries',
    parameters: [
      {
        name: 'entity_id',
        label: 'Entity ID',
        type: 'entity_id',
        required: true,
        placeholder: 'e.g., customer/alice-123',
        helpText: 'The unique identifier of the entity',
      },
      {
        name: 'limit',
        label: 'Result Limit',
        type: 'number',
        required: false,
        defaultValue: 100,
        helpText: 'Maximum number of results to return',
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>

SELECT ?attrName ?value ?confidence ?model ?timestamp
WHERE {
  <${GRAPHICA_NS}entity/\${entity_id}> gph:hasDerivedAttribute ?attr .
  ?attr gph:attributeName ?attrName ;
        gph:value ?value ;
        gph:confidence ?confidence ;
        prov:wasGeneratedBy ?model ;
        prov:generatedAtTime ?timestamp .
}
ORDER BY DESC(?timestamp)
LIMIT \${limit}`,
    exampleResults: '~12 attributes per entity',
  },
  {
    id: 'model_impact',
    name: 'Model Impact',
    description: 'Find all entities affected by a specific ML model, showing which predictions were generated.',
    category: 'Model Queries',
    parameters: [
      {
        name: 'model_id',
        label: 'Model ID',
        type: 'model_id',
        required: true,
        placeholder: 'e.g., risk-model-v2.1',
        helpText: 'The model identifier to analyze',
      },
      {
        name: 'limit',
        label: 'Result Limit',
        type: 'number',
        required: false,
        defaultValue: 100,
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>
PREFIX ml: <${ML_NS}>

SELECT ?entity ?attrName ?confidence ?timestamp
WHERE {
  ?attr a gph:DerivedAttribute ;
        prov:wasGeneratedBy <${ML_NS}model/\${model_id}> ;
        gph:attributeName ?attrName ;
        gph:confidence ?confidence ;
        prov:generatedAtTime ?timestamp .
  ?entity gph:hasDerivedAttribute ?attr .
}
ORDER BY ?entity ?timestamp
LIMIT \${limit}`,
    exampleResults: '~50-200 entities',
  },
  {
    id: 'low_confidence',
    name: 'Low Confidence Attributes',
    description: 'Find attributes with confidence scores below a threshold for quality control.',
    category: 'Quality Queries',
    parameters: [
      {
        name: 'threshold',
        label: 'Confidence Threshold',
        type: 'threshold',
        required: true,
        defaultValue: 0.7,
        helpText: 'Show predictions below this confidence score (0.0 - 1.0)',
      },
      {
        name: 'limit',
        label: 'Result Limit',
        type: 'number',
        required: false,
        defaultValue: 100,
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>

SELECT ?entity ?attrName ?confidence ?model
WHERE {
  ?entity gph:hasDerivedAttribute ?attr .
  ?attr gph:attributeName ?attrName ;
        gph:confidence ?confidence ;
        prov:wasGeneratedBy ?model .
  FILTER (?confidence < \${threshold})
}
ORDER BY ?confidence
LIMIT \${limit}`,
    exampleResults: '~20-100 low confidence predictions',
  },
  {
    id: 'fusion_history',
    name: 'Fusion History',
    description: 'Get the complete merge history for an entity, showing all fusion operations.',
    category: 'Governance',
    parameters: [
      {
        name: 'entity_id',
        label: 'Entity ID',
        type: 'entity_id',
        required: true,
        placeholder: 'e.g., customer/merged-789',
      },
      {
        name: 'limit',
        label: 'Result Limit',
        type: 'number',
        required: false,
        defaultValue: 50,
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>

SELECT ?fusion ?sourceEntity ?rule ?confidence ?timestamp ?reversed
FROM <http://graphica.io/graph/fusion>
WHERE {
  ?fusion a gph:FusionOperation ;
          gph:mergedEntity <${GRAPHICA_NS}entity/\${entity_id}> ;
          gph:sourceEntity ?sourceEntity ;
          gph:fusionRule ?rule ;
          gph:fusionConfidence ?confidence ;
          prov:atTime ?timestamp .
  OPTIONAL { ?fusion gph:reversedAt ?reversed }
}
ORDER BY DESC(?timestamp)
LIMIT \${limit}`,
    exampleResults: '~5-20 fusion operations',
  },
  {
    id: 'entity_as_of',
    name: 'Time-Travel Query',
    description: 'View entity state as it existed at a specific date (temporal query).',
    category: 'Governance',
    parameters: [
      {
        name: 'entity_id',
        label: 'Entity ID',
        type: 'entity_id',
        required: true,
      },
      {
        name: 'date',
        label: 'As-Of Date',
        type: 'date',
        required: true,
        defaultValue: '2025-01-01',
        helpText: 'View entity state as of this date',
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>

SELECT ?attrName ?value ?confidence
FROM <http://graphica.io/graph/\${date}>
WHERE {
  <${GRAPHICA_NS}entity/\${entity_id}> gph:hasDerivedAttribute ?attr .
  ?attr gph:attributeName ?attrName ;
        gph:value ?value ;
        gph:confidence ?confidence .
}`,
    exampleResults: 'Historical snapshot of entity',
  },
  {
    id: 'attribute_evolution',
    name: 'Attribute Evolution',
    description: 'Track how a specific attribute value changed over time.',
    category: 'Governance',
    parameters: [
      {
        name: 'entity_id',
        label: 'Entity ID',
        type: 'entity_id',
        required: true,
      },
      {
        name: 'attribute_name',
        label: 'Attribute Name',
        type: 'text',
        required: true,
        placeholder: 'e.g., risk_score',
        helpText: 'The attribute to track',
      },
      {
        name: 'limit',
        label: 'Result Limit',
        type: 'number',
        required: false,
        defaultValue: 100,
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>

SELECT ?timestamp ?value ?confidence ?model
WHERE {
  <${GRAPHICA_NS}entity/\${entity_id}> gph:hasDerivedAttribute ?attr .
  ?attr gph:attributeName "\${attribute_name}" ;
        gph:value ?value ;
        gph:confidence ?confidence ;
        prov:wasGeneratedBy ?model ;
        prov:generatedAtTime ?timestamp .
}
ORDER BY ?timestamp
LIMIT \${limit}`,
    exampleResults: 'Timeline of attribute changes',
  },
  {
    id: 'entity_lineage',
    name: 'Entity Lineage',
    description: 'Get complete provenance lineage graph for an entity (W3C PROV format).',
    category: 'Governance',
    parameters: [
      {
        name: 'entity_id',
        label: 'Entity ID',
        type: 'entity_id',
        required: true,
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>

CONSTRUCT {
  ?s ?p ?o .
}
WHERE {
  {
    <${GRAPHICA_NS}entity/\${entity_id}> ?p ?o .
    BIND(<${GRAPHICA_NS}entity/\${entity_id}> AS ?s)
  }
  UNION
  {
    <${GRAPHICA_NS}entity/\${entity_id}> prov:wasGeneratedBy ?activity .
    ?activity ?p ?o .
    BIND(?activity AS ?s)
  }
  UNION
  {
    <${GRAPHICA_NS}entity/\${entity_id}> prov:wasGeneratedBy/prov:used ?used .
    ?used ?p ?o .
    BIND(?used AS ?s)
  }
}`,
    exampleResults: 'RDF graph with lineage triples',
  },
  {
    id: 'count_by_type',
    name: 'Count Entities by Type',
    description: 'Statistics query showing entity counts grouped by type.',
    category: 'Statistics',
    parameters: [],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>

SELECT ?entityType (COUNT(?entity) AS ?count)
WHERE {
  ?entity a gph:Entity ;
          gph:entityType ?entityType .
}
GROUP BY ?entityType
ORDER BY DESC(?count)`,
    exampleResults: 'Distribution of entity types',
  },
  {
    id: 'stale_predictions',
    name: 'Find Stale Predictions',
    description: 'Find entities that need re-prediction because the model was updated.',
    category: 'Model Queries',
    parameters: [
      {
        name: 'model_id',
        label: 'Model ID',
        type: 'model_id',
        required: true,
      },
      {
        name: 'since',
        label: 'Since Date',
        type: 'date',
        required: true,
        defaultValue: '2025-01-01T00:00:00Z',
        helpText: 'Find predictions older than this date',
      },
      {
        name: 'limit',
        label: 'Result Limit',
        type: 'number',
        required: false,
        defaultValue: 100,
      },
    ],
    sparql: `PREFIX gph: <${GRAPHICA_NS}>
PREFIX prov: <${PROV_NS}>
PREFIX ml: <${ML_NS}>

SELECT DISTINCT ?entity
WHERE {
  ?attr prov:wasGeneratedBy <${ML_NS}model/\${model_id}> ;
        prov:generatedAtTime ?timestamp .
  ?entity gph:hasDerivedAttribute ?attr .
  FILTER (?timestamp < "\${since}"^^xsd:dateTime)
}
LIMIT \${limit}`,
    exampleResults: 'Entities needing re-prediction',
  },
];

/**
 * Substitute parameters in template SPARQL
 */
export function substituteParameters(
  sparql: string,
  params: Record<string, any>
): string {
  let result = sparql;

  for (const [key, value] of Object.entries(params)) {
    const placeholder = `\${${key}}`;
    result = result.replace(new RegExp(placeholder.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'), String(value));
  }

  return result;
}
