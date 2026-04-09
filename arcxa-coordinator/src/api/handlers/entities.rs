//! Entity Handler Functions
//!
//! HTTP handlers for entity queries, datasets, derived attributes, and timeseries.

use crate::api::dto::*;
use crate::api::ApiState;
use crate::governance::ontology::{uris, GRAPHICA_NS, PROV_NS, RDF_NS};
use crate::governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct EntityListQuery {
    pub domain: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub min_confidence: Option<f64>,
}

const DEFAULT_ENTITY_LIST_LIMIT: usize = 100;
const MAX_ENTITY_LIST_LIMIT: usize = 100;
const DEFAULT_ENTITY_CREATED_AT: &str = "2025-01-01T00:00:00Z";

fn json_value_to_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Number(value) => Some(value.to_string()),
        JsonValue::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn compare_timestamps_desc(left: &str, right: &str) -> Ordering {
    match (left.parse::<i64>(), right.parse::<i64>()) {
        (Ok(left_num), Ok(right_num)) => right_num.cmp(&left_num),
        _ => right.cmp(left),
    }
}

fn build_select_query(
    projections: &[&str],
    triple_pattern: &str,
    graph: Option<&NamedGraph>,
    limit: Option<usize>,
) -> String {
    let projection_clause = projections.join(" ");
    let (graph_prefix, graph_suffix) = if let Some(graph) = graph {
        (format!("GRAPH <{}> {{ ", graph.uri), " }")
    } else {
        (String::new(), "")
    };

    let limit_clause = limit
        .map(|limit| format!("\nLIMIT {}", limit))
        .unwrap_or_default();

    format!(
        "SELECT {projection_clause}\nWHERE {{\n    {graph_prefix}{triple_pattern} .{graph_suffix}\n}}{limit_clause}\n"
    )
}

fn query_rows(
    rdf_store: &GraphicaRdfStore,
    projections: &[&str],
    triple_pattern: &str,
    graph: Option<&NamedGraph>,
    limit: Option<usize>,
) -> Result<Vec<JsonValue>, ApiError> {
    let query = build_select_query(projections, triple_pattern, graph, limit);
    rdf_store
        .query(&query)
        .map_err(|e| ApiError::internal(format!("RDF query failed: {}", e)))
}

fn query_object_values(
    rdf_store: &GraphicaRdfStore,
    subject_uri: &str,
    predicate_uri: &str,
    graph: Option<&NamedGraph>,
) -> Result<Vec<String>, ApiError> {
    let triple_pattern = format!("<{}> <{}> ?value", subject_uri, predicate_uri);
    let rows = query_rows(rdf_store, &["?value"], &triple_pattern, graph, None)?;

    Ok(rows
        .iter()
        .filter_map(|row| row.get("value").and_then(json_value_to_string))
        .collect())
}

fn query_first_object_value(
    rdf_store: &GraphicaRdfStore,
    subject_uri: &str,
    predicate_uri: &str,
    graph: Option<&NamedGraph>,
) -> Result<Option<String>, ApiError> {
    Ok(
        query_object_values(rdf_store, subject_uri, predicate_uri, graph)?
            .into_iter()
            .next(),
    )
}

fn query_subjects_for_object(
    rdf_store: &GraphicaRdfStore,
    predicate_uri: &str,
    object_uri: &str,
    graph: Option<&NamedGraph>,
) -> Result<Vec<String>, ApiError> {
    let triple_pattern = format!("?subject <{}> <{}>", predicate_uri, object_uri);
    let rows = query_rows(rdf_store, &["?subject"], &triple_pattern, graph, None)?;

    Ok(rows
        .iter()
        .filter_map(|row| row.get("subject").and_then(json_value_to_string))
        .collect())
}

fn query_subject_predicate_objects(
    rdf_store: &GraphicaRdfStore,
    subject_uri: &str,
) -> Result<Vec<(String, String)>, ApiError> {
    let triple_pattern = format!("<{}> ?p ?o", subject_uri);
    let rows = query_rows(rdf_store, &["?p", "?o"], &triple_pattern, None, None)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            Some((
                row.get("p").and_then(json_value_to_string)?,
                row.get("o").and_then(json_value_to_string)?,
            ))
        })
        .collect())
}

