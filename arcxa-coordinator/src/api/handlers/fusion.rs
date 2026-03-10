//! Fusion Handler Functions
//!
//! HTTP handlers for entity fusion, resolution, and candidate management.

use crate::api::dto::*;
use crate::api::validators::{validate_entity_count, validate_match_rule};
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;

// ============================================================================
// Public Handler Functions
// ============================================================================

/// Resolve entity fusion (entity resolution)
pub async fn resolve_entity_fusion(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<FusionResolveRequest>,
) -> Result<Json<FusionResolveResponse>, ApiError> {
    tracing::info!("Resolving entity fusion with rule: {}", request.rule);

    // Validate match rule is supported
    validate_match_rule(&request.rule)?;

    // Validate entity count (2-100 entities)
    validate_entity_count(&request.entities)?;

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Extract entity IDs and validate they exist in RDF
        let entity_ids: Vec<String> = request
            .entities
            .iter()
            .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();

        if entity_ids.len() != request.entities.len() {
            return Err(ApiError::bad_request(
                "All entities must have an 'id' field".to_string(),
            ));
        }

        validate_entities_exist(rdf_store, &entity_ids).await?;

        // Generate fusion ID
        let fusion_id = format!("fus_{}", uuid::Uuid::new_v4());
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Determine merged entity (first entity in list)
        let merged_entity_id = request
            .entities
            .first()
            .and_then(|e| e.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Missing entity id".to_string()))?;

        // Create fusion operation triples
        let mut triples = Vec::new();

        // Fusion operation metadata
        triples.push(format!(
            "<{}/fusion/{}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{}/FusionOperation> .",
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            crate::governance::ontology::GRAPHICA_NS
        ));

        triples.push(format!(
            "<{}/fusion/{}> <{}/mergedEntity> <{}/entity/{}> .",
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::GRAPHICA_NS,
            merged_entity_id
        ));

        triples.push(format!(
            "<{}/fusion/{}> <{}/fusionRule> \"{}\" .",
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            crate::governance::ontology::GRAPHICA_NS,
            request.rule
        ));

        triples.push(format!(
            "<{}/fusion/{}> <{}/fusionConfidence> \"{}\"^^<http://www.w3.org/2001/XMLSchema#double> .",
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            crate::governance::ontology::GRAPHICA_NS,
            request.confidence.unwrap_or(0.95)
        ));

        triples.push(format!(
            "<{}/fusion/{}> <{}/atTime> \"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime> .",
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            crate::governance::ontology::PROV_NS,
            timestamp
        ));

        // Add source entities (skip first one as it's the merged entity)
        for entity in request.entities.iter().skip(1) {
            if let Some(id) = entity.get("id").and_then(|v| v.as_str()) {
                triples.push(format!(
                    "<{}/fusion/{}> <{}/sourceEntity> <{}/entity/{}> .",
                    crate::governance::ontology::GRAPHICA_NS,
                    fusion_id,
                    crate::governance::ontology::GRAPHICA_NS,
                    crate::governance::ontology::GRAPHICA_NS,
                    id
                ));
            }
        }

        // Insert triples into fusion named graph
        let insert_query = format!(
            r#"
PREFIX gph: <{}>
PREFIX prov: <{}>

INSERT DATA {{
    GRAPH <http://graphica.io/graph/fusion> {{
        {}
    }}
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::PROV_NS,
            triples.join("\n        ")
        );

        match rdf_store.as_ref().update(&insert_query) {
            Ok(_) => {
                tracing::info!("Created fusion operation: {}", fusion_id);
                return Ok(Json(FusionResolveResponse {
                    fusion_id: fusion_id.clone(),
                    merged_entity_id: merged_entity_id.to_string(),
                    source_entity_ids: request
                        .entities
                        .iter()
                        .skip(1)
                        .filter_map(|e| e.get("id").and_then(|v| v.as_str()).map(String::from))
                        .collect(),
                    rule: request.rule,
                    confidence: request.confidence.unwrap_or(0.95),
                    created_at: timestamp,
                }));
            }
            Err(e) => {
                tracing::error!("Failed to insert fusion triples: {}", e);
                return Err(ApiError::internal(format!("Fusion failed: {}", e)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Reverse entity fusion (mark as deleted)
pub async fn reverse_entity_fusion(
    State(state): State<Arc<ApiState>>,
    Path(fusion_id): Path<String>,
    Json(request): Json<ReverseFusionRequest>,
) -> Result<Json<ReverseFusionResponse>, ApiError> {
    tracing::info!("Reversing fusion: {}", fusion_id);

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        let timestamp = chrono::Utc::now().to_rfc3339();
        let reason = request
            .reason
            .unwrap_or_else(|| "Manual reversal".to_string());

        // Add reversal metadata to fusion operation
        let update_query = format!(
            r#"
PREFIX gph: <{}>

INSERT DATA {{
    GRAPH <http://graphica.io/graph/fusion> {{
        <{}/fusion/{}> gph:reversedAt "{}"^^<http://www.w3.org/2001/XMLSchema#dateTime> .
        <{}/fusion/{}> gph:reversalReason "{}" .
    }}
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            timestamp,
            crate::governance::ontology::GRAPHICA_NS,
            fusion_id,
            reason
        );

        match rdf_store.as_ref().update(&update_query) {
            Ok(_) => {
                tracing::info!("Reversed fusion operation: {}", fusion_id);
                return Ok(Json(ReverseFusionResponse {
                    fusion_id: fusion_id.clone(),
                    reversed: true,
                    reversed_at: timestamp,
                    reason: Some(reason),
                }));
            }
            Err(e) => {
                tracing::error!("Failed to reverse fusion: {}", e);
                return Err(ApiError::internal(format!("Reversal failed: {}", e)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Phase 1: Propose fusion candidates based on matching rules
pub async fn propose_fusion_candidates(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ProposeFusionRequest>,
) -> Result<Json<ProposeFusionResponse>, ApiError> {
    tracing::info!(
        "Proposing fusion candidates for dataset: {}, rule: {}",
        request.dataset,
        request.rule
    );

    // Validate match rule is supported
    validate_match_rule(&request.rule)?;

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Validate dataset exists and has entities
        let entity_count = validate_dataset_exists(rdf_store, &request.dataset).await?;
        tracing::info!(
            "Dataset '{}' has {} entities",
            request.dataset,
            entity_count
        );

        // Query entities from the dataset
        let query = format!(
            r#"
PREFIX gph: <{}>

SELECT ?entity ?attr ?value
WHERE {{
    ?entity a gph:Entity ;
            gph:dataset "{}" ;
            ?attr ?value .
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            request.dataset
        );

        match rdf_store.as_ref().query(&query) {
            Ok(results) => {
                // Group entities by their field values for matching
                let mut candidates = Vec::new();
                let mut entity_map: std::collections::HashMap<
                    String,
                    serde_json::Map<String, serde_json::Value>,
                > = std::collections::HashMap::new();

                // Parse SPARQL results and group by entity
                for row in results {
                    if let (Some(entity_uri), Some(attr), Some(value)) = (
                        row.get("entity").and_then(|v| v.as_str()),
                        row.get("attr").and_then(|v| v.as_str()),
                        row.get("value"),
                    ) {
                        let entity_id = entity_uri.split('/').last().unwrap_or("");
                        let entry = entity_map.entry(entity_id.to_string()).or_insert_with(|| {
                            let mut map = serde_json::Map::new();
                            map.insert(
                                "id".to_string(),
                                serde_json::Value::String(entity_id.to_string()),
                            );
                            map
                        });

                        if let Some(attr_name) = attr.split('#').last() {
                            entry.insert(attr_name.to_string(), value.clone());
                        }
                    }
                }

                // Apply matching rule to find candidates
                let match_field = &request.rule; // e.g., "email", "phone", etc.
                let mut field_groups: std::collections::HashMap<
                    String,
                    Vec<serde_json::Map<String, serde_json::Value>>,
                > = std::collections::HashMap::new();

                for (_, entity) in entity_map {
                    if let Some(field_value) = entity.get(match_field).and_then(|v| v.as_str()) {
                        field_groups
                            .entry(field_value.to_string())
                            .or_insert_with(Vec::new)
                            .push(entity);
                    }
                }

                // Generate candidate groups (only groups with 2+ entities)
                for (match_value, entities) in field_groups {
                    if entities.len() >= 2 {
                        // Check if duplicate proposal already exists
                        match check_duplicate_proposal(
                            rdf_store,
                            &request.dataset,
                            &request.rule,
                            &match_value,
                        )
                        .await
                        {
                            Ok(true) => {
                                tracing::info!(
                                    "Skipping duplicate proposal for {}={}",
                                    request.rule,
                                    match_value
                                );
                                continue;
                            }
                            Ok(false) => {
                                // Not a duplicate, proceed
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Duplicate check failed, proceeding anyway: {:?}",
                                    e
                                );
                            }
                        }

                        let candidate_id = format!("cand_{}", uuid::Uuid::new_v4());
                        candidates.push(FusionCandidate {
                            candidate_id: candidate_id.clone(),
                            entities: entities.clone(),
                            match_rule: request.rule.clone(),
                            match_value: match_value.clone(),
                            confidence: calculate_match_confidence(&request.rule, &entities),
                            proposed_at: chrono::Utc::now().to_rfc3339(),
                            status: "proposed".to_string(),
                        });

                        // Store candidate in RDF for staging
                        let candidate_triples = format_fusion_candidate_triples(
                            &candidate_id,
                            &entities,
                            &request.rule,
                            &match_value,
                        );
                        let insert_query = format!(
                            r#"
INSERT DATA {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        {}
    }}
}}
"#,
                            candidate_triples
                        );

                        if let Err(e) = rdf_store.as_ref().update(&insert_query) {
                            tracing::warn!("Failed to store candidate {}: {}", candidate_id, e);
                        }
                    }
                }

                let total = candidates.len();
                tracing::info!("Proposed {} fusion candidates", total);
                return Ok(Json(ProposeFusionResponse {
                    candidates,
                    total_count: total,
                }));
            }
            Err(e) => {
                tracing::error!("Failed to query entities: {}", e);
                return Err(ApiError::internal(format!("Query failed: {}", e)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Phase 2: List staged fusion candidates
pub async fn list_fusion_candidates(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<FusionCandidateQuery>,
) -> Result<Json<FusionCandidateListResponse>, ApiError> {
    tracing::info!("Listing fusion candidates (status: {:?})", params.status);

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Query candidates from RDF
        let status_filter = params.status.as_deref().unwrap_or("proposed");
        let query = format!(
            r#"
PREFIX gph: <{}>

SELECT ?candidate ?rule ?value ?confidence ?proposedAt ?status
WHERE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        ?candidate a gph:FusionCandidate ;
                   gph:matchRule ?rule ;
                   gph:matchValue ?value ;
                   gph:confidence ?confidence ;
                   gph:proposedAt ?proposedAt ;
                   gph:status "{}" .
    }}
}}
LIMIT {}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            status_filter,
            params.limit.unwrap_or(100)
        );

        match rdf_store.as_ref().query(&query) {
            Ok(results) => {
                let mut candidates = Vec::new();

                for row in results {
                    if let (
                        Some(candidate_id),
                        Some(rule),
                        Some(value),
                        Some(confidence),
                        Some(proposed_at),
                    ) = (
                        row.get("candidate").and_then(|v| v.as_str()),
                        row.get("rule").and_then(|v| v.as_str()),
                        row.get("value").and_then(|v| v.as_str()),
                        row.get("confidence").and_then(|v| v.as_f64()),
                        row.get("proposedAt").and_then(|v| v.as_str()),
                    ) {
                        let id = candidate_id.split('/').last().unwrap_or("");

                        // Query entities for this candidate
                        let entities_query = format!(
                            r#"
PREFIX gph: <{}>

SELECT ?entity
WHERE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}> gph:hasEntity ?entity .
    }}
}}
"#,
                            crate::governance::ontology::GRAPHICA_NS,
                            candidate_id
                        );

                        let entities =
                            if let Ok(entity_results) = rdf_store.as_ref().query(&entities_query) {
                                entity_results
                                    .iter()
                                    .filter_map(|r| r.get("entity").and_then(|v| v.as_str()))
                                    .map(|uri| {
                                        let mut map = serde_json::Map::new();
                                        map.insert(
                                            "id".to_string(),
                                            serde_json::Value::String(
                                                uri.split('/').last().unwrap_or("").to_string(),
                                            ),
                                        );
                                        map
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };

                        candidates.push(FusionCandidate {
                            candidate_id: id.to_string(),
                            entities,
                            match_rule: rule.to_string(),
                            match_value: value.to_string(),
                            confidence,
                            proposed_at: proposed_at.to_string(),
                            status: status_filter.to_string(),
                        });
                    }
                }

                let total = candidates.len();
                return Ok(Json(FusionCandidateListResponse {
                    candidates,
                    total_count: total,
                }));
            }
            Err(e) => {
                tracing::error!("Failed to list candidates: {}", e);
                return Err(ApiError::internal(format!("Query failed: {}", e)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Phase 2.5: Approve a fusion candidate
pub async fn approve_fusion_candidate(
    State(state): State<Arc<ApiState>>,
    Path(candidate_id): Path<String>,
    Json(request): Json<ReviewCandidateRequest>,
) -> Result<Json<ReviewCandidateResponse>, ApiError> {
    tracing::info!("Approving fusion candidate: {}", candidate_id);

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Validate candidate exists and is in "proposed" state
        validate_candidate_state(rdf_store, &candidate_id, "proposed").await?;

        let timestamp = chrono::Utc::now().to_rfc3339();
        let reviewer = request.reviewer.unwrap_or_else(|| "system".to_string());

        // Update candidate status to "approved"
        let update_query = format!(
            r#"
PREFIX gph: <{}>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

DELETE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> gph:status ?oldStatus .
    }}
}}
INSERT {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> gph:status "approved" .
        <{}/fusion/candidate/{}> gph:reviewedBy <{}/user/{}> .
        <{}/fusion/candidate/{}> gph:reviewedAt "{}"^^xsd:dateTime .
        <{}/fusion/candidate/{}> gph:reviewNotes "{}" .
    }}
}}
WHERE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> gph:status ?oldStatus .
        FILTER(?oldStatus = "proposed")
    }}
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            crate::governance::ontology::GRAPHICA_NS,
            reviewer,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            timestamp,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            request.notes.unwrap_or_default(),
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id
        );

        match rdf_store.as_ref().update(&update_query) {
            Ok(_) => {
                return Ok(Json(ReviewCandidateResponse {
                    candidate_id,
                    status: "approved".to_string(),
                    reviewed_by: reviewer,
                    reviewed_at: timestamp,
                }));
            }
            Err(e) => {
                tracing::error!("Failed to approve candidate: {}", e);
                return Err(ApiError::internal(format!("Approval failed: {}", e)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

/// Phase 2.5: Reject a fusion candidate
pub async fn reject_fusion_candidate(
    State(state): State<Arc<ApiState>>,
    Path(candidate_id): Path<String>,
    Json(request): Json<ReviewCandidateRequest>,
) -> Result<Json<ReviewCandidateResponse>, ApiError> {
    tracing::info!("Rejecting fusion candidate: {}", candidate_id);

    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // Validate candidate exists and is in "proposed" state
        validate_candidate_state(rdf_store, &candidate_id, "proposed").await?;

        let timestamp = chrono::Utc::now().to_rfc3339();
        let reviewer = request.reviewer.unwrap_or_else(|| "system".to_string());

        // Update candidate status to "rejected"
        let update_query = format!(
            r#"
PREFIX gph: <{}>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

DELETE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> gph:status ?oldStatus .
    }}
}}
INSERT {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> gph:status "rejected" .
        <{}/fusion/candidate/{}> gph:reviewedBy <{}/user/{}> .
        <{}/fusion/candidate/{}> gph:reviewedAt "{}"^^xsd:dateTime .
        <{}/fusion/candidate/{}> gph:reviewNotes "{}" .
    }}
}}
WHERE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> gph:status ?oldStatus .
        FILTER(?oldStatus = "proposed")
    }}
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            crate::governance::ontology::GRAPHICA_NS,
            reviewer,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            timestamp,
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id,
            request.notes.unwrap_or_default(),
            crate::governance::ontology::GRAPHICA_NS,
            candidate_id
        );

        match rdf_store.as_ref().update(&update_query) {
            Ok(_) => {
                return Ok(Json(ReviewCandidateResponse {
                    candidate_id,
                    status: "rejected".to_string(),
                    reviewed_by: reviewer,
                    reviewed_at: timestamp,
                }));
            }
            Err(e) => {
                tracing::error!("Failed to reject candidate: {}", e);
                return Err(ApiError::internal(format!("Rejection failed: {}", e)));
            }
        }
    }

    Err(ApiError::internal("RDF store not available".to_string()))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Validate dataset exists and has entities
