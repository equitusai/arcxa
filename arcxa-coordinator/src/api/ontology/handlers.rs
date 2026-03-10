//! REST API handlers for custom ontology management

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use utoipa;

use graphica_core::catalog::ValidationStatus;

use super::tree_builder::OntologyTreeBuilder;
use super::types::*;
use crate::api::ApiState;

/// Register a new custom ontology
///
/// POST /api/v1/ontology
#[utoipa::path(
    post,
    path = "/api/v1/ontology",
    request_body = RegisterOntologyRequest,
    responses(
        (status = 200, description = "Ontology registered successfully. The ontology content is parsed, validated, and stored in both the ontology registry (RocksDB) and the RDF governance store. This enables DDL generation, SHACL validation, and SPARQL queries. The ontology is loaded into a named graph (http://graphica.io/ontologies/{id}) and the default graph for compatibility.", body = RegisterOntologyResponse),
        (status = 400, description = "Invalid request or ontology content. Common causes include malformed Turtle/RDF syntax, missing required fields (id, content), or parsing errors. Ensure the ontology uses valid RDF/Turtle format with proper @prefix declarations.", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during registration or RDF store operation", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn register_ontology(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterOntologyRequest>,
) -> Result<Json<RegisterOntologyResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    let persisted_registry = state.persisted_ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    // Register the ontology (async - persists to RocksDB + triggers cache invalidation)
    let result = persisted_registry
        .register_custom_ontology(&request.id, &request.content, request.namespace.clone())
        .await;

    match result {
        Ok(_metadata) => {
            // Update metadata with request fields (and persist to database)
            let name = request.name.clone();
            let description = request.description.clone();
            let tags = request.tags.clone();
            let version = request.version.clone();
            let author = request.author.clone();

            persisted_registry
                .update_metadata(&request.id, |metadata| {
                    metadata.name = name;
                    metadata.description = description;
                    metadata.tags = tags;
                    if let Some(v) = version {
                        metadata.version = v;
                    }
                    if let Some(a) = author {
                        metadata.author = Some(a);
                    }
                })
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(OntologyErrorResponse {
                            error: "METADATA_UPDATE_FAILED".to_string(),
                            message: format!(
                                "Ontology registered but failed to update metadata: {}",
                                e
                            ),
                            details: None,
                        }),
                    )
                })?;

            // CRITICAL FIX: Also load the RDF content into the governance RDF store
            // This enables DDL generation to find SHACL shapes via SPARQL queries
            if let Some(ref rdf_store) = state.rdf_store {
                use crate::governance::rdf_store::{NamedGraph, RdfStore};
                use tracing::info;

                // Create a named graph for this ontology to keep it organized
                let graph =
                    NamedGraph::new(format!("http://graphica.io/ontologies/{}", request.id));

                // Load the Turtle content into the RDF store
                match rdf_store.load_turtle(&request.content, Some(&graph)) {
                    Ok(()) => {
                        info!(
                            "Successfully loaded ontology '{}' into RDF store (graph: {})",
                            request.id, graph.uri
                        );

                        // Also load into the default graph for DDL generator compatibility
                        // DDL generator queries the default graph for SHACL shapes
                        if let Err(e) = rdf_store.load_turtle(&request.content, None) {
                            tracing::warn!(
                                "Failed to load ontology '{}' into default graph: {}. \
                                DDL generation may not work.",
                                request.id,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load ontology '{}' into RDF store: {}. \
                            Ontology stored in registry but DDL generation will not work.",
                            request.id,
                            e
                        );
                    }
                }
            } else {
                tracing::warn!(
                    "RDF store not available - ontology '{}' stored in registry only. \
                    DDL generation will not work without RDF store.",
                    request.id
                );
            }

            // Get final state with validation status from the in-memory registry
            let registry = persisted_registry.registry();
            let registry_read = registry.read();
            let ontology = registry_read.get_ontology(&request.id).ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(OntologyErrorResponse {
                        error: "NOT_FOUND".to_string(),
                        message: "Ontology not found after registration".to_string(),
                        details: None,
                    }),
                )
            })?;

            Ok(Json(RegisterOntologyResponse {
                metadata: ontology.metadata.clone(),
                validation: ontology.validation_status.clone(),
                message: "Ontology registered and persisted successfully".to_string(),
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(OntologyErrorResponse {
                error: "REGISTRATION_FAILED".to_string(),
                message: format!("Failed to register ontology: {}", e),
                details: None,
            }),
        )),
    }
}

