use super::types::*;
use crate::api::ApiState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use graphica_core::migration_evidence::{
    ConnectorStoreBackend, ConnectorStoreHealth, ConnectorStoreRuntimeStatus,
    EvidenceIngestionRuntimeStatus, MigrationEvidenceDeliveryMode, ValueLocator,
};
use std::collections::HashMap;
use std::sync::Arc;

fn gateway(
    state: &Arc<ApiState>,
) -> Result<&Arc<super::MigrationEvidenceGateway>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    state.migration_evidence_gateway.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(MigrationEvidenceErrorResponse {
                error: "Migration evidence gateway is not configured".to_string(),
                details: HashMap::new(),
            }),
        )
    })
}

pub async fn upsert_connector(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<UpsertMigrationConnectorRequest>,
) -> Result<Json<UpsertMigrationConnectorResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let connector = gateway(&state)?
        .upsert_connector(request.connector)
        .await
        .map_err(map_error)?;
    Ok(Json(UpsertMigrationConnectorResponse { connector }))
}

pub async fn run_connector(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(request): Json<RunMigrationConnectorRequest>,
) -> Result<Json<RunMigrationConnectorResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let (summary, events) = gateway(&state)?
        .run_connector(&id, request.run)
        .await
        .map_err(map_error)?;
    let ingested_events = events
        .into_iter()
        .map(|event| serde_json::to_value(event))
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_error)?;
    Ok(Json(RunMigrationConnectorResponse {
        summary,
        ingested_events,
    }))
}

pub async fn explain_value(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ExplainValueQuery>,
) -> Result<Json<ExplainValueResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let explanation = gateway(&state)?
        .explain_value(ValueLocator {
            program_id: query.program_id,
            object_id: query.object_id,
            target_field_path: query.target_field_path,
            target_record_id: query.target_record_id,
            source_record_id: query.source_record_id,
        })
        .await
        .map_err(map_error)?;
    Ok(Json(ExplainValueResponse { explanation }))
}

pub async fn get_evidence_packet(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(query): Query<EvidencePacketQuery>,
) -> Result<Json<EvidencePacketResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let packet = gateway(&state)?
        .evidence_packet_for_object(&id, query.value_key.as_deref())
        .await
        .map_err(map_error)?;
    Ok(Json(EvidencePacketResponse { packet }))
}

pub async fn get_object_controls(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ObjectControlsResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let controls = gateway(&state)?
        .controls_for_object(&id)
        .await
        .map_err(map_error)?;
    Ok(Json(ObjectControlsResponse { controls }))
}

pub async fn get_program_exceptions(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ProgramExceptionsResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let exceptions = gateway(&state)?
        .exceptions_for_program(&id)
        .await
        .map_err(map_error)?;
    Ok(Json(ProgramExceptionsResponse { exceptions }))
}

pub async fn get_program_approvals(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ProgramApprovalsResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let approvals = gateway(&state)?
        .approvals_for_program(&id)
        .await
        .map_err(map_error)?;
    Ok(Json(ProgramApprovalsResponse { approvals }))
}

pub async fn get_runtime_status(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<MigrationEvidenceRuntimeStatusResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let gateway = gateway(&state)?;
    let status = gateway
        .runtime_status()
        .await
        .map_err(map_error)?;
    let ingestion_status = Some(
        gateway
            .ingestion_runtime_status()
            .await
            .unwrap_or_else(|error| unavailable_ingestion_status(error.to_string())),
    );
    Ok(Json(MigrationEvidenceRuntimeStatusResponse {
        status,
        ingestion_status,
    }))
}

pub async fn rebuild_read_models(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<MigrationEvidenceRebuildResponse>, (StatusCode, Json<MigrationEvidenceErrorResponse>)> {
    let summary = gateway(&state)?
        .rebuild_read_models()
        .await
        .map_err(map_error)?;
    Ok(Json(MigrationEvidenceRebuildResponse { summary }))
}

fn map_error(
    error: impl std::fmt::Display,
) -> (StatusCode, Json<MigrationEvidenceErrorResponse>) {
    let message = error.to_string();
    let status = if is_not_found(&message) {
        StatusCode::NOT_FOUND
    } else if is_invalid_request(&message) {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };

    (
        status,
        Json(MigrationEvidenceErrorResponse {
            error: message,
            details: HashMap::new(),
        }),
    )
}

fn is_not_found(message: &str) -> bool {
    message.contains("unknown connector")
        || message.contains("unknown migration object")
        || message.contains("no evidence found for object")
        || message.contains("no controls found for object")
}

fn is_invalid_request(message: &str) -> bool {
    message.contains("cannot be empty")
        || message.contains("require")
        || message.contains("missing")
        || message.contains("disabled")
        || message.contains("must be")
        || message.contains("not supported")
}

fn unavailable_ingestion_status(error: String) -> EvidenceIngestionRuntimeStatus {
    EvidenceIngestionRuntimeStatus {
        connector_store: ConnectorStoreRuntimeStatus {
            backend: ConnectorStoreBackend::Unknown,
            health: ConnectorStoreHealth::Unavailable,
            connector_count: 0,
            writable: false,
            updated_at: Utc::now(),
            last_successful_write_at: None,
            legacy_imported_at: None,
            last_error: Some(error),
        },
        delivery_mode: MigrationEvidenceDeliveryMode::Direct,
        verification_service_configured: false,
        updated_at: Utc::now(),
    }
}