fn query_entity_uris(rdf_store: &GraphicaRdfStore) -> Result<Vec<String>, ApiError> {
    let triple_pattern = format!("?entity <{}type> <{}Entity>", RDF_NS, GRAPHICA_NS);
    let rows = query_rows(rdf_store, &["?entity"], &triple_pattern, None, None)?;

    Ok(rows
        .iter()
        .filter_map(|row| row.get("entity").and_then(json_value_to_string))
        .collect())
}

fn entity_id_from_uri(entity_uri: &str) -> String {
    entity_uri
        .rsplit('/')
        .next()
        .unwrap_or(entity_uri)
        .to_string()
}

fn fetch_derived_attributes(
    rdf_store: &GraphicaRdfStore,
    entity_id: &str,
) -> Result<Vec<DerivedAttribute>, ApiError> {
    let entity_uri = uris::entity(entity_id);
    let attribute_uris = query_object_values(
        rdf_store,
        &entity_uri,
        &format!("{GRAPHICA_NS}hasDerivedAttribute"),
        None,
    )?;

    let mut attributes = Vec::new();

    for attribute_uri in attribute_uris {
        let name = query_first_object_value(
            rdf_store,
            &attribute_uri,
            &format!("{GRAPHICA_NS}attributeName"),
            None,
        )?;
        let value = query_first_object_value(
            rdf_store,
            &attribute_uri,
            &format!("{GRAPHICA_NS}value"),
            None,
        )?;
        let confidence = query_first_object_value(
            rdf_store,
            &attribute_uri,
            &format!("{GRAPHICA_NS}confidence"),
            None,
        )?;
        let model_id = query_first_object_value(
            rdf_store,
            &attribute_uri,
            &format!("{PROV_NS}wasGeneratedBy"),
            None,
        )?
        .map(|model_uri| entity_id_from_uri(&model_uri));
        let timestamp = query_first_object_value(
            rdf_store,
            &attribute_uri,
            &format!("{PROV_NS}generatedAtTime"),
            None,
        )?;

        let Some(name) = name else { continue };
        let Some(value) = value else { continue };
        let Some(timestamp) = timestamp else { continue };

        let confidence = confidence
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);

        attributes.push(DerivedAttribute {
            name,
            value,
            confidence,
            model_id,
            timestamp,
        });
    }

    attributes.sort_by(|left, right| compare_timestamps_desc(&left.timestamp, &right.timestamp));
    Ok(attributes)
}

fn fetch_fusion_metadata(
    rdf_store: &GraphicaRdfStore,
    entity_id: &str,
) -> Result<
    (
        i32,
        Option<Vec<String>>,
        Option<String>,
        Option<f64>,
        Option<String>,
    ),
    ApiError,