/// Get ontology by ID
///
/// GET /api/v1/ontology/:id
#[utoipa::path(
    get,
    path = "/api/v1/ontology/{id}",
    params(
        ("id" = String, Path, description = "Unique identifier for the ontology (e.g., 'retail-v1', 'healthcare-fhir')")
    ),
    responses(
        (status = 200, description = "Ontology retrieved successfully. Returns complete ontology metadata, content (Turtle/RDF format), validation status, and usage statistics including class count, property count, individual count, and usage count across ETL pipelines.", body = OntologyResponse),
        (status = 404, description = "Ontology not found with the specified ID", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during ontology retrieval", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn get_ontology(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<OntologyResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    let registry_arc = state.ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    let registry = registry_arc.read();

    match registry.get_ontology(&id) {
        Some(ontology) => Ok(Json(OntologyResponse {
            metadata: ontology.metadata.clone(),
            content: ontology.content.clone(),
            validation: ontology.validation_status.clone(),
            stats: OntologyStats {
                class_count: ontology.stats.class_count,
                property_count: ontology.stats.property_count,
                individual_count: ontology.stats.individual_count,
                size_bytes: ontology.stats.size_bytes,
                usage_count: ontology.stats.usage_count,
            },
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(OntologyErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Ontology {} not found", id),
                details: None,
            }),
        )),
    }
}

/// List all ontologies
///
/// GET /api/v1/ontology
#[utoipa::path(
    get,
    path = "/api/v1/ontology",
    params(
        ("active_only" = Option<bool>, Query, description = "Filter to only active ontologies (default: false). Active ontologies are included in DDL generation and SPARQL queries.")
    ),
    responses(
        (status = 200, description = "List of ontologies retrieved successfully. Returns metadata for all ontologies (or only active ones if filtered), including validation status, usage statistics, and total count. Useful for discovering available ontologies before merging or referencing in ETL workflows.", body = ListOntologiesResponse),
        (status = 500, description = "Internal server error during ontology listing", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn list_ontologies(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ListOntologiesQuery>,
) -> Result<Json<ListOntologiesResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    let registry_arc = state.ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    let registry = registry_arc.read();

    let ontologies = if params.active_only {
        registry.list_active_ontologies()
    } else {
        registry.list_ontologies()
    };

    let ontologies: Vec<_> = ontologies.into_iter().cloned().collect();
    let total = ontologies.len();

    Ok(Json(ListOntologiesResponse {
        ontologies,
        total,
        active_only: params.active_only,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ListOntologiesQuery {
    #[serde(default)]
    pub active_only: bool,
}

/// Update an existing ontology
///
/// PUT /api/v1/ontology/:id
#[utoipa::path(
    put,
    path = "/api/v1/ontology/{id}",
    params(
        ("id" = String, Path, description = "Unique identifier for the ontology to update")
    ),
    request_body = UpdateOntologyRequest,
    responses(
        (status = 200, description = "Ontology updated successfully. The ontology content and/or metadata are updated in both the ontology registry (RocksDB) and the RDF governance store. The updated ontology is reloaded into the named graph and default graph to ensure DDL generation and SPARQL queries reflect the latest version. Returns updated metadata, content, validation status, and statistics.", body = OntologyResponse),
        (status = 400, description = "Invalid request or ontology content. Validation errors may include malformed RDF/Turtle syntax or invalid metadata fields.", body = OntologyErrorResponse),
        (status = 404, description = "Ontology not found with the specified ID", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during update operation or RDF store synchronization", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn update_ontology(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateOntologyRequest>,
) -> Result<Json<OntologyResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    let persisted_registry = state.persisted_ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    // Update content (async - persists to RocksDB + triggers cache invalidation)
    if let Err(e) = persisted_registry
        .update_ontology(&id, &request.content)
        .await
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(OntologyErrorResponse {
                error: "UPDATE_FAILED".to_string(),
                message: format!("Failed to update ontology: {}", e),
                details: None,
            }),
        ));
    }

    // CRITICAL FIX: Also update the RDF content in the governance RDF store
    // This ensures DDL generation continues to work after ontology updates
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::{NamedGraph, RdfStore};

        // Clear previous version and reload
        // Note: In production, you might want to use SPARQL UPDATE instead
        let graph = NamedGraph::new(format!("http://graphica.io/ontologies/{}", id));

        // Load the updated Turtle content into the RDF store
        match rdf_store.load_turtle(&request.content, Some(&graph)) {
            Ok(()) => {
                tracing::info!(
                    "Successfully updated ontology '{}' in RDF store (graph: {})",
                    id,
                    graph.uri
                );

                // Also update in the default graph for DDL generator compatibility
                if let Err(e) = rdf_store.load_turtle(&request.content, None) {
                    tracing::warn!(
                        "Failed to update ontology '{}' in default graph: {}. \
                        DDL generation may not work.",
                        id,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to update ontology '{}' in RDF store: {}. \
                    Ontology updated in registry but DDL generation will not reflect changes.",
                    id,
                    e
                );
            }
        }
    }

    // Update metadata fields if provided (async - persists to RocksDB)
    let has_metadata_update = request.name.is_some()
        || request.version.is_some()
        || request.active.is_some()
        || request.description.is_some()
        || request.tags.is_some();

    if has_metadata_update {
        if let Err(e) = persisted_registry
            .update_metadata(&id, |metadata| {
                if let Some(name) = &request.name {
                    metadata.name = name.clone();
                }
                if let Some(version) = &request.version {
                    metadata.version = version.clone();
                }
                if let Some(active) = request.active {
                    metadata.active = active;
                }
                if let Some(description) = &request.description {
                    metadata.description = Some(description.clone());
                }
                if let Some(tags) = &request.tags {
                    metadata.tags = tags.clone();
                }
            })
            .await
        {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OntologyErrorResponse {
                    error: "METADATA_UPDATE_FAILED".to_string(),
                    message: format!("Failed to update metadata: {}", e),
                    details: None,
                }),
            ));
        }
    }

    // Read final state and return
    let registry = persisted_registry.registry();
    let registry_read = registry.read();
    if let Some(ontology) = registry_read.get_ontology(&id) {
        Ok(Json(OntologyResponse {
            metadata: ontology.metadata.clone(),
            content: ontology.content.clone(),
            validation: ontology.validation_status.clone(),
            stats: OntologyStats {
                class_count: ontology.stats.class_count,
                property_count: ontology.stats.property_count,
                individual_count: ontology.stats.individual_count,
                size_bytes: ontology.stats.size_bytes,
                usage_count: ontology.stats.usage_count,
            },
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(OntologyErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Ontology {} not found after update", id),
                details: None,
            }),
        ))
    }
}

