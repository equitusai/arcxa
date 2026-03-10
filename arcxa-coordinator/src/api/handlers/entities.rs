//! Entity Handler Functions
//!
//! HTTP handlers for entity queries, datasets, derived attributes, and timeseries.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct EntityListQuery {
    pub domain: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DatasetScope {
    Materialized,
    SourceAssets,
    All,
}

impl DatasetScope {
    fn parse(raw: Option<&str>) -> Result<Self, ApiError> {
        match raw.unwrap_or("materialized") {
            "materialized" => Ok(Self::Materialized),
            "source_assets" => Ok(Self::SourceAssets),
            "all" => Ok(Self::All),
            other => Err(ApiError::bad_request(format!(
                "Invalid dataset_scope '{}'. Expected one of: materialized, source_assets, all",
                other
            ))),
        }
    }

    fn filter_clause(self, dataset_type: Option<&str>) -> Option<String> {
        if let Some(dataset_type) = dataset_type {
            return Some(format!(
                "FILTER (?datasetType = \"{}\")",
                escape_sparql_literal(dataset_type)
            ));
        }

        match self {
            DatasetScope::Materialized => Some("FILTER (?datasetType != \"source\")".to_string()),
            DatasetScope::SourceAssets => Some("FILTER (?datasetType = \"source\")".to_string()),
            DatasetScope::All => None,
        }
    }
}

fn escape_sparql_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn dataset_asset_kind(dataset_type: &str) -> &'static str {
    if dataset_type == "source" {
        "source_asset"
    } else {
        "materialized_dataset"
    }
}