async fn validate_dataset_exists(
    rdf_store: &Arc<crate::governance::rdf_store::GraphicaRdfStore>,
    dataset: &str,
) -> Result<usize, ApiError> {
    use crate::governance::rdf_store::RdfStore;

    tracing::debug!("Validating dataset exists: {}", dataset);

    let query = format!(
        r#"
PREFIX gph: <{}>

SELECT (COUNT(?entity) as ?count)
WHERE {{
    ?entity a gph:Entity ;
            gph:dataset "{}" .
}}
"#,
        crate::governance::ontology::GRAPHICA_NS,
        dataset
    );

    match rdf_store.as_ref().query(&query) {
        Ok(results) => {
            if let Some(row) = results.first() {
                if let Some(count) = row.get("count").and_then(|v| v.as_u64()) {
                    if count == 0 {
                        tracing::warn!("Dataset '{}' has no entities", dataset);
                        return Err(ApiError::not_found(format!(
                            "Dataset '{}' has no entities",
                            dataset
                        )));
                    }
                    tracing::debug!("Dataset '{}' validated with {} entities", dataset, count);
                    return Ok(count as usize);
                }
            }
            tracing::error!("Failed to parse entity count for dataset '{}'", dataset);
            Err(ApiError::internal(
                "Failed to parse entity count".to_string(),
            ))
        }
        Err(e) => {
            tracing::error!("Failed to validate dataset '{}': {}", dataset, e);
            Err(ApiError::internal(format!(
                "Dataset validation failed: {}",
                e
            )))
        }
    }
}