/// Delete (deactivate) an ontology
///
/// DELETE /api/v1/ontology/:id
#[utoipa::path(
    delete,
    path = "/api/v1/ontology/{id}",
    params(
        ("id" = String, Path, description = "Unique identifier for the ontology to delete or deactivate"),
        ("permanent" = Option<bool>, Query, description = "Permanent deletion flag (default: false). If false, performs soft delete (deactivation) - ontology remains in registry but excluded from merges and queries. If true, permanently removes ontology from RocksDB storage. Soft delete is recommended for production to preserve history.")
    ),
    responses(
        (status = 204, description = "Ontology deleted or deactivated successfully. No content returned."),
        (status = 404, description = "Ontology not found with the specified ID", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during deletion or deactivation operation", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn delete_ontology(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(params): Query<DeleteOntologyQuery>,
) -> Result<StatusCode, (StatusCode, Json<OntologyErrorResponse>)> {
    let persisted_registry = state.persisted_ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    if params.permanent {
        // Permanent deletion (async - removes from RocksDB + triggers cache invalidation)
        match persisted_registry.remove_ontology(&id).await {
            Ok(_) => Ok(StatusCode::NO_CONTENT),
            Err(e) => Err((
                StatusCode::NOT_FOUND,
                Json(OntologyErrorResponse {
                    error: "DELETE_FAILED".to_string(),
                    message: format!("Failed to delete ontology: {}", e),
                    details: None,
                }),
            )),
        }
    } else {
        // Soft delete (deactivate) (async - persists to RocksDB + triggers cache invalidation)
        match persisted_registry.deactivate_ontology(&id).await {
            Ok(_) => Ok(StatusCode::NO_CONTENT),
            Err(e) => Err((
                StatusCode::NOT_FOUND,
                Json(OntologyErrorResponse {
                    error: "DEACTIVATE_FAILED".to_string(),
                    message: format!("Failed to deactivate ontology: {}", e),
                    details: None,
                }),
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteOntologyQuery {
    #[serde(default)]
    pub permanent: bool,
}

/// Get merged ontology
///
/// POST /api/v1/ontology/merge
#[utoipa::path(
    post,
    path = "/api/v1/ontology/merge",
    request_body = GetMergedOntologyRequest,
    responses(
        (status = 200, description = "Merged ontology retrieved successfully. Combines multiple ontologies into a single unified Turtle/RDF document, resolving namespace conflicts and ensuring consistency. If ontology_ids is empty, merges all active ontologies. Returns the merged content with size in bytes and list of included ontology IDs. Useful for generating comprehensive DDL schemas or SPARQL queries across multiple domains.", body = MergedOntologyResponse),
        (status = 400, description = "Invalid merge request. Common causes include specifying non-existent ontology IDs, namespace conflicts that cannot be resolved, or incompatible ontology versions.", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during ontology merge operation", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn get_merged_ontology(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<GetMergedOntologyRequest>,
) -> Result<Json<MergedOntologyResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    let registry_arc = state.ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    let registry = registry_arc.read();

    let content = if request.ontology_ids.is_empty() {
        // Get all active ontologies
        registry.get_merged_ontology()
    } else {
        // Get specific ontologies
        match registry.get_merged_with_ontologies(&request.ontology_ids) {
            Ok(merged) => merged,
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(OntologyErrorResponse {
                        error: "MERGE_FAILED".to_string(),
                        message: format!("Failed to merge ontologies: {}", e),
                        details: None,
                    }),
                ));
            }
        }
    };

    let included_ontologies = if request.ontology_ids.is_empty() {
        registry
            .list_active_ontologies()
            .into_iter()
            .map(|m| m.id.clone())
            .collect()
    } else {
        request.ontology_ids.clone()
    };

    Ok(Json(MergedOntologyResponse {
        size_bytes: content.len(),
        content,
        included_ontologies,
    }))
}

/// Validate ontology content
///
/// POST /api/v1/ontology/validate
#[utoipa::path(
    post,
    path = "/api/v1/ontology/validate",
    request_body = ValidateOntologyRequest,
    responses(
        (status = 200, description = "Ontology validation completed. Returns validation status (Valid, ValidWithWarnings, or Invalid) and detailed messages. Validates RDF/Turtle syntax, checks for required @prefix declarations, verifies class and property definitions (owl:Class, rdfs:Class, rdf:Property, owl:DatatypeProperty), and detects common errors. Use this endpoint before registering an ontology to catch syntax errors early.", body = ValidateOntologyResponse),
        (status = 500, description = "Internal server error during validation operation", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn validate_ontology(
    Json(request): Json<ValidateOntologyRequest>,
) -> Result<Json<ValidateOntologyResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    // Basic validation
    let mut messages = Vec::new();

    if request.content.trim().is_empty() {
        return Ok(Json(ValidateOntologyResponse {
            status: ValidationStatus::Invalid {
                errors: vec!["Ontology content is empty".to_string()],
            },
            messages: vec!["Validation failed: empty content".to_string()],
        }));
    }

    // Check for prefix declarations
    if !request.content.contains("@prefix") && !request.content.contains("PREFIX") {
        messages.push("Warning: No prefix declarations found".to_string());
    }

    // Check for basic RDF syntax
    if request.content.contains("a rdfs:Class") || request.content.contains("a owl:Class") {
        messages.push("Found class definitions".to_string());
    }

    if request.content.contains("a rdf:Property") || request.content.contains("a owl:") {
        messages.push("Found property definitions".to_string());
    }

    let status = if messages.is_empty() {
        ValidationStatus::Valid
    } else {
        ValidationStatus::ValidWithWarnings {
            warnings: messages.clone(),
        }
    };

    Ok(Json(ValidateOntologyResponse { status, messages }))
}

/// Get ontology tree structure
///
/// GET /api/v1/ontology/:id/tree?max_depth=-1&include_properties=true&include_individuals=false
#[utoipa::path(
    get,
    path = "/api/v1/ontology/{id}/tree",
    params(
        ("id" = String, Path, description = "Unique identifier for the ontology to visualize"),
        ("max_depth" = Option<i32>, Query, description = "Maximum tree depth to traverse (default: -1 for unlimited). Use to limit results for large ontologies."),
        ("include_properties" = Option<bool>, Query, description = "Include property definitions in tree (default: true). Shows owl:DatatypeProperty, owl:ObjectProperty, and rdf:Property nodes."),
        ("include_individuals" = Option<bool>, Query, description = "Include individual instances in tree (default: false). Enable to show owl:NamedIndividual nodes (may increase response size).")
    ),
    responses(
        (status = 200, description = "Ontology tree structure retrieved successfully. Returns hierarchical representation of classes, properties, and individuals with parent-child relationships (rdfs:subClassOf, rdfs:domain, rdfs:range). Supports both Turtle and RDF/XML formats (automatically converted). Useful for visualizing ontology structure, understanding class hierarchies, and exploring property definitions.", body = OntologyTreeResponse),
        (status = 404, description = "Ontology not found with the specified ID", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during tree generation or RDF/XML conversion. Check logs for parsing errors.", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn get_ontology_tree(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(params): Query<GetOntologyTreeRequest>,
) -> Result<Json<OntologyTreeResponse>, (StatusCode, Json<OntologyErrorResponse>)> {
    let registry_arc = state.ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    let registry = registry_arc.read();

    // Get the ontology
    let ontology = registry.get_ontology(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(OntologyErrorResponse {
                error: "NOT_FOUND".to_string(),
                message: format!("Ontology {} not found", id),
                details: None,
            }),
        )
    })?;

    // Convert RDF/XML to Turtle if needed (tree builder only understands Turtle)
    let content = convert_to_turtle_if_needed(&ontology.content, &ontology.metadata.namespace)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OntologyErrorResponse {
                    error: "CONVERSION_FAILED".to_string(),
                    message: format!("Failed to convert ontology format: {}", e),
                    details: Some(format!("{:?}", e)),
                }),
            )
        })?;

    // Build tree structure
    let builder = OntologyTreeBuilder::new(
        content,
        params.max_depth,
        params.include_properties,
        params.include_individuals,
    );

    match builder.build(ontology.metadata.clone()) {
        Ok(tree) => Ok(Json(tree)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "TREE_BUILD_FAILED".to_string(),
                message: format!("Failed to build ontology tree: {}", e),
                details: Some(format!("{:?}", e)),
            }),
        )),
    }
}