/// List entities with optional domain filter
pub async fn list_entities(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<EntityListQuery>,
) -> Result<Json<EntityListResponse>, ApiError> {
    tracing::info!("Listing entities (domain filter: {:?})", query.domain);

    // Query RDF store for all entities
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Build SPARQL query with optional domain filter
        let domain_filter = if let Some(ref domain) = query.domain {
            format!("FILTER (?domain = \"{}\")", domain)
        } else {
            String::new()
        };

        let entity_list_query = format!(
            r#"
PREFIX gph: <{}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

SELECT ?entity ?entityType ?domain ?createdAt
WHERE {{
    ?entity a gph:Entity .
    OPTIONAL {{ ?entity gph:entityType ?entityType }}
    OPTIONAL {{ ?entity gph:domain ?domain }}
    OPTIONAL {{ ?entity gph:createdAt ?createdAt }}
    {}
}}
ORDER BY ?entity
LIMIT 100
"#,
            crate::governance::ontology::GRAPHICA_NS,
            domain_filter
        );

        match rdf_store.as_ref().query(&entity_list_query) {
            Ok(results) => {
                let mut entities = Vec::new();

                for result in results.iter() {
                    if let Some(entity_uri) = result.get("entity").and_then(|v| v.as_str()) {
                        let entity_id = entity_uri.rsplit('/').next().unwrap_or(entity_uri);

                        // For each entity, fetch fusion metadata
                        let fusion_query = crate::governance::sparql_templates::SparqlTemplates::get_fusion_history(entity_id);

                        let (source_count, source_ids, fusion_rule, fusion_confidence, fusion_date) =
                            if let Ok(fusion_data) = rdf_store.as_ref().query(&fusion_query) {
                                if !fusion_data.is_empty() {
                                    // Extract unique source entity IDs
                                    let mut sources: Vec<String> = fusion_data
                                        .iter()
                                        .filter_map(|r| {
                                            r.get("sourceEntity").and_then(|v| v.as_str()).map(
                                                |s| s.rsplit('/').next().unwrap_or(s).to_string(),
                                            )
                                        })
                                        .collect();
                                    sources.sort();
                                    sources.dedup();

                                    let first = &fusion_data[0];
                                    let rule = first
                                        .get("rule")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);
                                    let confidence =
                                        first.get("confidence").and_then(|v| v.as_f64());
                                    let timestamp = first
                                        .get("timestamp")
                                        .and_then(|v| v.as_str())
                                        .map(String::from);

                                    let count = sources.len() as i32;
                                    (
                                        count,
                                        if count > 0 { Some(sources) } else { None },
                                        rule,
                                        confidence,
                                        timestamp,
                                    )
                                } else {
                                    (1, None, None, None, None)
                                }
                            } else {
                                (1, None, None, None, None)
                            };

                        entities.push(EntitySummary {
                            id: entity_id.to_string(),
                            entity_type: result
                                .get("entityType")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            domain: result
                                .get("domain")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            attribute_count: 0, // TODO: Count attributes efficiently
                            avg_confidence: 0.0, // TODO: Calculate average confidence
                            status: "active".to_string(),
                            created_at: result
                                .get("createdAt")
                                .and_then(|v| v.as_str())
                                .unwrap_or("2025-01-01T00:00:00Z")
                                .to_string(),
                            source_count,
                            source_ids,
                            fusion_rule,
                            fusion_confidence,
                            fusion_date,
                        });
                    }
                }

                let total = entities.len();

                return Ok(Json(EntityListResponse { entities, total }));
            }
            Err(e) => {
                tracing::warn!("RDF query for entity list failed: {}", e);
                return Err(ApiError::internal(format!(
                    "Failed to list entities: {}",
                    e
                )));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// List datasets with RDF query
pub async fn list_datasets(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<DatasetListQuery>,
) -> Result<Json<DatasetListResponse>, ApiError> {
    let dataset_scope = DatasetScope::parse(query.dataset_scope.as_deref())?;
    tracing::info!(
        "Listing datasets (type filter: {:?}, scope: {:?})",
        query.dataset_type,
        dataset_scope
    );

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        let page = query.page.unwrap_or(0);
        let page_size = query.page_size.unwrap_or(50).min(100);
        let offset = page * page_size;
        let filter_clause = dataset_scope.filter_clause(query.dataset_type.as_deref());

        let sparql = crate::governance::sparql_templates::SparqlTemplates::list_datasets(
            filter_clause.as_deref(),
            page_size,
            offset,
        );

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                let datasets: Vec<DatasetSummary> = results
                    .iter()
                    .filter_map(|r| {
                        let dataset_type = r.get("type")?.as_str()?.to_string();
                        Some(DatasetSummary {
                            id: r.get("dataset")?.as_str()?.rsplit('/').next()?.to_string(),
                            name: r.get("name")?.as_str()?.to_string(),
                            asset_kind: dataset_asset_kind(&dataset_type).to_string(),
                            dataset_type,
                            record_count: r.get("recordCount")?.as_i64()?,
                            quality_score: r.get("qualityScore").and_then(|v| v.as_f64()),
                            created_at: r.get("createdAt")?.as_str()?.to_string(),
                            updated_at: r.get("updatedAt")?.as_str()?.to_string(),
                            last_ingested_at: None,
                            source_datasource_id: r
                                .get("sourceDataSource")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            workflow_execution_id: r
                                .get("workflow")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.rsplit('/').next())
                                .map(String::from),
                        })
                    })
                    .collect();

                let total = datasets.len();

                return Ok(Json(DatasetListResponse {
                    datasets,
                    total,
                    page,
                    page_size,
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query for datasets failed: {}", e);
                return Err(ApiError::internal(format!(
                    "Failed to list datasets: {}",
                    e
                )));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Get dataset by ID with full schema and lineage
pub async fn get_dataset(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<DatasetResponse>, ApiError> {
    tracing::info!("Getting dataset: {}", id);

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Query dataset metadata
        let metadata_query =
            crate::governance::sparql_templates::SparqlTemplates::get_dataset_by_id(&id);
        let schema_query =
            crate::governance::sparql_templates::SparqlTemplates::get_dataset_schema(&id);
        let lineage_query =
            crate::governance::sparql_templates::SparqlTemplates::get_dataset_lineage(&id);

        match (
            rdf_store.as_ref().query(&metadata_query),
            rdf_store.as_ref().query(&schema_query),
            rdf_store.as_ref().query(&lineage_query),
        ) {
            (Ok(metadata), Ok(schema), Ok(lineage)) => {
                // Parse metadata
                let mut name = String::new();
                let mut dataset_type = String::new();
                let mut record_count = 0;
                let mut quality_score = None;
                let mut created_at = String::new();
                let mut updated_at = String::new();

                for result in metadata.iter() {
                    if let (Some(prop), Some(val)) = (result.get("property"), result.get("value")) {
                        if let (Some(p), Some(v)) = (prop.as_str(), val.as_str()) {
                            match p.rsplit('#').next().or_else(|| p.rsplit('/').next()) {
                                Some("datasetName") => name = v.to_string(),
                                Some("datasetType") => dataset_type = v.to_string(),
                                Some("recordCount") => record_count = v.parse().unwrap_or(0),
                                Some("qualityScore") => quality_score = v.parse().ok(),
                                Some("createdAt") => created_at = v.to_string(),
                                Some("updatedAt") => updated_at = v.to_string(),
                                _ => {}
                            }
                        }
                    }
                }

                // Parse schema
                let columns: Vec<DatasetColumnDto> = schema
                    .iter()
                    .filter_map(|r| {
                        Some(DatasetColumnDto {
                            name: r.get("columnName")?.as_str()?.to_string(),
                            data_type: r.get("columnType")?.as_str()?.to_string(),
                            nullable: r.get("nullable")?.as_bool()?,
                            distinct_count: r.get("distinctCount").and_then(|v| v.as_i64()),
                            null_percentage: r.get("nullPercentage").and_then(|v| v.as_f64()),
                        })
                    })
                    .collect();

                // Parse lineage
                let lineage_data = if !lineage.is_empty() {
                    let first = &lineage[0];
                    DatasetLineage {
                        source_datasource_id: first
                            .get("sourceDataSource")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        workflow_execution_id: first
                            .get("workflow")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.rsplit('/').next())
                            .map(String::from),
                        workflow_name: first
                            .get("workflowName")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        executed_at: first
                            .get("executedAt")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    }
                } else {
                    DatasetLineage {
                        source_datasource_id: None,
                        workflow_execution_id: None,
                        workflow_name: None,
                        executed_at: None,
                    }
                };

                return Ok(Json(DatasetResponse {
                    id: id.clone(),
                    name,
                    asset_kind: dataset_asset_kind(&dataset_type).to_string(),
                    dataset_type,
                    record_count,
                    schema: columns,
                    quality_score,
                    created_at,
                    updated_at,
                    last_ingested_at: None,
                    lineage: lineage_data,
                }));
            }
            _ => {
                return Err(ApiError::not_found(format!("Dataset {} not found", id)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Get derived attributes for an entity - Queries RDF for model predictions
pub async fn get_entity_attributes(
    State(state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
) -> Result<Json<EntityAttributesResponse>, ApiError> {
    tracing::info!("Getting derived attributes for entity: {}", entity_id);

    // Query RDF store for entity attributes
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;
        let sparql =
            crate::governance::sparql_templates::SparqlTemplates::get_entity_attributes(&entity_id);

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                tracing::info!(
                    "Found {} attributes for entity {}",
                    results.len(),
                    entity_id
                );

                // Parse results into attributes
                let attributes: Vec<DerivedAttribute> = results
                    .iter()
                    .filter_map(|r| {
                        Some(DerivedAttribute {
                            name: r.get("attrName")?.as_str()?.to_string(),
                            value: r.get("value")?.as_str()?.to_string(),
                            confidence: r.get("confidence")?.as_f64()?,
                            model_id: r.get("model")?.as_str().map(String::from),
                            timestamp: r.get("timestamp")?.as_str()?.to_string(),
                        })
                    })
                    .collect();

                let total = attributes.len();

                return Ok(Json(EntityAttributesResponse {
                    entity_id: entity_id.clone(),
                    attributes,
                    total,
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query for entity attributes failed: {}", e);
                return Err(ApiError::internal(format!(
                    "Failed to query attributes: {}",
                    e
                )));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Get complete entity with all properties (including derived attributes)
pub async fn get_entity(
    State(state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
) -> Result<Json<EntityResponse>, ApiError> {
    tracing::info!("Getting complete entity: {}", entity_id);

    // Query RDF store for entity and all its properties
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // First get basic entity properties
        let entity_query = format!(
            r#"
PREFIX gph: <{}>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

SELECT ?p ?o
WHERE {{
    <{}/entity/{}> ?p ?o .
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::GRAPHICA_NS,
            entity_id
        );

        // Get derived attributes
        let attributes_query =
            crate::governance::sparql_templates::SparqlTemplates::get_entity_attributes(&entity_id);

        // Get fusion metadata
        let fusion_query =
            crate::governance::sparql_templates::SparqlTemplates::get_fusion_history(&entity_id);

        match (
            rdf_store.as_ref().query(&entity_query),
            rdf_store.as_ref().query(&attributes_query),
            rdf_store.as_ref().query(&fusion_query),
        ) {
            (Ok(entity_results), Ok(attr_results), fusion_results) => {
                // Parse entity properties
                let mut properties: std::collections::HashMap<String, serde_json::Value> =
                    std::collections::HashMap::new();
                for r in entity_results.iter() {
                    if let (Some(p), Some(o)) = (r.get("p"), r.get("o")) {
                        if let (Some(pred), Some(obj)) = (p.as_str(), o.as_str()) {
                            properties.insert(
                                pred.to_string(),
                                serde_json::Value::String(obj.to_string()),
                            );
                        }
                    }
                }

                // Parse derived attributes
                let attributes: Vec<DerivedAttribute> = attr_results
                    .iter()
                    .filter_map(|r| {
                        Some(DerivedAttribute {
                            name: r.get("attrName")?.as_str()?.to_string(),
                            value: r.get("value")?.as_str()?.to_string(),
                            confidence: r.get("confidence")?.as_f64()?,
                            model_id: r.get("model")?.as_str().map(String::from),
                            timestamp: r.get("timestamp")?.as_str()?.to_string(),
                        })
                    })
                    .collect();

                // Parse fusion metadata
                let (source_count, source_ids, fusion_rule, fusion_confidence, fusion_date) =
                    if let Ok(fusion_data) = fusion_results {
                        if !fusion_data.is_empty() {
                            // Extract unique source entity IDs
                            let mut sources: Vec<String> = fusion_data
                                .iter()
                                .filter_map(|r| {
                                    r.get("sourceEntity").and_then(|v| v.as_str()).map(|s| {
                                        // Extract entity ID from URI
                                        s.rsplit('/').next().unwrap_or(s).to_string()
                                    })
                                })
                                .collect();
                            sources.sort();
                            sources.dedup();

                            // Get fusion metadata from most recent fusion (first result)
                            let first = &fusion_data[0];
                            let rule = first.get("rule").and_then(|v| v.as_str()).map(String::from);
                            let confidence = first.get("confidence").and_then(|v| v.as_f64());
                            let timestamp = first
                                .get("timestamp")
                                .and_then(|v| v.as_str())
                                .map(String::from);

                            let count = sources.len() as i32;
                            (
                                count,
                                if count > 0 { Some(sources) } else { None },
                                rule,
                                confidence,
                                timestamp,
                            )
                        } else {
                            // No fusion data - single source entity
                            (1, None, None, None, None)
                        }
                    } else {
                        // Fusion query failed or not available - assume single source
                        (1, None, None, None, None)
                    };

                return Ok(Json(EntityResponse {
                    entity_id: entity_id.clone(),
                    entity_type: properties
                        .get("http://graphica.io/ontology#entityType")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    properties,
                    derived_attributes: attributes,
                    source_count,
                    source_ids,
                    fusion_rule,
                    fusion_confidence,
                    fusion_date,
                }));
            }
            _ => {
                return Err(ApiError::internal("Failed to query entity".to_string()));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Get entity lineage (W3C PROV provenance graph)
pub async fn get_entity_lineage(
    State(state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
) -> Result<Json<EntityLineageResponse>, ApiError> {
    tracing::info!("Getting lineage for entity: {}", entity_id);

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;
        let sparql =
            crate::governance::sparql_templates::SparqlTemplates::get_entity_lineage(&entity_id);

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                tracing::info!(
                    "Found {} lineage triples for entity {}",
                    results.len(),
                    entity_id
                );

                return Ok(Json(EntityLineageResponse {
                    entity_id: entity_id.clone(),
                    lineage_graph: results,
                    format: "W3C PROV".to_string(),
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query for entity lineage failed: {}", e);
                return Err(ApiError::internal(format!(
                    "Failed to query lineage: {}",
                    e
                )));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Get attribute timeseries (evolution over time)
pub async fn get_attribute_timeseries(
    State(state): State<Arc<ApiState>>,
    Path((entity_id, attr_name)): Path<(String, String)>,
) -> Result<Json<AttributeTimeseriesResponse>, ApiError> {
    tracing::info!(
        "Getting timeseries for entity {} attribute {}",
        entity_id,
        attr_name
    );

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;
        let sparql = crate::governance::sparql_templates::SparqlTemplates::get_attribute_evolution(
            &entity_id, &attr_name,
        );

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                tracing::info!(
                    "Found {} historical values for {}.{}",
                    results.len(),
                    entity_id,
                    attr_name
                );

                let datapoints: Vec<AttributeDatapoint> = results
                    .iter()
                    .filter_map(|r| {
                        Some(AttributeDatapoint {
                            timestamp: r.get("timestamp")?.as_str()?.to_string(),
                            value: r.get("value")?.as_str()?.to_string(),
                            confidence: r.get("confidence")?.as_f64()?,
                            model_id: r.get("model")?.as_str().map(String::from),
                        })
                    })
                    .collect();

                let total = datapoints.len();

                return Ok(Json(AttributeTimeseriesResponse {
                    entity_id: entity_id.clone(),
                    attribute_name: attr_name.clone(),
                    datapoints,
                    total,
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query for attribute timeseries failed: {}", e);
                return Err(ApiError::internal(format!(
                    "Failed to query timeseries: {}",
                    e
                )));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}
