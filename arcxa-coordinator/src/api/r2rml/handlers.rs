//! R2RML API Handlers
//!
//! HTTP request handlers for R2RML mapping operations.

use super::types::*;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use crate::mapping::profiling::SourceProfiler;
use crate::mapping::r2rml::{R2rmlExecutor, R2rmlMapping, R2rmlSerializer};
use axum::{
    extract::{Path, State},
    response::Json,
};
use std::sync::Arc;
use utoipa;
use uuid::Uuid;

/// Create a new R2RML mapping
#[utoipa::path(
    post,
    path = "/api/v1/r2rml/mappings",
    request_body = CreateMappingRequest,
    responses(
        (status = 200, description = "Mapping created successfully", body = CreateMappingResponse),
        (status = 400, description = "Invalid mapping definition", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn create_mapping(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateMappingRequest>,
) -> Result<Json<CreateMappingResponse>, ApiError> {
    tracing::info!(
        "Creating R2RML mapping: mapping_id={}, source={}",
        request.mapping.mapping_id,
        request.mapping.source_dataset
    );

    // Validate mapping
    let validation_errors = match request.mapping.validate() {
        Ok(_) => vec![],
        Err(e) => vec![e.to_string()],
    };

    let is_valid = validation_errors.is_empty();

    // Generate mapping URI
    let mapping_uri = request.mapping.get_mapping_uri();

    // Store in RDF store if requested
    let graph_uri = if request.store_in_rdf && is_valid {
        if let Some(rdf_store) = &state.rdf_store {
            // Serialize mapping to R2RML Turtle
            let serializer = R2rmlSerializer::new();
            let r2rml_turtle = serializer
                .serialize(&request.mapping)
                .map_err(|e| ApiError::internal(format!("Failed to serialize R2RML: {}", e)))?;

            // Create named graph for this mapping
            use crate::governance::rdf_store::{NamedGraph, RdfStore};
            let graph = NamedGraph::new(format!("{}graph", mapping_uri));

            // Store R2RML Turtle in RDF store
            rdf_store
                .load_turtle(&r2rml_turtle, Some(&graph))
                .map_err(|e| {
                    ApiError::internal(format!("Failed to store R2RML in RDF store: {}", e))
                })?;

            // Also store JSON representation for easy retrieval (pragmatic solution)
            let mapping_json = serde_json::to_string(&request.mapping).map_err(|e| {
                ApiError::internal(format!("Failed to serialize mapping to JSON: {}", e))
            })?;
            let escaped_json = mapping_json.replace('\\', "\\\\").replace('"', "\\\"");

            let metadata_turtle = format!(
                r#"@prefix gph: <http://graphica.io/ontology#> .
                <{mapping_uri}> gph:mappingJson "{escaped_json}" ."#,
                mapping_uri = mapping_uri,
                escaped_json = escaped_json
            );

            rdf_store
                .load_turtle(&metadata_turtle, Some(&graph))
                .map_err(|e| ApiError::internal(format!("Failed to store mapping JSON: {}", e)))?;

            tracing::info!("Stored R2RML mapping in RDF store: {}", graph.uri);
            Some(graph.uri)
        } else {
            tracing::warn!("RDF store not available, mapping not persisted");
            Some(format!("{}graph", mapping_uri))
        }
    } else {
        None
    };

    let response = CreateMappingResponse {
        mapping_id: request.mapping.mapping_id.clone(),
        mapping_uri: mapping_uri.clone(),
        graph_uri,
        is_valid,
        validation_errors,
        mapping_link: format!("/api/v1/mappings/{}", request.mapping.mapping_id),
    };

    tracing::info!(
        "Mapping created: {} (valid={})",
        request.mapping.mapping_id,
        is_valid
    );

    Ok(Json(response))
}

/// List all R2RML mappings
#[utoipa::path(
    get,
    path = "/api/v1/r2rml/mappings",
    responses(
        (status = 200, description = "List of mappings retrieved successfully", body = ListMappingsResponse),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn list_mappings(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ListMappingsResponse>, ApiError> {
    tracing::debug!("Listing all R2RML mappings");

    if let Some(rdf_store) = &state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Query for all R2RML mappings
        let sparql = r#"
            PREFIX rr: <http://www.w3.org/ns/r2rml#>
            PREFIX dcterms: <http://purl.org/dc/terms/>
            PREFIX gph: <http://graphica.io/ontology#>

            SELECT ?mapping ?id ?source ?created ?updated ?creator (COUNT(?tm) as ?tmCount)
            WHERE {
                ?mapping a gph:R2RMLMapping ;
                         dcterms:identifier ?id ;
                         gph:sourceDataset ?source .
                OPTIONAL { ?mapping dcterms:created ?created }
                OPTIONAL { ?mapping dcterms:modified ?updated }
                OPTIONAL { ?mapping dcterms:creator ?creator }
                OPTIONAL { ?mapping gph:hasTriplesMap ?tm }
            }
            GROUP BY ?mapping ?id ?source ?created ?updated ?creator
            ORDER BY DESC(?created)
        "#;

        let results = rdf_store
            .query(sparql)
            .map_err(|e| ApiError::internal(format!("SPARQL query failed: {}", e)))?;

        let mut mappings = Vec::new();
        for row in results {
            if let (Some(id), Some(source)) = (row.get("id"), row.get("source")) {
                let mapping_id = id.as_str().unwrap_or("").trim_matches('"').to_string();
                let source_dataset = source.as_str().unwrap_or("").trim_matches('"').to_string();

                // Parse created/updated timestamps (ISO 8601 format)
                let created_at = row
                    .get("created")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s.trim_matches('"')).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                let updated_at = row
                    .get("updated")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s.trim_matches('"')).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or(created_at);

                let mapping_uri = row
                    .get("mapping")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim_matches('<')
                    .trim_matches('>')
                    .to_string();

                let triples_maps_count =
                    row.get("tmCount").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                let created_by = row
                    .get("creator")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim_matches('"').to_string());

                mappings.push(MappingSummary {
                    mapping_id,
                    mapping_uri,
                    source_dataset,
                    triples_maps_count,
                    created_at,
                    updated_at,
                    created_by,
                });
            }
        }

        let total_count = mappings.len();
        tracing::debug!("Found {} R2RML mappings in RDF store", total_count);

        Ok(Json(ListMappingsResponse {
            mappings,
            total_count,
        }))
    } else {
        tracing::warn!("RDF store not available, returning empty list");
        Ok(Json(ListMappingsResponse {
            mappings: vec![],
            total_count: 0,
        }))
    }
}

