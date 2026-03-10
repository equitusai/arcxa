//! # Field Mapping API Handlers
//!
//! REST endpoints for the Advanced Field Mapping Engine.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::api::ApiState;
use crate::mapping::types::*;

/// POST /api/v1/mapping/analyze
///
/// Analyze a source schema and extract features from fields.
pub async fn analyze_schema(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<AnalyzeSchemaRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Analyzing schema: source={}, table={}",
        request.source_id, request.table_name
    );

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    match mapping_engine.analyze_schema(request).await {
        Ok(response) => {
            info!(
                "✓ Schema analyzed: {} fields in {}ms",
                response.fields.len(),
                response.processing_time_ms
            );
            Ok((StatusCode::OK, Json(response)))
        }
        Err(e) => {
            error!("Failed to analyze schema: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// GET /api/v1/mapping/fields/:field_id/candidates
///
/// Get mapping candidates for a field.
#[derive(Debug, Deserialize)]
pub struct GetCandidatesQuery {
    /// Maximum number of candidates to return (default: 10)
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Minimum confidence threshold (default: 0.5)
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
}

fn default_top_k() -> usize {
    10
}

fn default_min_confidence() -> f64 {
    0.5
}

pub async fn get_candidates(
    State(state): State<Arc<ApiState>>,
    Path(field_id): Path<String>,
    Query(params): Query<GetCandidatesQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Getting candidates for field: {} (top_k={}, min_confidence={})",
        field_id, params.top_k, params.min_confidence
    );

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    match mapping_engine
        .get_candidates(&field_id, params.top_k, params.min_confidence, None)
        .await
    {
        Ok(response) => {
            info!(
                "✓ Found {} candidates for field {}",
                response.candidates.len(),
                field_id
            );
            Ok((StatusCode::OK, Json(response)))
        }
        Err(e) => {
            error!("Failed to get candidates: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// POST /api/v1/mapping/feedback
///
/// Record user feedback on a mapping suggestion.
pub async fn record_feedback(
    State(state): State<Arc<ApiState>>,
    Json(feedback): Json<MappingFeedback>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Recording feedback for field {} by user {}",
        feedback.field_id, feedback.user_id
    );

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    match mapping_engine.record_feedback(feedback).await {
        Ok(_) => {
            info!("✓ Feedback recorded successfully");
            Ok((
                StatusCode::OK,
                Json(serde_json::json!({"status": "success"})),
            ))
        }
        Err(e) => {
            error!("Failed to record feedback: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// GET /api/v1/mapping/health
///
/// Health check for mapping engine.
pub async fn health_check(
    State(state): State<Arc<ApiState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let is_available = state.mapping_engine.is_some();

    // Check if semantic matcher is available
    let (semantic_available, phase_status) = if let Some(engine) = &state.mapping_engine {
        (engine.is_semantic_available(), engine.get_phase_status())
    } else {
        (false, "unavailable")
    };

    Ok((
        if is_available {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(serde_json::json!({
            "status": if is_available { "available" } else { "unavailable" },
            "phase": phase_status,
            "features": {
                "statistical_matcher": true,
                "semantic_matcher": semantic_available,
                "gnn_matcher": false,
                "symbolic_matcher": false
            }
        })),
    ))
}

// ============================================================================
// Mapping Session Workflow Handlers - Phase 1 Implementation
// ============================================================================

/// POST /api/v1/datasources/:source_id/analyze-for-mapping
///
/// Analyze a data source for mapping and create a mapping session.
pub async fn analyze_for_mapping(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
    Json(request): Json<AnalyzeForMappingRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Starting mapping analysis for source: {}, user: {}",
        source_id, request.user_id
    );

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    match mapping_engine
        .analyze_for_mapping(&source_id, request)
        .await
    {
        Ok(response) => {
            info!(
                "✓ Mapping analysis complete: session={}, {} fields, {} auto-approved",
                response.session_id, response.summary.total_fields, response.summary.auto_approved
            );
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            error!("Failed to analyze for mapping: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// GET /api/v1/mapping/sessions/:session_id
///
/// Get details of a mapping session.
pub async fn get_session(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("Getting mapping session: {}", session_id);

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    // Access storage through mapping engine
    match mapping_engine.storage.get_session(&session_id) {
        Ok(Some(session)) => {
            info!("✓ Found session: {}", session_id);
            Ok((StatusCode::OK, Json(session)))
        }
        Ok(None) => {
            info!("Session not found: {}", session_id);
            Err((
                StatusCode::NOT_FOUND,
                format!("Session not found: {}", session_id),
            ))
        }
        Err(e) => {
            error!("Failed to get session: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

/// POST /api/v1/mapping/sessions/:session_id/review
///
/// Review and update field mappings in a session.
pub async fn review_mappings(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
    Json(request): Json<ReviewMappingsRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Reviewing mappings for session: {}, {} updates, finalize={}",
        session_id,
        request.field_mappings.len(),
        request.finalize
    );

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    // Get current session
    let session = mapping_engine
        .storage
        .get_session(&session_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Session not found: {}", session_id),
            )
        })?;

    // Validate session is in PendingReview state
    if session.status != MappingSessionStatus::PendingReview
        && session.status != MappingSessionStatus::Draft
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Session is in {:?} state, cannot review", session.status),
        ));
    }

    // Process each field mapping update
    for update in &request.field_mappings {
        let (approval_status, selected_mapping) = match &update.action {
            ReviewAction::Approve => {
                // Find the field to get the top candidate
                let field_mapping = session
                    .tables
                    .iter()
                    .flat_map(|t| &t.field_mappings)
                    .find(|f| f.field_id == update.field_id)
                    .ok_or_else(|| {
                        (
                            StatusCode::NOT_FOUND,
                            format!("Field not found: {}", update.field_id),
                        )
                    })?;

                let selected = if let Some(selected_uri) = &update.selected_mapping {
                    // User selected a specific mapping
                    field_mapping
                        .candidates
                        .iter()
                        .find(|c| &c.ontology_term_uri == selected_uri)
                        .map(|c| SelectedMapping {
                            ontology_term_uri: c.ontology_term_uri.clone(),
                            confidence: c.confidence,
                            was_top_candidate: field_mapping
                                .candidates
                                .first()
                                .map(|t| &t.ontology_term_uri == selected_uri)
                                .unwrap_or(false),
                            transformation: c.transformation.clone(),
                        })
                        .ok_or_else(|| {
                            (
                                StatusCode::BAD_REQUEST,
                                format!("Selected mapping not found in candidates"),
                            )
                        })?
                } else {
                    // Approve top candidate
                    field_mapping
                        .candidates
                        .first()
                        .map(|c| SelectedMapping {
                            ontology_term_uri: c.ontology_term_uri.clone(),
                            confidence: c.confidence,
                            was_top_candidate: true,
                            transformation: c.transformation.clone(),
                        })
                        .ok_or_else(|| {
                            (
                                StatusCode::BAD_REQUEST,
                                "No candidates available".to_string(),
                            )
                        })?
                };

                (FieldApprovalStatus::Approved, Some(selected))
            }
            ReviewAction::Reject => (FieldApprovalStatus::Rejected, None),
            ReviewAction::Modify => {
                let selected_uri = update.selected_mapping.as_ref().ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Modified action requires selected_mapping".to_string(),
                    )
                })?;

                // Find the field to get candidates
                let field_mapping = session
                    .tables
                    .iter()
                    .flat_map(|t| &t.field_mappings)
                    .find(|f| f.field_id == update.field_id)
                    .ok_or_else(|| {
                        (
                            StatusCode::NOT_FOUND,
                            format!("Field not found: {}", update.field_id),
                        )
                    })?;

                let selected = field_mapping
                    .candidates
                    .iter()
                    .find(|c| &c.ontology_term_uri == selected_uri)
                    .map(|c| SelectedMapping {
                        ontology_term_uri: c.ontology_term_uri.clone(),
                        confidence: c.confidence,
                        was_top_candidate: false,
                        transformation: c.transformation.clone(),
                    })
                    .ok_or_else(|| {
                        (
                            StatusCode::BAD_REQUEST,
                            "Selected mapping not found in candidates".to_string(),
                        )
                    })?;

                (FieldApprovalStatus::Modified, Some(selected))
            }
        };

        // Update field mapping
        mapping_engine
            .storage
            .update_field_mapping(
                &session_id,
                &update.field_id,
                approval_status,
                selected_mapping,
                Some(request.reviewed_by.clone()),
                update.notes.clone(),
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // If finalizing, transition to Approved status
    if request.finalize {
        mapping_engine
            .storage
            .update_session_status(&session_id, MappingSessionStatus::Approved)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Get updated session
    let updated_session = mapping_engine
        .storage
        .get_session(&session_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Session disappeared".to_string(),
            )
        })?;

    let approved_count =
        updated_session.summary.auto_approved + updated_session.summary.user_approved;
    let ready_to_apply = updated_session.status == MappingSessionStatus::Approved;

    info!(
        "✓ Review complete: session={}, status={:?}, {} approved",
        session_id, updated_session.status, approved_count
    );

    Ok((
        StatusCode::OK,
        Json(ReviewMappingsResponse {
            status: updated_session.status,
            summary: updated_session.summary,
            approved_mappings: approved_count,
            ready_to_apply,
        }),
    ))
}

/// POST /api/v1/mapping/sessions/:session_id/apply
///
/// Apply approved mappings to RDF store.
pub async fn apply_mappings(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
    Json(_request): Json<ApplyMappingsRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("Applying mappings for session: {}", session_id);

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    // Get session
    let session = mapping_engine
        .storage
        .get_session(&session_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Session not found: {}", session_id),
            )
        })?;

    // Validate session is in Approved state
    if session.status != MappingSessionStatus::Approved {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Session must be in Approved state, currently {:?}",
                session.status
            ),
        ));
    }

    // Generate and store RDF triples to the governance brain
    let rdf_store = state.rdf_store.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "RDF store not initialized".to_string(),
        )
    })?;

    // Use a mapping-specific named graph
    use crate::governance::rdf_store::NamedGraph;
    let graph = NamedGraph::new(format!("http://graphica.io/graph/mappings/{}", session_id));

    let mut triples = Vec::new();

    // Generate ontology namespaces
    let gph_ns = "http://graphica.io/ontology#";
    let prov_ns = "http://www.w3.org/ns/prov#";
    let rdf_ns = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let xsd_ns = "http://www.w3.org/2001/XMLSchema#";

    // Create MappingSession resource
    let session_uri = format!("{}mapping/session/{}", gph_ns, session_id);

    triples.push((
        format!("<{}>", session_uri),
        format!("<{}type>", rdf_ns),
        format!("<{}MappingSession>", gph_ns),
    ));

    triples.push((
        format!("<{}>", session_uri),
        format!("<{}sessionId>", gph_ns),
        format!("\"{}\"^^<{}string>", session_id, xsd_ns),
    ));

    triples.push((
        format!("<{}>", session_uri),
        format!("<{}forDataSource>", gph_ns),
        format!("\"{}\"^^<{}string>", session.source_id, xsd_ns),
    ));

    triples.push((
        format!("<{}>", session_uri),
        format!("<{}createdAt>", gph_ns),
        format!(
            "\"{}\"^^<{}dateTime>",
            chrono::NaiveDateTime::from_timestamp_opt(session.created_at, 0)
                .map(|dt| dt.and_utc().to_rfc3339())
                .unwrap_or_default(),
            xsd_ns
        ),
    ));

    triples.push((
        format!("<{}>", session_uri),
        format!("<{}wasGeneratedBy>", prov_ns),
        format!("\"{}\"", session.created_by),
    ));

    // Generate FieldMapping resources for each approved mapping
    let mut field_mapping_count = 0;
    for table in &session.tables {
        for field_mapping in &table.field_mappings {
            if let Some(selected) = &field_mapping.selected_mapping {
                field_mapping_count += 1;

                let field_mapping_uri = format!(
                    "{}mapping/field/{}_{}_{}",
                    gph_ns, session.source_id, table.table_name, field_mapping.field_name
                );

                // Field mapping resource
                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}type>", rdf_ns),
                    format!("<{}FieldMapping>", gph_ns),
                ));

                // Link to session
                triples.push((
                    format!("<{}>", session_uri),
                    format!("<{}hasMapping>", gph_ns),
                    format!("<{}>", field_mapping_uri),
                ));

                // Source field metadata
                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}sourceTable>", gph_ns),
                    format!("\"{}\"^^<{}string>", table.table_name, xsd_ns),
                ));

                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}sourceField>", gph_ns),
                    format!("\"{}\"^^<{}string>", field_mapping.field_name, xsd_ns),
                ));

                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}dataType>", gph_ns),
                    format!("\"{}\"^^<{}string>", field_mapping.data_type, xsd_ns),
                ));

                // Mapping target
                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}mapsToOntologyTerm>", gph_ns),
                    format!("<{}>", selected.ontology_term_uri),
                ));

                // Confidence score
                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}confidence>", gph_ns),
                    format!("\"{}\"^^<{}double>", selected.confidence, xsd_ns),
                ));

                // Approval status
                let approval_str = format!("{:?}", field_mapping.approval_status).to_lowercase();
                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}approvalStatus>", gph_ns),
                    format!("\"{}\"^^<{}string>", approval_str, xsd_ns),
                ));

                // Was it the top candidate?
                triples.push((
                    format!("<{}>", field_mapping_uri),
                    format!("<{}wasTopCandidate>", gph_ns),
                    format!("\"{}\"^^<{}boolean>", selected.was_top_candidate, xsd_ns),
                ));

                // Optional transformation
                if let Some(transform) = &selected.transformation {
                    triples.push((
                        format!("<{}>", field_mapping_uri),
                        format!("<{}transformation>", gph_ns),
                        format!("\"{}\"^^<{}string>", transform, xsd_ns),
                    ));
                }

                // Review information if available
                if let Some(reviewer) = &field_mapping.reviewed_by {
                    triples.push((
                        format!("<{}>", field_mapping_uri),
                        format!("<{}reviewedBy>", gph_ns),
                        format!("\"{}\"^^<{}string>", reviewer, xsd_ns),
                    ));
                }

                if let Some(reviewed_at) = field_mapping.reviewed_at {
                    triples.push((
                        format!("<{}>", field_mapping_uri),
                        format!("<{}reviewedAt>", gph_ns),
                        format!(
                            "\"{}\"^^<{}dateTime>",
                            chrono::NaiveDateTime::from_timestamp_opt(reviewed_at, 0)
                                .map(|dt| dt.and_utc().to_rfc3339())
                                .unwrap_or_default(),
                            xsd_ns
                        ),
                    ));
                }

                // Optional notes
                if let Some(notes) = &field_mapping.notes {
                    triples.push((
                        format!("<{}>", field_mapping_uri),
                        format!("<{}notes>", gph_ns),
                        format!("\"{}\"^^<{}string>", notes.replace('"', "\\\""), xsd_ns),
                    ));
                }
            }
        }
    }

    // Store triples in batch
    use crate::governance::rdf_store::RdfStore;
    rdf_store
        .insert_triples(triples.clone(), Some(&graph))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to store RDF triples: {}", e),
            )
        })?;

    let triple_count = triples.len();
    info!(
        "✓ Stored {} RDF triples for {} field mappings",
        triple_count, field_mapping_count
    );

    // Update session status to Applied
    mapping_engine
        .storage
        .update_session_status(&session_id, MappingSessionStatus::Applied)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Transition to Active
    mapping_engine
        .storage
        .update_session_status(&session_id, MappingSessionStatus::Active)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    info!(
        "✓ Mappings applied: session={}, {} RDF triples stored",
        session_id, triple_count
    );

    Ok((
        StatusCode::OK,
        Json(ApplyMappingsResponse {
            status: MappingSessionStatus::Active,
            rdf_triples_stored: triple_count,
            ready_for_import: true,
            default_import_config: None,
        }),
    ))
}

