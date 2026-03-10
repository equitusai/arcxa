//! Quality Handler Functions
//!
//! HTTP handlers for data quality scorecards, violations, and rule management.

use crate::api::dto::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;

/// Get quality scorecard - Queries RDF for quality metrics
pub async fn get_scorecard(
    State(state): State<Arc<ApiState>>,
    Path(dataset): Path<String>,
    Query(params): Query<ScorecardQuery>,
) -> Result<Json<ScorecardResponse>, ApiError> {
    tracing::info!("Getting quality scorecard for dataset: {}", dataset);

    // Query RDF store for quality metrics
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        // SPARQL query to get quality score and metrics
        let sparql = format!(
            r#"
PREFIX gph: <{}>
PREFIX xsd: <{}>

SELECT ?score ?totalRecords ?violationCount ?severity
WHERE {{
    <{}/dataset/{}> gph:hasQualityScore ?score .
    OPTIONAL {{ <{}/dataset/{}> gph:totalRecords ?totalRecords }}
    OPTIONAL {{
        ?scorecard gph:dataset <{}/dataset/{}> ;
                   gph:violationCount ?violationCount ;
                   gph:severity ?severity .
    }}
}}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::XSD_NS,
            crate::governance::ontology::GRAPHICA_NS,
            dataset,
            crate::governance::ontology::GRAPHICA_NS,
            dataset,
            crate::governance::ontology::GRAPHICA_NS,
            dataset,
        );

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                // Parse overall score
                let overall_score = results
                    .first()
                    .and_then(|r| r.get("score"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0); // Default to 1.0 if no score found

                // Build dimension scores from results
                let mut dimension_scores = std::collections::HashMap::new();
                dimension_scores.insert("completeness".to_string(), overall_score);
                dimension_scores.insert("validity".to_string(), overall_score);
                dimension_scores.insert("consistency".to_string(), overall_score);

                return Ok(Json(ScorecardResponse {
                    dataset: dataset.clone(),
                    overall_score,
                    period_start: params.start.clone(),
                    period_end: params.end.clone(),
                    dimension_scores,
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query for scorecard failed: {}", e);
                // Fall through to default response
            }
        }
    }

    // Fallback: return default scorecard
    Ok(Json(ScorecardResponse {
        dataset,
        overall_score: 1.0,
        period_start: params.start,
        period_end: params.end,
        dimension_scores: std::collections::HashMap::new(),
    }))
}

/// List quality violations - Queries RDF for violations
pub async fn list_violations(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<ViolationQuery>,
) -> Result<Json<ViolationListResponse>, ApiError> {
    tracing::info!("Listing quality violations");

    // Query RDF store for violations
    if let Some(ref rdf_store) = state.rdf_store {
        use crate::governance::rdf_store::RdfStore;

        let page = params.page.unwrap_or(1);
        let limit = params.limit.unwrap_or(100);
        let offset = (page - 1) * limit;

        // Build SPARQL query with optional dataset filter
        let dataset_filter = if let Some(ref dataset) = params.dataset {
            format!(
                "FILTER(?dataset = <{}/dataset/{}>)",
                crate::governance::ontology::GRAPHICA_NS,
                dataset
            )
        } else {
            String::new()
        };

        let sparql = format!(
            r#"
PREFIX gph: <{}>
PREFIX prov: <{}>

SELECT ?violation ?dataset ?ruleId ?severity ?message ?recordId ?detectedAt
WHERE {{
    ?violation a gph:QualityViolation ;
               gph:dataset ?dataset ;
               gph:ruleId ?ruleId ;
               gph:severity ?severity ;
               gph:message ?message ;
               gph:affectedRecord ?recordId ;
               prov:generatedAtTime ?detectedAt .
    {}
}}
ORDER BY DESC(?detectedAt)
LIMIT {}
OFFSET {}
"#,
            crate::governance::ontology::GRAPHICA_NS,
            crate::governance::ontology::PROV_NS,
            dataset_filter,
            limit,
            offset,
        );

        match rdf_store.as_ref().query(&sparql) {
            Ok(results) => {
                let total = results.len() as u64;

                return Ok(Json(ViolationListResponse {
                    violations: results,
                    total,
                    page,
                }));
            }
            Err(e) => {
                tracing::warn!("RDF query for violations failed: {}", e);
            }
        }
    }

    // Fallback: return empty list
    Ok(Json(ViolationListResponse {
        violations: vec![],
        total: 0,
        page: params.page.unwrap_or(1),
    }))
}

/// Create quality rule - STUB
pub async fn create_rule(
    State(_state): State<Arc<ApiState>>,
    Json(rule): Json<CreateRuleRequest>,
) -> Result<Json<RuleResponse>, ApiError> {
    Ok(Json(RuleResponse {
        id: uuid::Uuid::new_v4().to_string(),
        name: rule.name,
        created: true,
    }))
}

/// Get quality rule - STUB
pub async fn get_rule(
    State(_state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<RuleResponse>, ApiError> {
    Ok(Json(RuleResponse {
        id,
        name: "example_rule".to_string(),
        created: false,
    }))
}