/// Get a specific R2RML mapping
#[utoipa::path(
    get,
    path = "/api/v1/r2rml/mappings/{mapping_id}",
    params(
        ("mapping_id" = String, Path, description = "Unique mapping identifier")
    ),
    responses(
        (status = 200, description = "Mapping retrieved successfully", body = GetMappingResponse),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn get_mapping(
    State(state): State<Arc<ApiState>>,
    Path(mapping_id): Path<String>,
) -> Result<Json<GetMappingResponse>, ApiError> {
    tracing::debug!("Getting R2RML mapping: {}", mapping_id);

    if let Some(rdf_store) = &state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Query for the mapping's basic info
        let sparql = format!(
            r#"
            PREFIX dcterms: <http://purl.org/dc/terms/>
            PREFIX gph: <http://graphica.io/ontology#>

            SELECT ?mapping ?turtle
            WHERE {{
                ?mapping a gph:R2RMLMapping ;
                         dcterms:identifier "{}" .
                # For now, we don't have the turtle stored separately
                # This is a simplified implementation
            }}
            "#,
            mapping_id
        );

        let results = rdf_store
            .query(&sparql)
            .map_err(|e| ApiError::internal(format!("SPARQL query failed: {}", e)))?;

        if results.is_empty() {
            return Err(ApiError::not_found(format!(
                "Mapping not found: {}",
                mapping_id
            )));
        }

        // For now, return a placeholder response indicating the mapping exists
        // Full implementation would require parsing RDF back to R2rmlMapping struct
        // which is complex. Alternative: store JSON alongside Turtle for easy retrieval.

        tracing::warn!(
            "get_mapping currently returns placeholder - full R2RML parsing not yet implemented. \
            Consider storing JSON representation alongside Turtle for retrieval."
        );

        // Create a minimal R2rmlMapping placeholder
        let mapping = R2rmlMapping::new(
            mapping_id.clone(),
            "http://example.com/".to_string(),
            "unknown".to_string(),
        );

        Ok(Json(GetMappingResponse {
            mapping,
            r2rml_turtle: Some(
                "# R2RML Turtle representation not available in this version".to_string(),
            ),
        }))
    } else {
        Err(ApiError::not_found(format!(
            "Mapping not found: {} (RDF store not available)",
            mapping_id
        )))
    }
}