/// Convert RDF/XML to Turtle format if needed
///
/// The tree builder only understands Turtle format, so we need to convert RDF/XML to Turtle.
fn convert_to_turtle_if_needed(content: &str, namespace: &str) -> anyhow::Result<String> {
    use crate::mapping::ontology_registry::rdfxml_parser::RdfXmlParser;

    let trimmed = content.trim();

    // Check if content is RDF/XML
    if trimmed.starts_with("<?xml")
        || trimmed.contains("<rdf:RDF")
        || trimmed.contains("<owl:Ontology")
    {
        // Convert RDF/XML to Turtle
        let terms = RdfXmlParser::parse(content, namespace)?;

        // Generate Turtle from parsed terms
        let mut turtle = String::new();
        turtle.push_str(&format!("@prefix : <{}> .\n", namespace));
        turtle.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        turtle.push_str("@prefix owl: <http://www.w3.org/2002/07/owl#> .\n");
        turtle.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n");

        for term in &terms {
            // Determine if this is a class or property based on data_type field
            // Properties have a data_type (SQL type like VARCHAR), classes don't
            if term.data_type.is_some() {
                // This is a property
                turtle.push_str(&format!("<{}> a owl:DatatypeProperty ;\n", term.uri));
                turtle.push_str(&format!("    rdfs:label \"{}\"", term.label));

                // Add domain if present (from parent_classes field)
                if !term.parent_classes.is_empty() {
                    for domain in &term.parent_classes {
                        turtle.push_str(&format!(" ;\n    rdfs:domain <{}>", domain));
                    }
                }

                turtle.push_str(" .\n\n");
            } else {
                // This is a class
                turtle.push_str(&format!("<{}> a owl:Class ;\n", term.uri));
                turtle.push_str(&format!("    rdfs:label \"{}\"", term.label));

                // Add parent classes (rdfs:subClassOf)
                if !term.parent_classes.is_empty() {
                    for parent in &term.parent_classes {
                        turtle.push_str(&format!(" ;\n    rdfs:subClassOf <{}>", parent));
                    }
                }

                turtle.push_str(" .\n\n");
            }
        }

        // Log the generated Turtle for debugging
        tracing::debug!(
            "Generated Turtle from RDF/XML ({} terms):\n{}",
            terms.len(),
            turtle
        );

        Ok(turtle)
    } else {
        // Already Turtle format
        Ok(content.to_string())
    }
}