/// Validate fusion candidate exists and is in expected state
async fn validate_candidate_state(
    rdf_store: &Arc<crate::governance::rdf_store::GraphicaRdfStore>,
    candidate_id: &str,
    expected_status: &str,
) -> Result<(), ApiError> {
    use crate::governance::rdf_store::RdfStore;

    tracing::debug!(
        "Validating candidate '{}' is in '{}' state",
        candidate_id,
        expected_status
    );

    let query = format!(
        r#"
PREFIX gph: <{}>

SELECT ?status
WHERE {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        <{}/fusion/candidate/{}> a gph:FusionCandidate ;
                                   gph:status ?status .
    }}
}}
"#,
        crate::governance::ontology::GRAPHICA_NS,
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id
    );

    match rdf_store.as_ref().query(&query) {
        Ok(results) => {
            if results.is_empty() {
                tracing::warn!("Candidate '{}' not found", candidate_id);
                return Err(ApiError::not_found(format!(
                    "Candidate '{}' not found",
                    candidate_id
                )));
            }

            if let Some(row) = results.first() {
                if let Some(status) = row.get("status").and_then(|v| v.as_str()) {
                    if status != expected_status {
                        tracing::warn!(
                            "Invalid state transition for candidate '{}': current='{}', expected='{}'",
                            candidate_id, status, expected_status
                        );
                        return Err(ApiError::bad_request(format!(
                            "Candidate '{}' is in state '{}', expected '{}'. Invalid state transition.",
                            candidate_id, status, expected_status
                        )));
                    }
                    tracing::debug!("Candidate '{}' state validation passed", candidate_id);
                    return Ok(());
                }
            }

            tracing::error!("Failed to parse status for candidate '{}'", candidate_id);
            Err(ApiError::internal(
                "Failed to parse candidate status".to_string(),
            ))
        }
        Err(e) => {
            tracing::error!("Failed to validate candidate '{}': {}", candidate_id, e);
            Err(ApiError::internal(format!(
                "Candidate validation failed: {}",
                e
            )))
        }
    }
}

