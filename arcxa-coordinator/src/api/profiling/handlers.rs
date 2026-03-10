//! Profiling API Handlers
//!
//! HTTP request handlers for dataset profiling operations.

use super::types::*;
use crate::api::dto::ApiError;
use crate::api::ApiState;
use crate::governance::rdf_store::RdfStore;
use crate::mapping::profiling::{ProfileConfig, SourceProfiler};
use axum::{
    extract::{Path, State},
    response::Json,
};
use std::path::PathBuf;
use std::sync::Arc;

/// Profile a dataset (CSV or Parquet)
///
/// POST /api/v1/profiling/profile
pub async fn profile_dataset(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<ProfileDatasetRequest>,
) -> Result<Json<ProfileDatasetResponse>, ApiError> {
    tracing::info!(
        "Profiling dataset: source={}, format={}",
        request.source,
        request.format
    );

    // Create profiler configuration
    let config = ProfileConfig {
        sample_size: request.sample_size,
        detect_semantic_types: request.detect_semantic_types,
        infer_relationships: request.infer_relationships,
        ..Default::default()
    };

    let profiler = SourceProfiler::new(config);

    // Profile the source file
    let source_path = PathBuf::from(&request.source);
    let profile = match request.format.as_str() {
        "csv" => profiler.profile_csv(&source_path).await,
        "parquet" => {
            return Err(ApiError::bad_request(
                "Parquet profiling not yet implemented".to_string(),
            ));
        }
        _ => {
            return Err(ApiError::bad_request(format!(
                "Unsupported format: {}",
                request.format
            )));
        }
    }
    .map_err(|e| {
        tracing::error!("Profiling failed: {}", e);
        ApiError::internal(format!("Profiling failed: {}", e))
    })?;

    // Generate dataset URI
    let dataset_uri = profiler.generate_dataset_uri(&source_path);

    // Optionally store in RDF store
    let graph_uri = if request.store_in_rdf {
        if let Some(ref rdf_store) = state.rdf_store {
            // Serialize profile to DCAT/VoID RDF
            let rdf_turtle = profiler
                .profile_to_rdf(&profile, &dataset_uri)
                .map_err(|e| {
                    tracing::error!("RDF serialization failed: {}", e);
                    ApiError::internal(format!("RDF serialization failed: {}", e))
                })?;

            // Store in RDF graph
            let named_graph =
                crate::governance::rdf_store::NamedGraph::new(dataset_uri.graph.clone());
            rdf_store
                .load_turtle(&rdf_turtle, Some(&named_graph))
                .map_err(|e| {
                    tracing::error!("RDF storage failed: {}", e);
                    ApiError::internal(format!("RDF storage failed: {}", e))
                })?;

            tracing::info!(
                "Stored profile in RDF graph: {} ({} triples)",
                dataset_uri.graph,
                rdf_turtle.lines().count()
            );

            Some(dataset_uri.graph.clone())
        } else {
            tracing::warn!("RDF store not available, skipping RDF storage");
            None
        }
    } else {
        None
    };

    let response = ProfileDatasetResponse {
        dataset_uri: dataset_uri.uri.clone(),
        dataset_id: profile.dataset_id.clone(),
        rows_profiled: profile.rows_profiled,
        column_count: profile.column_count,
        file_size_bytes: profile.file_size_bytes,
        duration_seconds: profile.duration_seconds,
        candidate_keys: profile.candidate_keys.clone(),
        profiled_at: profile.profiled_at,
        graph_uri,
        profile_link: format!("/api/v1/profiling/profiles/{}", dataset_uri.local_id),
    };

    tracing::info!(
        "Profiling complete: {} rows, {} columns in {:.1}s",
        profile.rows_profiled,
        profile.column_count,
        profile.duration_seconds
    );

    Ok(Json(response))
}