/// POST /api/v1/mapping/sessions/:session_id/import
///
/// Import data using approved mappings.
pub async fn import_from_mappings(
    State(state): State<Arc<ApiState>>,
    Path(session_id): Path<String>,
    Json(request): Json<ImportDataRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        "Importing data for session: {}, user: {}",
        session_id, request.user_id
    );

    let mapping_engine = state.mapping_engine.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mapping engine not initialized".to_string(),
        )
    })?;

    // Execute import
    match mapping_engine.execute_import(&session_id, request).await {
        Ok(response) => {
            info!(
                "✓ Import complete: import_id={}, {} entities, {} triples",
                response.import_id, response.stats.entities_created, response.stats.triples_stored
            );
            Ok((StatusCode::OK, Json(response)))
        }
        Err(e) => {
            error!("Failed to import data: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_query_params() {
        assert_eq!(default_top_k(), 10);
        assert_eq!(default_min_confidence(), 0.5);
    }

    #[test]
    fn test_get_candidates_query_deserialization() {
        let json = r#"{"top_k": 20, "min_confidence": 0.7}"#;
        let query: GetCandidatesQuery = serde_json::from_str(json).unwrap();

        assert_eq!(query.top_k, 20);
        assert!((query.min_confidence - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_get_candidates_query_defaults() {
        let json = r#"{}"#;
        let query: GetCandidatesQuery = serde_json::from_str(json).unwrap();

        assert_eq!(query.top_k, 10);
        assert!((query.min_confidence - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_rdf_triple_generation_logic() {
        // Test triple format generation
        let gph_ns = "http://graphica.io/ontology#";
        let rdf_ns = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
        let xsd_ns = "http://www.w3.org/2001/XMLSchema#";

        let session_id = "session_test_123";
        let session_uri = format!("{}mapping/session/{}", gph_ns, session_id);

        // Verify URI format
        assert!(session_uri.starts_with("http://graphica.io/ontology#mapping/session/"));

        // Verify type triple format
        let type_triple = (
            session_uri.clone(),
            format!("{}type", rdf_ns),
            format!("{}MappingSession", gph_ns),
        );

        assert_eq!(
            type_triple.1,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
        );
        assert_eq!(type_triple.2, "http://graphica.io/ontology#MappingSession");

        // Verify literal with datatype format
        let string_literal = format!("\"{}\"^^{}string", session_id, xsd_ns);
        assert!(string_literal.contains("http://www.w3.org/2001/XMLSchema#string"));

        // Verify confidence score format
        let confidence = 0.82;
        let confidence_literal = format!("\"{}\"^^{}double", confidence, xsd_ns);
        assert!(confidence_literal.contains("0.82"));
        assert!(confidence_literal.contains("http://www.w3.org/2001/XMLSchema#double"));
    }
}