/// Validate entities exist in RDF
async fn validate_entities_exist(
    rdf_store: &Arc<crate::governance::rdf_store::GraphicaRdfStore>,
    entity_ids: &[String],
) -> Result<(), ApiError> {
    use crate::governance::rdf_store::RdfStore;

    tracing::debug!("Validating {} entities exist in RDF", entity_ids.len());

    for entity_id in entity_ids {
        let query = format!(
            r#"
PREFIX gph: <{}>

ASK {{
    <{}/entity/{}> a gph:Entity .
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::GRAPHICA_NS,
            entity_id
        );

        match rdf_store.as_ref().query(&query) {
            Ok(results) => {
                // ASK queries return boolean in results
                if results.is_empty() {
                    tracing::warn!("Entity '{}' not found in RDF store", entity_id);
                    return Err(ApiError::not_found(format!(
                        "Entity '{}' not found",
                        entity_id
                    )));
                }
                tracing::trace!("Entity '{}' exists", entity_id);
            }
            Err(e) => {
                tracing::error!("Failed to validate entity '{}': {}", entity_id, e);
                return Err(ApiError::internal(format!(
                    "Entity validation failed: {}",
                    e
                )));
            }
        }
    }

    tracing::debug!("All {} entities validated successfully", entity_ids.len());
    Ok(())
}

/// Check if candidate already exists for same entities
async fn check_duplicate_proposal(
    rdf_store: &Arc<crate::governance::rdf_store::GraphicaRdfStore>,
    _dataset: &str,
    rule: &str,
    match_value: &str,
) -> Result<bool, ApiError> {
    use crate::governance::rdf_store::RdfStore;

    let query = format!(
        r#"
PREFIX gph: <{}>

ASK {{
    GRAPH <http://graphica.io/graph/fusion/candidates> {{
        ?candidate a gph:FusionCandidate ;
                   gph:matchRule "{}" ;
                   gph:matchValue "{}" ;
                   gph:status ?status .
        FILTER(?status != "rejected")
    }}
}}
"#,
        crate::governance::ontology::GRAPHICA_NS,
        rule,
        match_value
    );

    match rdf_store.as_ref().query(&query) {
        Ok(results) => Ok(!results.is_empty()),
        Err(e) => {
            tracing::warn!("Failed to check duplicate: {}", e);
            Ok(false) // Allow on query failure
        }
    }
}