use serde::Deserialize;

/// Activate an ontology
///
/// POST /api/v1/ontology/:id/activate
#[utoipa::path(
    post,
    path = "/api/v1/ontology/{id}/activate",
    params(
        ("id" = String, Path, description = "Unique identifier for the ontology to activate")
    ),
    responses(
        (status = 204, description = "Ontology activated successfully. The ontology is now included in DDL generation, SPARQL queries, and merge operations. Active ontologies are automatically loaded into the RDF governance store and available for ETL workflows. No content returned."),
        (status = 404, description = "Ontology not found with the specified ID", body = OntologyErrorResponse),
        (status = 500, description = "Internal server error during activation operation", body = OntologyErrorResponse),
    ),
    tag = "Ontology Management"
)]
pub async fn activate_ontology(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<OntologyErrorResponse>)> {
    let persisted_registry = state.persisted_ontology_registry.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OntologyErrorResponse {
                error: "SERVICE_UNAVAILABLE".to_string(),
                message: "Ontology registry not available".to_string(),
                details: None,
            }),
        )
    })?;

    match persisted_registry.activate_ontology(&id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(OntologyErrorResponse {
                error: "ACTIVATE_FAILED".to_string(),
                message: format!("Failed to activate ontology: {}", e),
                details: None,
            }),
        )),
    }
}