/// Get profile details by dataset ID
///
/// GET /api/v1/profiling/profiles/{dataset_id}
pub async fn get_profile(
    State(state): State<Arc<ApiState>>,
    Path(dataset_id): Path<String>,
) -> Result<Json<GetProfileResponse>, ApiError> {
    tracing::debug!("Getting profile for dataset: {}", dataset_id);

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not available".to_string()))?;

    // Construct dataset URI from local ID
    let dataset_uri = format!("http://graphica.io/dataset/{}", dataset_id);

    // SPARQL query to retrieve profile metadata
    let sparql_query = format!(
        r#"
        PREFIX dcat: <http://www.w3.org/ns/dcat#>
        PREFIX void: <http://rdfs.org/ns/void#>
        PREFIX dcterms: <http://purl.org/dc/terms/>
        PREFIX gph: <http://graphica.io/ontology#>
        PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

        SELECT ?title ?format ?sourceLocation ?byteSize ?totalRows ?rowsProfiled ?columnCount ?created
        WHERE {{
            <{}> a dcat:Dataset ;
                 dcterms:title ?title ;
                 gph:format ?format ;
                 gph:sourceLocation ?sourceLocation ;
                 dcat:byteSize ?byteSize ;
                 gph:rowsProfiled ?rowsProfiled ;
                 gph:columnCount ?columnCount ;
                 dcterms:created ?created .
            OPTIONAL {{ <{}> void:entities ?totalRows . }}
        }}
        "#,
        dataset_uri, dataset_uri
    );

    let results = rdf_store.query(&sparql_query).map_err(|e| {
        tracing::error!("SPARQL query failed: {}", e);
        ApiError::internal(format!("SPARQL query failed: {}", e))
    })?;

    if results.is_empty() {
        return Err(ApiError::not_found(format!(
            "Profile not found: {}",
            dataset_id
        )));
    }

    let result = &results[0];

    // Extract basic metadata
    let title = result
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::internal("Missing title".to_string()))?
        .to_string();

    let format = result
        .get("format")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::internal("Missing format".to_string()))?
        .to_string();

    let source_location = result
        .get("sourceLocation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::internal("Missing sourceLocation".to_string()))?
        .to_string();

    let file_size_bytes = result
        .get("byteSize")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ApiError::internal("Missing byteSize".to_string()))?;

    let rows_profiled = result
        .get("rowsProfiled")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ApiError::internal("Missing rowsProfiled".to_string()))?;

    let column_count = result
        .get("columnCount")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ApiError::internal("Missing columnCount".to_string()))?
        as usize;

    let created_str = result
        .get("created")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::internal("Missing created".to_string()))?;

    let profiled_at = chrono::DateTime::parse_from_rfc3339(created_str)
        .map_err(|e| ApiError::internal(format!("Invalid timestamp: {}", e)))?
        .with_timezone(&chrono::Utc);

    let total_rows = result.get("totalRows").and_then(|v| v.as_u64());

    // Query for columns (simplified - just get column names for now)
    // Full column profile reconstruction would require additional SPARQL queries
    let columns_query = format!(
        r#"
        PREFIX gph: <http://graphica.io/ontology#>
        PREFIX void: <http://rdfs.org/ns/void#>

        SELECT ?columnName ?columnIndex ?dataType ?nullCount ?nullPercentage ?distinctCount ?cardinality
        WHERE {{
            <{}> gph:hasColumn ?column .
            ?column gph:columnName ?columnName ;
                    gph:columnIndex ?columnIndex ;
                    gph:dataType ?dataType ;
                    gph:nullCount ?nullCount ;
                    gph:nullPercentage ?nullPercentage ;
                    gph:cardinality ?cardinality ;
                    void:distinctValues ?distinctCount .
        }}
        ORDER BY ?columnIndex
        "#,
        dataset_uri
    );

    let column_results = rdf_store.query(&columns_query).unwrap_or_else(|_| vec![]);

    let columns: Vec<super::types::ColumnProfileDto> = column_results
        .iter()
        .map(|col| super::types::ColumnProfileDto {
            name: col
                .get("columnName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            index: col.get("columnIndex").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            data_type: col
                .get("dataType")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            semantic_type: None,
            null_count: col.get("nullCount").and_then(|v| v.as_u64()).unwrap_or(0),
            null_percentage: col
                .get("nullPercentage")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            distinct_count: col
                .get("distinctCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cardinality: col
                .get("cardinality")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            min_value: None,
            max_value: None,
            mean: None,
            median: None,
            std_dev: None,
            min_length: None,
            max_length: None,
            avg_length: None,
            pattern_example: None,
            top_values: vec![],
        })
        .collect();

    let response = GetProfileResponse {
        dataset_uri: dataset_uri.clone(),
        dataset_id: dataset_id.clone(),
        source_location,
        format,
        file_size_bytes,
        total_rows,
        rows_profiled,
        column_count,
        columns,
        candidate_keys: vec![], // TODO: Query candidate keys from RDF
        profiled_at,
        duration_seconds: 0.0, // Not stored in RDF currently
        rdf_turtle: None,      // Could optionally serialize to turtle
    };

    Ok(Json(response))
}

/// List all profiled datasets
///
/// GET /api/v1/profiling/profiles
pub async fn list_profiles(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<ListProfilesResponse>, ApiError> {
    tracing::debug!("Listing all profiled datasets");

    let rdf_store = state
        .rdf_store
        .as_ref()
        .ok_or_else(|| ApiError::internal("RDF store not available".to_string()))?;

    // SPARQL query to list all datasets
    let sparql_query = r#"
        PREFIX dcat: <http://www.w3.org/ns/dcat#>
        PREFIX dcterms: <http://purl.org/dc/terms/>
        PREFIX gph: <http://graphica.io/ontology#>

        SELECT ?dataset ?title ?format ?sourceLocation ?rowsProfiled ?columnCount ?created
        WHERE {
            ?dataset a dcat:Dataset ;
                     dcterms:title ?title ;
                     gph:format ?format ;
                     gph:sourceLocation ?sourceLocation ;
                     gph:rowsProfiled ?rowsProfiled ;
                     gph:columnCount ?columnCount ;
                     dcterms:created ?created .
        }
        ORDER BY DESC(?created)
        "#;

    let results = rdf_store.query(sparql_query).map_err(|e| {
        tracing::error!("SPARQL query failed: {}", e);
        ApiError::internal(format!("SPARQL query failed: {}", e))
    })?;

    let profiles: Vec<super::types::ProfileSummaryDto> = results
        .iter()
        .filter_map(|row| {
            let dataset_uri = row.get("dataset")?.as_str()?;
            let title = row.get("title")?.as_str()?.to_string();
            let format = row.get("format")?.as_str()?.to_string();
            let source_location = row.get("sourceLocation")?.as_str()?.to_string();
            let rows_profiled = row.get("rowsProfiled")?.as_u64()?;
            let column_count = row.get("columnCount")?.as_u64()? as usize;
            let created_str = row.get("created")?.as_str()?;
            let profiled_at = chrono::DateTime::parse_from_rfc3339(created_str)
                .ok()?
                .with_timezone(&chrono::Utc);

            Some(super::types::ProfileSummaryDto {
                dataset_uri: dataset_uri.to_string(),
                dataset_id: title.clone(),
                source_location,
                format,
                rows_profiled,
                column_count,
                profiled_at,
            })
        })
        .collect();

    let total_count = profiles.len();

    tracing::info!("Found {} profiled datasets", total_count);

    Ok(Json(ListProfilesResponse {
        profiles,
        total_count,
    }))
}