pub fn calculate_match_confidence(
    rule: &str,
    _entities: &[serde_json::Map<String, serde_json::Value>],
) -> f64 {
    // Simple confidence calculation based on match count and rule type
    match rule {
        "email" => 0.95,   // High confidence for email matches
        "phone" => 0.90,   // High confidence for phone matches
        "ssn" => 0.99,     // Very high confidence for SSN matches
        "name" => 0.70,    // Lower confidence for name matches (common names)
        "address" => 0.75, // Medium confidence for address matches
        "tax_id" => 0.98,  // Very high confidence for tax ID matches
        _ => 0.80,         // Default confidence
    }
}

pub fn format_fusion_candidate_triples(
    candidate_id: &str,
    entities: &[serde_json::Map<String, serde_json::Value>],
    rule: &str,
    match_value: &str,
) -> String {
    let mut triples = Vec::new();

    // Candidate metadata
    triples.push(format!(
        "<{}/fusion/candidate/{}> a <{}/FusionCandidate> .",
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id,
        crate::governance::ontology::GRAPHICA_NS
    ));

    triples.push(format!(
        "<{}/fusion/candidate/{}> <{}/matchRule> \"{}\" .",
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id,
        crate::governance::ontology::GRAPHICA_NS,
        rule
    ));

    triples.push(format!(
        "<{}/fusion/candidate/{}> <{}/matchValue> \"{}\" .",
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id,
        crate::governance::ontology::GRAPHICA_NS,
        match_value
    ));

    triples.push(format!(
        "<{}/fusion/candidate/{}> <{}/confidence> \"{}\"^^<http://www.w3.org/2001/XMLSchema#double> .",
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id,
        crate::governance::ontology::GRAPHICA_NS,
        calculate_match_confidence(rule, entities)
    ));

    triples.push(format!(
        "<{}/fusion/candidate/{}> <{}/proposedAt> \"{}\"^^<http://www.w3.org/2001/XMLSchema#dateTime> .",
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id,
        crate::governance::ontology::GRAPHICA_NS,
        chrono::Utc::now().to_rfc3339()
    ));

    triples.push(format!(
        "<{}/fusion/candidate/{}> <{}/status> \"proposed\" .",
        crate::governance::ontology::GRAPHICA_NS,
        candidate_id,
        crate::governance::ontology::GRAPHICA_NS
    ));

    // Link entities
    for entity in entities {
        if let Some(id) = entity.get("id").and_then(|v| v.as_str()) {
            triples.push(format!(
                "<{}/fusion/candidate/{}> <{}/hasEntity> <{}/entity/{}> .",
                crate::governance::ontology::GRAPHICA_NS,
                candidate_id,
                crate::governance::ontology::GRAPHICA_NS,
                crate::governance::ontology::GRAPHICA_NS,
                id
            ));
        }
    }

    triples.join("\n        ")
}