> {
    let fusion_graph = NamedGraph::fusion();
    let entity_uri = uris::entity(entity_id);
    let fusion_uris = query_subjects_for_object(
        rdf_store,
        &format!("{GRAPHICA_NS}mergedEntity"),
        &entity_uri,
        Some(&fusion_graph),
    )?;

    if fusion_uris.is_empty() {
        return Ok((1, None, None, None, None));
    }

    let mut source_ids = BTreeSet::new();
    let mut fusion_records = Vec::new();

    for fusion_uri in fusion_uris {
        for source_uri in query_object_values(
            rdf_store,
            &fusion_uri,
            &format!("{GRAPHICA_NS}sourceEntity"),
            Some(&fusion_graph),
        )? {
            source_ids.insert(entity_id_from_uri(&source_uri));
        }

        let rule = query_first_object_value(
            rdf_store,
            &fusion_uri,
            &format!("{GRAPHICA_NS}fusionRule"),
            Some(&fusion_graph),
        )?;
        let confidence = query_first_object_value(
            rdf_store,
            &fusion_uri,
            &format!("{GRAPHICA_NS}fusionConfidence"),
            Some(&fusion_graph),
        )?
        .and_then(|value| value.parse::<f64>().ok());
        let timestamp = query_first_object_value(
            rdf_store,
            &fusion_uri,
            &format!("{PROV_NS}atTime"),
            Some(&fusion_graph),
        )?;

        fusion_records.push((timestamp, rule, confidence));
    }

    fusion_records.sort_by(|left, right| match (&left.0, &right.0) {
        (Some(left_ts), Some(right_ts)) => compare_timestamps_desc(left_ts, right_ts),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    let source_ids: Vec<String> = source_ids.into_iter().collect();
    let source_count = if source_ids.is_empty() {
        1
    } else {
        source_ids.len() as i32
    };

    let (fusion_date, fusion_rule, fusion_confidence) = fusion_records
        .into_iter()
        .next()
        .map(|(timestamp, rule, confidence)| (timestamp, rule, confidence))
        .unwrap_or((None, None, None));

    Ok((
        source_count,
        if source_ids.is_empty() {
            None
        } else {
            Some(source_ids)
        },
        fusion_rule,
        fusion_confidence,
        fusion_date,
    ))
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

    if let Some(ref rdf_store) = state.rdf_store {
        let limit = query
            .limit
            .unwrap_or(DEFAULT_ENTITY_LIST_LIMIT)
            .min(MAX_ENTITY_LIST_LIMIT);

        let mut entity_uris = query_entity_uris(rdf_store.as_ref()).map_err(|e| {
            tracing::warn!("RDF query for entity list failed");
            e
        })?;
        entity_uris.sort();

        let mut entities = Vec::new();

        for entity_uri in entity_uris {
            let entity_id = entity_id_from_uri(&entity_uri);
            let entity_type = query_first_object_value(
                rdf_store.as_ref(),
                &entity_uri,
                &format!("{GRAPHICA_NS}entityType"),
                None,
            )?;
            let domain = query_first_object_value(
                rdf_store.as_ref(),
                &entity_uri,
                &format!("{GRAPHICA_NS}domain"),
                None,
            )?;
            let created_at = query_first_object_value(
                rdf_store.as_ref(),
                &entity_uri,
                &format!("{GRAPHICA_NS}createdAt"),
                None,
            )?
            .unwrap_or_else(|| DEFAULT_ENTITY_CREATED_AT.to_string());

            if let Some(domain_filter) = query.domain.as_ref() {
                if domain.as_deref() != Some(domain_filter.as_str()) {
                    continue;
                }
            }

            let derived_attributes = fetch_derived_attributes(rdf_store.as_ref(), &entity_id)?;
            let attribute_count = derived_attributes.len();
            let avg_confidence = if attribute_count == 0 {
                0.0
            } else {
                derived_attributes
                    .iter()
                    .map(|attribute| attribute.confidence)
                    .sum::<f64>()
                    / attribute_count as f64
            };

            let (source_count, source_ids, fusion_rule, fusion_confidence, fusion_date) =
                fetch_fusion_metadata(rdf_store.as_ref(), &entity_id)?;

            entities.push(EntitySummary {
                id: entity_id,
                entity_type,
                domain,
                attribute_count,
                avg_confidence,
                status: "active".to_string(),
                created_at,
                source_count,
                source_ids,
                fusion_rule,
                fusion_confidence,
                fusion_date,
            });
        }

        entities.sort_by(|left, right| left.id.cmp(&right.id));
        let total = entities.len();
        entities.truncate(limit);

        return Ok(Json(EntityListResponse { entities, total }));
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

    if let Some(ref rdf_store) = state.rdf_store {
        let attributes = fetch_derived_attributes(rdf_store.as_ref(), &entity_id)?;
        let total = attributes.len();

        tracing::info!("Found {} attributes for entity {}", total, entity_id);

        return Ok(Json(EntityAttributesResponse {
            entity_id: entity_id.clone(),
            attributes,
            total,
        }));
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Get complete entity with all properties (including derived attributes)
pub async fn get_entity(
    State(state): State<Arc<ApiState>>,
    Path(entity_id): Path<String>,
) -> Result<Json<EntityResponse>, ApiError> {
    tracing::info!("Getting complete entity: {}", entity_id);

    if let Some(ref rdf_store) = state.rdf_store {
        let entity_uri = uris::entity(&entity_id);
        let property_rows = query_subject_predicate_objects(rdf_store.as_ref(), &entity_uri)?;

        if property_rows.is_empty() {
            return Err(ApiError::not_found(format!(
                "Entity {} not found",
                entity_id
            )));
        }

        let mut properties: HashMap<String, serde_json::Value> = HashMap::new();
        for (predicate, object) in property_rows {
            properties.insert(predicate, serde_json::Value::String(object));
        }

        let derived_attributes = fetch_derived_attributes(rdf_store.as_ref(), &entity_id)?;
        let (source_count, source_ids, fusion_rule, fusion_confidence, fusion_date) =
            fetch_fusion_metadata(rdf_store.as_ref(), &entity_id)?;

        return Ok(Json(EntityResponse {
            entity_id: entity_id.clone(),
            entity_type: properties
                .get(&format!("{GRAPHICA_NS}entityType"))
                .and_then(|value| value.as_str())
                .map(String::from),
            properties,
            derived_attributes,
            source_count,
            source_ids,
            fusion_rule,
            fusion_confidence,
            fusion_date,
        }));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_test_rdf_store() -> GraphicaRdfStore {
        let rdf_store = GraphicaRdfStore::new_in_memory().expect("in-memory rdf store");

        let current_graph_turtle = format!(
            r#"
@prefix gph: <{GRAPHICA_NS}> .
@prefix prov: <{PROV_NS}> .
@prefix rdf: <{RDF_NS}> .

<{GRAPHICA_NS}entity/customer-001> rdf:type gph:Entity ;
    gph:entityType "customer" ;
    gph:domain "sales" ;
    gph:createdAt "2026-04-08T00:00:00Z" ;
    gph:hasDerivedAttribute <{GRAPHICA_NS}attr/attr-001> .

<{GRAPHICA_NS}attr/attr-001> gph:attributeName "churn_score" ;
    gph:value "0.82" ;
    gph:confidence "0.97" ;
    prov:wasGeneratedBy <http://graphica.io/ml#model/churn-model> ;
    prov:generatedAtTime "1712534400" .
"#
        );

        let fusion_graph_turtle = format!(
            r#"
@prefix gph: <{GRAPHICA_NS}> .
@prefix prov: <{PROV_NS}> .

<{GRAPHICA_NS}fusion/fusion-001> gph:mergedEntity <{GRAPHICA_NS}entity/customer-001> ;
    gph:sourceEntity <{GRAPHICA_NS}entity/source-a> ;
    gph:sourceEntity <{GRAPHICA_NS}entity/source-b> ;
    gph:fusionRule "email_exact" ;
    gph:fusionConfidence "0.99" ;
    prov:atTime "1712534500" .
"#
        );

        rdf_store
            .load_turtle(&current_graph_turtle, None)
            .expect("seed current graph");
        rdf_store
            .load_turtle(&fusion_graph_turtle, Some(&NamedGraph::fusion()))
            .expect("seed fusion graph");

        rdf_store
    }

    #[test]
    fn derives_entity_summaries_from_simple_queries() {
        let rdf_store = seed_test_rdf_store();

        let entity_uris = query_entity_uris(&rdf_store).expect("entity list");
        assert_eq!(
            entity_uris,
            vec![format!("{GRAPHICA_NS}entity/customer-001")]
        );

        let entity_uri = &entity_uris[0];
        let entity_type = query_first_object_value(
            &rdf_store,
            entity_uri,
            &format!("{GRAPHICA_NS}entityType"),
            None,
        )
        .expect("entity type");
        let domain = query_first_object_value(
            &rdf_store,
            entity_uri,
            &format!("{GRAPHICA_NS}domain"),
            None,
        )
        .expect("entity domain");

        assert_eq!(entity_type.as_deref(), Some("customer"));
        assert_eq!(domain.as_deref(), Some("sales"));
    }

    #[test]
    fn fetches_derived_attributes_without_optional_sparql() {
        let rdf_store = seed_test_rdf_store();

        let attributes = fetch_derived_attributes(&rdf_store, "customer-001").expect("attributes");

        assert_eq!(attributes.len(), 1);
        assert_eq!(attributes[0].name, "churn_score");
        assert_eq!(attributes[0].value, "0.82");
        assert_eq!(attributes[0].confidence, 0.97);
        assert_eq!(attributes[0].model_id.as_deref(), Some("churn-model"));
        assert_eq!(attributes[0].timestamp, "1712534400");
    }

    #[test]
    fn fetches_fusion_metadata_from_fusion_graph() {
        let rdf_store = seed_test_rdf_store();

        let (source_count, source_ids, fusion_rule, fusion_confidence, fusion_date) =
            fetch_fusion_metadata(&rdf_store, "customer-001").expect("fusion metadata");

        assert_eq!(source_count, 2);
        assert_eq!(
            source_ids,
            Some(vec!["source-a".to_string(), "source-b".to_string()])
        );
        assert_eq!(fusion_rule.as_deref(), Some("email_exact"));
        assert_eq!(fusion_confidence, Some(0.99));
        assert_eq!(fusion_date.as_deref(), Some("1712534500"));
    }
}