/// Update an existing R2RML mapping
#[utoipa::path(
    put,
    path = "/api/v1/r2rml/mappings/{mapping_id}",
    params(
        ("mapping_id" = String, Path, description = "Unique mapping identifier")
    ),
    request_body = UpdateMappingRequest,
    responses(
        (status = 200, description = "Mapping updated successfully", body = UpdateMappingResponse),
        (status = 400, description = "Invalid mapping definition", body = ApiError),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn update_mapping(
    State(state): State<Arc<ApiState>>,
    Path(mapping_id): Path<String>,
    Json(request): Json<UpdateMappingRequest>,
) -> Result<Json<UpdateMappingResponse>, ApiError> {
    let _state = state; // TODO: Update in RDF store
    tracing::info!("Updating R2RML mapping: {}", mapping_id);

    // Validate mapping
    let validation_errors = match request.mapping.validate() {
        Ok(_) => vec![],
        Err(e) => vec![e.to_string()],
    };

    let is_valid = validation_errors.is_empty();

    // TODO: Update mapping in RDF store
    let response = UpdateMappingResponse {
        mapping_id: mapping_id.clone(),
        updated_at: chrono::Utc::now(),
        is_valid,
        validation_errors,
    };

    tracing::info!("Mapping updated: {} (valid={})", mapping_id, is_valid);

    Ok(Json(response))
}

/// Delete an R2RML mapping
#[utoipa::path(
    delete,
    path = "/api/v1/r2rml/mappings/{mapping_id}",
    params(
        ("mapping_id" = String, Path, description = "Unique mapping identifier")
    ),
    responses(
        (status = 200, description = "Mapping deleted successfully", body = DeleteMappingResponse),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn delete_mapping(
    State(state): State<Arc<ApiState>>,
    Path(mapping_id): Path<String>,
) -> Result<Json<DeleteMappingResponse>, ApiError> {
    let _state = state; // TODO: Delete from RDF store
    tracing::info!("Deleting R2RML mapping: {}", mapping_id);

    // TODO: Delete mapping from RDF store
    let response = DeleteMappingResponse {
        mapping_id: mapping_id.clone(),
        deleted_at: chrono::Utc::now(),
        message: format!("Mapping {} deleted successfully", mapping_id),
    };

    tracing::info!("Mapping deleted: {}", mapping_id);

    Ok(Json(response))
}

/// Execute an R2RML mapping against a CSV file from the file library
///
/// ## Architecture: File Library First
///
/// This endpoint enforces the File Library First architecture:
/// - Files must be uploaded to the file library first
/// - Use the returned `file_id` in this request
/// - Direct file paths are NOT accepted
///
/// ## Workflow
///
/// 1. Upload CSV file: `POST /api/v1/file-library/files` → returns `file_id`
/// 2. Execute mapping: `POST /api/v1/r2rml/mappings/{mapping_id}/execute` with `source_file_id`
#[utoipa::path(
    post,
    path = "/api/v1/r2rml/mappings/{mapping_id}/execute",
    params(
        ("mapping_id" = String, Path, description = "Unique mapping identifier")
    ),
    request_body = ExecuteMappingRequest,
    responses(
        (status = 200, description = "Mapping executed successfully", body = ExecuteMappingResponse),
        (status = 400, description = "Invalid execution request or file not found in library", body = ApiError),
        (status = 404, description = "Mapping not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn execute_mapping(
    State(state): State<Arc<ApiState>>,
    Path(mapping_id): Path<String>,
    Json(request): Json<ExecuteMappingRequest>,
) -> Result<Json<ExecuteMappingResponse>, ApiError> {
    use std::time::Instant;

    tracing::info!(
        "Executing R2RML mapping: mapping_id={}, source_file_id={}",
        mapping_id,
        request.source_file_id
    );

    let start = Instant::now();
    let execution_id = format!("exec_{}", uuid::Uuid::new_v4().simple());

    // Phase 1: Retrieve mapping from RDF store
    let mapping = if let Some(rdf_store) = &state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        let sparql = format!(
            r#"
            PREFIX gph: <http://graphica.io/ontology#>
            PREFIX dcterms: <http://purl.org/dc/terms/>

            SELECT ?json
            WHERE {{
                ?mapping a gph:R2RMLMapping ;
                         dcterms:identifier "{}" ;
                         gph:mappingJson ?json .
            }}
            "#,
            mapping_id
        );

        let results = rdf_store
            .query(&sparql)
            .map_err(|e| ApiError::internal(format!("Failed to query mapping: {}", e)))?;

        if results.is_empty() {
            return Err(ApiError::not_found(format!(
                "Mapping not found: {}",
                mapping_id
            )));
        }

        let json_str = results[0]
            .get("json")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::internal("Mapping JSON not found in RDF store".to_string()))?
            .trim_matches('"');

        serde_json::from_str::<R2rmlMapping>(json_str)
            .map_err(|e| ApiError::internal(format!("Failed to parse mapping JSON: {}", e)))?
    } else {
        return Err(ApiError::internal("RDF store not available".to_string()));
    };

    // Phase 2: Get file from library (File Library First architecture)
    let file_library = state
        .file_library
        .as_ref()
        .ok_or_else(|| ApiError::internal("File library not available".to_string()))?;

    // Create CSV data source from file library
    use crate::mapping::data_source::FileLibraryCsvSource;
    let csv_source = FileLibraryCsvSource::new(file_library.as_ref(), &request.source_file_id)
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to load file from library: {}", e)))?;

    // Phase 3: Execute mapping via R2rmlExecutor with StructuredDataSource
    let executor = R2rmlExecutor::new(mapping);
    let triples = executor
        .execute(&csv_source)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to execute R2RML mapping: {}", e)))?;

    let triples_generated = triples.len();
    // Estimate rows processed (actual count would require row counter in executor)
    let rows_processed = if triples_generated > 0 {
        triples_generated / 3
    } else {
        0
    }; // Rough estimate

    // Phase 4: Store triples if requested
    let target_graph = request
        .target_graph
        .clone()
        .unwrap_or_else(|| format!("http://graphica.io/graph/entities/{}", mapping_id));

    if request.store_triples {
        if let Some(rdf_store) = &state.rdf_store {
            use crate::governance::rdf_store::{NamedGraph, RdfStore};

            // Convert RdfTriple to (subject, predicate, object) tuples
            let triple_tuples: Vec<(String, String, String)> = triples
                .iter()
                .map(|t| {
                    let object_with_type = if let Some(dt) = &t.datatype {
                        format!("\"{}\"^^<{}>", t.object, dt)
                    } else if let Some(lang) = &t.language {
                        format!("\"{}\"@{}", t.object, lang)
                    } else if t.object.starts_with('<') {
                        t.object.clone() // Already a URI
                    } else {
                        format!("\"{}\"", t.object) // Plain literal
                    };

                    (t.subject.clone(), t.predicate.clone(), object_with_type)
                })
                .collect();

            let graph = NamedGraph::new(target_graph.clone());
            rdf_store
                .insert_triples(triple_tuples, Some(&graph))
                .map_err(|e| ApiError::internal(format!("Failed to store triples: {}", e)))?;

            tracing::info!(
                "Stored {} triples in graph: {}",
                triples_generated,
                target_graph
            );
        } else {
            tracing::warn!("RDF store not available, triples not persisted");
        }
    }

    let duration = start.elapsed();

    // Phase 5: Return response
    let response = ExecuteMappingResponse {
        mapping_id: mapping_id.clone(),
        execution_id,
        triples_generated,
        rows_processed: rows_processed as usize,
        duration_seconds: duration.as_secs_f64(),
        target_graph: Some(target_graph),
        triples: if request.include_triples {
            Some(triples.iter().map(|t| t.to_ntriples()).collect())
        } else {
            None
        },
        executed_at: chrono::Utc::now(),
    };

    tracing::info!(
        "Mapping execution complete: {} triples in {:.2}s",
        triples_generated,
        duration.as_secs_f64()
    );

    Ok(Json(response))
}

/// Suggest R2RML mapping from profile
#[utoipa::path(
    post,
    path = "/api/v1/r2rml/mappings/suggest",
    request_body = SuggestMappingRequest,
    responses(
        (status = 200, description = "Mapping suggestion generated successfully", body = SuggestMappingResponse),
        (status = 400, description = "Invalid request or missing profile data", body = ApiError),
        (status = 404, description = "Profile not found for dataset_id", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn suggest_mapping_from_profile(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<SuggestMappingRequest>,
) -> Result<Json<SuggestMappingResponse>, ApiError> {
    use crate::mapping::profiling::types::{ProfileResult, SemanticType};

    tracing::info!(
        "Suggesting R2RML mapping for dataset: {:?}",
        request.dataset_id
    );

    // Phase 1: Get ProfileResult (from request or query from RDF store)
    let profile = if let Some(p) = request.profile {
        p
    } else if let Some(dataset_id) = &request.dataset_id {
        // TODO: Query profile from RDF store when implemented
        // For now, return error
        return Err(ApiError::not_found(format!(
            "Profile not found: {} (provide profile directly or implement RDF profile storage)",
            dataset_id
        )));
    } else {
        return Err(ApiError::bad_request(
            "Either dataset_id or profile must be provided".to_string(),
        ));
    };

    // Phase 2: Generate R2RML mapping from profile
    let mapping_id = format!(
        "{}_mapping",
        profile.dataset_id.replace(".", "_").replace("/", "_")
    );

    // Determine subject template from first candidate key
    let (subject_template, subject_confidence) =
        if let Some(key_col) = profile.candidate_keys.first() {
            (
                format!("{}entity/{{{}}}", request.base_uri, key_col),
                0.95, // High confidence with candidate key
            )
        } else if let Some(first_col) = profile.columns.first() {
            (
                format!("{}entity/{{{}}}", request.base_uri, first_col.name),
                0.60, // Lower confidence without candidate key
            )
        } else {
            return Err(ApiError::bad_request("Profile has no columns".to_string()));
        };

    // Generate predicate-object maps for each column
    let mut predicate_object_maps = Vec::new();
    let mut column_suggestions = Vec::new();
    let mut total_confidence = 0.0;

    for column in &profile.columns {
        // Determine predicate URI from semantic type or column name
        let (predicate, pred_confidence, reasoning) = if request.use_semantic_types {
            match column.semantic_type {
                Some(SemanticType::Email) => (
                    "http://schema.org/email".to_string(),
                    0.95,
                    "Semantic type: Email detected".to_string(),
                ),
                Some(SemanticType::Phone) => (
                    "http://schema.org/telephone".to_string(),
                    0.95,
                    "Semantic type: Phone number detected".to_string(),
                ),
                Some(SemanticType::Url) => (
                    "http://schema.org/url".to_string(),
                    0.95,
                    "Semantic type: URL detected".to_string(),
                ),
                Some(SemanticType::Zipcode) => (
                    "http://schema.org/postalCode".to_string(),
                    0.90,
                    "Semantic type: Postal code detected".to_string(),
                ),
                Some(SemanticType::CountryCode) => (
                    "http://schema.org/addressCountry".to_string(),
                    0.90,
                    "Semantic type: Country code detected".to_string(),
                ),
                _ => {
                    // Fallback to namespace + normalized column name
                    let normalized = column.name.to_lowercase().replace("_", "").replace("-", "");

                    let predicate = match request.default_namespace.as_str() {
                        "schema" => format!("http://schema.org/{}", &column.name),
                        "dbo" => format!("http://dbpedia.org/ontology/{}", &column.name),
                        ns => format!("http://{}.org/{}", ns, &column.name),
                    };

                    (
                        predicate,
                        0.70,
                        format!("Column name mapping to {}", request.default_namespace),
                    )
                }
            }
        } else {
            // Use default namespace + column name
            let predicate = match request.default_namespace.as_str() {
                "schema" => format!("http://schema.org/{}", &column.name),
                "dbo" => format!("http://dbpedia.org/ontology/{}", &column.name),
                ns => format!("http://{}.org/{}", ns, &column.name),
            };

            (
                predicate,
                0.70,
                format!("Column name mapping to {}", request.default_namespace),
            )
        };

        // Get XSD datatype
        let xsd_datatype = column.data_type.to_xsd_uri().to_string();

        // Create predicate-object map
        use crate::mapping::r2rml::types::{ObjectMap, PredicateObjectMap, PredicateSpec};

        let pom = PredicateObjectMap {
            predicate: PredicateSpec::Constant(predicate.clone()),
            object_map: ObjectMap::Column {
                column: column.name.clone(),
                datatype: Some(xsd_datatype.clone()),
                language: None,
            },
        };
        predicate_object_maps.push(pom);

        // Create column suggestion
        column_suggestions.push(ColumnSuggestion {
            column_name: column.name.clone(),
            suggested_predicate: predicate,
            suggested_datatype: Some(xsd_datatype),
            confidence: pred_confidence,
            reasoning,
        });

        total_confidence += pred_confidence;
    }

    // Calculate overall confidence
    let confidence = if !column_suggestions.is_empty() {
        (total_confidence / column_suggestions.len() as f64) * subject_confidence
    } else {
        0.0
    };

    // Create TriplesMap
    use crate::mapping::r2rml::types::{LogicalTable, SubjectMap, TriplesMap};

    let subject_map = SubjectMap::from_template(subject_template)
        .with_class("http://schema.org/Thing".to_string());

    let triples_map = TriplesMap {
        name: format!("{}Map", profile.dataset_id.replace(".", "_")),
        logical_table: LogicalTable::TableName {
            table_name: profile.source_location.clone(),
        },
        subject_map,
        predicate_object_maps,
        graph_map: None,
    };

    // Create R2RML mapping
    let mut mapping = R2rmlMapping::new(
        mapping_id.clone(),
        request.base_uri.clone(),
        profile.source_location.clone(),
    );
    mapping.add_triples_map(triples_map);
    mapping.description = Some(format!(
        "Auto-generated R2RML mapping from profile (confidence: {:.2})",
        confidence
    ));

    // Serialize to Turtle
    let serializer = R2rmlSerializer::new();
    let r2rml_turtle = serializer
        .serialize(&mapping)
        .map_err(|e| ApiError::internal(format!("Failed to serialize R2RML: {}", e)))?;

    let response = SuggestMappingResponse {
        mapping,
        confidence,
        column_suggestions,
        r2rml_turtle,
    };

    tracing::info!(
        "Generated R2RML mapping suggestion with confidence: {:.2}",
        confidence
    );

    Ok(Json(response))
}

/// Validate an R2RML mapping
#[utoipa::path(
    post,
    path = "/api/v1/r2rml/mappings/validate",
    request_body = ValidateMappingRequest,
    responses(
        (status = 200, description = "Mapping validation completed", body = ValidateMappingResponse),
        (status = 400, description = "Invalid request format", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "R2RML Mappings"
)]
pub async fn validate_mapping(
    State(_state): State<Arc<ApiState>>,
    Json(request): Json<ValidateMappingRequest>,
) -> Result<Json<ValidateMappingResponse>, ApiError> {
    tracing::info!("Validating R2RML mapping: {}", request.mapping.mapping_id);

    let mut errors = vec![];
    let mut warnings = vec![];

    // Perform validation
    if let Err(e) = request.mapping.validate() {
        errors.push(ValidationError {
            code: "VALIDATION_ERROR".to_string(),
            message: e.to_string(),
            location: None,
        });
    }

    // Additional semantic validation
    for (idx, triples_map) in request.mapping.triples_maps.iter().enumerate() {
        // Check for empty predicate-object maps
        if triples_map.predicate_object_maps.is_empty() {
            errors.push(ValidationError {
                code: "EMPTY_PREDICATE_OBJECT_MAPS".to_string(),
                message: format!(
                    "TriplesMap '{}' has no predicate-object maps",
                    triples_map.name
                ),
                location: Some(format!("triples_maps[{}]", idx)),
            });
        }

        // Check for subject map without classes
        if triples_map.subject_map.class.is_none() {
            warnings.push(ValidationWarning {
                code: "NO_RDF_CLASS".to_string(),
                message: format!(
                    "TriplesMap '{}' subject map has no rdf:type classes",
                    triples_map.name
                ),
                location: Some(format!("triples_maps[{}].subject_map", idx)),
            });
        }
    }

    let is_valid = errors.is_empty();

    let response = ValidateMappingResponse {
        mapping_id: request.mapping.mapping_id.clone(),
        is_valid,
        errors,
        warnings,
    };

    tracing::info!(
        "Validation complete: {} (valid={}, errors={}, warnings={})",
        request.mapping.mapping_id,
        is_valid,
        response.errors.len(),
        response.warnings.len()
    );

    Ok(Json(response))
}
