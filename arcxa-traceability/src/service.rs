use crate::projection;
use crate::signing::{build_evidence_packet_signature, verify_evidence_packet_signature};
use crate::store::{ObjectIndex, PersistedTraceabilityStore};
use crate::EventBusRuntimeMonitor;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use graphica_core::distributed::proto::migration_evidence::{
    traceability_service_server::TraceabilityService, ApprovalsReply, ControlsReply,
    EvidencePacketReply, ExplainValueRequest, ExceptionsReply, GetEvidencePacketRequest,
    GetObjectRequest, GetProgramRequest, HealthRequest, HealthResponse, IngestEventsRequest,
    IngestEventsResponse, RebuildReadModelsRequest, RebuildReadModelsResponse,
    RuntimeStatusReply, RuntimeStatusRequest, ValueExplanationReply,
};
use graphica_core::migration_evidence::{
    ApprovalEvent, ControlResult, EvidencePacket, ExecutionEvent, ExceptionRecord,
    MigrationEvidenceArtifactType, MigrationEvidenceEvent, MigrationObject, MigrationProgram,
    TraceabilityRebuildSummary, TraceabilityRuntimeStatus, TransformationRule,
    ValueExplanation, ValueLocator,
};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status};

#[derive(Debug, Clone, Default)]
pub struct GraphProjectionConfig {
    pub shard_endpoint: Option<String>,
}

#[derive(Clone)]
pub struct TraceabilityManager {
    store: PersistedTraceabilityStore,
    signing_key: Arc<SigningKey>,
    graph_projection: GraphProjectionConfig,
    event_bus_runtime: EventBusRuntimeMonitor,
}

impl TraceabilityManager {
    pub fn new(
        store: PersistedTraceabilityStore,
        signing_key: SigningKey,
        graph_projection: GraphProjectionConfig,
        event_bus_runtime: EventBusRuntimeMonitor,
    ) -> Self {
        Self {
            store,
            signing_key: Arc::new(signing_key),
            graph_projection,
            event_bus_runtime,
        }
    }

    pub async fn ingest_events(&self, events: Vec<MigrationEvidenceEvent>) -> Result<(usize, Vec<String>, Vec<String>)> {
        let mut graph_triples = Vec::new();
        let (ingested_count, program_ids, object_ids) = self
            .store
            .append_events_and_mutate(&events, |state, accepted_events| {
                let summary = apply_events_to_state(state, accepted_events, &mut graph_triples);
                (
                    summary.ingested_count,
                    summary.program_ids,
                    summary.object_ids,
                )
            })
            .await?;

        if let Some(endpoint) = &self.graph_projection.shard_endpoint {
            let _ = projection::push_triples_to_shard(endpoint, graph_triples).await;
        }

        Ok((ingested_count, program_ids, object_ids))
    }

    pub async fn runtime_status(&self) -> Result<TraceabilityRuntimeStatus> {
        let mut status = self.store.runtime_status().await?;
        status.event_bus = self.event_bus_runtime.snapshot().await;
        Ok(status)
    }

    pub fn event_bus_runtime_monitor(&self) -> EventBusRuntimeMonitor {
        self.event_bus_runtime.clone()
    }

    pub async fn rebuild_read_models(&self) -> Result<TraceabilityRebuildSummary> {
        let events = self.store.replay_events().await?;
        let mut rebuilt_state = crate::store::TraceabilityState::default();
        let summary = apply_events_to_state(&mut rebuilt_state, &events, &mut Vec::new());
        rebuilt_state.processed_event_ids = events.iter().map(|event| event.event_id.clone()).collect();
        rebuilt_state.last_event_sequence = events.len() as u64;
        self.store.replace_state_from_replay(rebuilt_state).await?;

        Ok(TraceabilityRebuildSummary {
            backend: self.store.backend_kind(),
            replayed_event_count: events.len(),
            touched_program_count: summary.program_ids.len(),
            touched_object_count: summary.object_ids.len(),
            rebuilt_at: Utc::now(),
        })
    }

    pub async fn explain_value(&self, locator: ValueLocator) -> Result<ValueExplanation> {
        let state = self.store.snapshot().await;
        let object_index = state
            .object_indexes
            .get(&locator.object_id)
            .ok_or_else(|| anyhow!("no evidence found for object '{}'", locator.object_id))?;
        let value_key = projection::value_key(&locator);

        let rules = collect_by_ids(&state.rules, object_index.value_key_rules.get(&value_key), &object_index.object_level_rule_ids);
        let executions = collect_by_ids(&state.executions, object_index.value_key_execution_ids.get(&value_key), &object_index.object_level_execution_ids);
        let exceptions = collect_by_ids(&state.exceptions, object_index.value_key_exception_ids.get(&value_key), &object_index.object_level_exception_ids);
        let controls = collect_by_ids(&state.controls, object_index.value_key_control_ids.get(&value_key), &object_index.object_level_control_ids);
        let approvals = collect_by_ids(&state.approvals, object_index.value_key_approval_ids.get(&value_key), &object_index.object_level_approval_ids);
        let packets = collect_by_ids(&state.packets, object_index.value_key_packet_ids.get(&value_key), &object_index.object_level_packet_ids);

        let target_rule = rules
            .iter()
            .find(|rule| rule.target_fields.iter().any(|field| field.field_path == locator.target_field_path))
            .cloned()
            .or_else(|| rules.first().cloned());

        let target_field = target_rule
            .as_ref()
            .and_then(|rule| {
                rule.target_fields
                    .iter()
                    .find(|field| field.field_path == locator.target_field_path)
                    .cloned()
                    .or_else(|| rule.target_fields.first().cloned())
            })
            .or_else(|| packets.first().map(|packet| packet.target_field.clone()))
            .ok_or_else(|| anyhow!("no target field evidence found for '{}:{}'", locator.object_id, locator.target_field_path))?;

        let source_field = target_rule
            .as_ref()
            .and_then(|rule| rule.source_fields.first().cloned())
            .or_else(|| packets.first().map(|packet| packet.source_field.clone()))
            .ok_or_else(|| anyhow!("no source field evidence found for '{}:{}'", locator.object_id, locator.target_field_path))?;

        let execution_event = primary_execution(executions);
        let source_value = exceptions.first().and_then(|record| record.source_value.clone()).or_else(|| {
            controls
                .first()
                .and_then(|control| control.expected_value.clone())
        });
        let target_value = exceptions.first().and_then(|record| record.target_value.clone()).or_else(|| {
            controls
                .first()
                .and_then(|control| control.actual_value.clone())
        });

        let mut explanation = ValueExplanation {
            explanation_id: uuid::Uuid::new_v4().to_string(),
            locator,
            source_field,
            target_field,
            source_value,
            target_value,
            transformation_rule: target_rule,
            execution_event,
            exceptions,
            controls,
            approvals,
            evidence_packet_id: None,
            graph_refs: vec![],
            confidence_summary: Some("Evidence graph assembled from transformation, execution, control, and approval artifacts".to_string()),
            generated_at: Utc::now(),
        };

        let packet = if let Some(packet) = packets.first().cloned() {
            packet
        } else {
            self.build_evidence_packet(&explanation).await?
        };
        explanation.evidence_packet_id = Some(packet.packet_id.clone());
        explanation.graph_refs = projection::explanation_graph_refs(&explanation, Some(&packet));
        Ok(explanation)
    }

    pub async fn evidence_packet_for_object(&self, object_id: &str, value_key: Option<&str>) -> Result<EvidencePacket> {
        let state = self.store.snapshot().await;
        let object = state
            .objects
            .get(object_id)
            .ok_or_else(|| anyhow!("unknown migration object '{}'", object_id))?;

        if let Some(packet) = find_existing_packet(&state, object, value_key) {
            return Ok(packet);
        }

        let locator = ValueLocator {
            program_id: object.program_id.clone(),
            object_id: object_id.to_string(),
            target_field_path: value_key.unwrap_or("$.value").to_string(),
            target_record_id: object.target_record_id.clone(),
            source_record_id: object.source_record_id.clone(),
        };
        drop(state);
        let explanation = self.explain_value(locator).await?;
        self.build_evidence_packet(&explanation).await
    }

    pub async fn controls_for_object(&self, object_id: &str) -> Result<Vec<ControlResult>> {
        let state = self.store.snapshot().await;
        let object_index = state
            .object_indexes
            .get(object_id)
            .ok_or_else(|| anyhow!("no controls found for object '{}'", object_id))?;
        Ok(collect_all_by_ids(&state.controls, object_index))
    }

    pub async fn exceptions_for_program(&self, program_id: &str) -> Result<Vec<ExceptionRecord>> {
        let state = self.store.snapshot().await;
        let object_ids = state.program_to_objects.get(program_id).cloned().unwrap_or_default();
        let mut results = Vec::new();
        for object_id in object_ids {
            if let Some(index) = state.object_indexes.get(&object_id) {
                results.extend(collect_all_by_ids(&state.exceptions, index));
            }
        }
        results.sort_by_key(|record| record.detected_at);
        Ok(results)
    }

    pub async fn approvals_for_program(&self, program_id: &str) -> Result<Vec<ApprovalEvent>> {
        let state = self.store.snapshot().await;
        let object_ids = state.program_to_objects.get(program_id).cloned().unwrap_or_default();
        let mut results = Vec::new();
        for object_id in object_ids {
            if let Some(index) = state.object_indexes.get(&object_id) {
                results.extend(collect_all_by_ids(&state.approvals, index));
            }
        }
        results.sort_by_key(|record| record.approved_at);
        Ok(results)
    }

    async fn build_evidence_packet(&self, explanation: &ValueExplanation) -> Result<EvidencePacket> {
        let value_key = projection::value_key(&explanation.locator);
        let state = self.store.snapshot().await;
        if let Some(object) = state.objects.get(&explanation.locator.object_id) {
            if let Some(packet) = find_existing_packet(&state, object, Some(&value_key)) {
                return Ok(packet);
            }
        }
        drop(state);

        let packet_id = format!("packet-{}", uuid::Uuid::new_v4());
        let mut packet = EvidencePacket {
            packet_id,
            program_id: explanation.locator.program_id.clone(),
            object_id: explanation.locator.object_id.clone(),
            value_key: value_key.clone(),
            generated_at: Utc::now(),
            source_field: explanation.source_field.clone(),
            target_field: explanation.target_field.clone(),
            transformation_rule: explanation.transformation_rule.clone(),
            execution_event: explanation.execution_event.clone(),
            exceptions: explanation.exceptions.clone(),
            controls: explanation.controls.clone(),
            approvals: explanation.approvals.clone(),
            graph_refs: vec![],
            narrative: Some(build_narrative(explanation)),
            signature: None,
            metadata: HashMap::new(),
        };
        packet.graph_refs = projection::explanation_graph_refs(explanation, Some(&packet));
        let signature = build_evidence_packet_signature(&packet, &self.signing_key)?;
        packet.signature = Some(signature.clone());
        if !verify_evidence_packet_signature(&packet, &signature) {
            return Err(anyhow!("generated evidence packet signature failed verification"));
        }

        let event = graphica_core::migration_evidence::MigrationEvidenceEvent::new(
            "traceability-service",
            format!("packet-run-{}", packet.packet_id),
            graphica_core::migration_evidence::MigrationConnectorVendor::Generic,
            packet.program_id.clone(),
            packet.object_id.clone(),
            graphica_core::migration_evidence::MigrationEvidenceArtifactType::EvidencePacket,
            Some(value_key),
            serde_json::to_value(&packet).context("failed to serialize evidence packet")?,
        );
        let _ = self.ingest_events(vec![event]).await?;
        Ok(packet)
    }
}

struct AppliedEventSummary {
    ingested_count: usize,
    program_ids: Vec<String>,
    object_ids: Vec<String>,
}

fn apply_events_to_state(
    state: &mut crate::store::TraceabilityState,
    events: &[MigrationEvidenceEvent],
    graph_triples: &mut Vec<graphica_core::distributed::proto::shard_service::Triple>,
) -> AppliedEventSummary {
    let mut touched_program_ids = Vec::new();
    let mut touched_object_ids = Vec::new();

    for event in events {
        touched_program_ids.push(event.program_id.clone());

        let object_index = if matches!(event.artifact_type, MigrationEvidenceArtifactType::Program)
            || event.object_id.is_empty()
        {
            None
        } else {
            touched_object_ids.push(event.object_id.clone());
            push_unique(
                state
                    .program_to_objects
                    .entry(event.program_id.clone())
                    .or_default(),
                event.object_id.clone(),
            );
            Some(
                state
                    .object_indexes
                    .entry(event.object_id.clone())
                    .or_insert_with(ObjectIndex::default),
            )
        };

        match event.artifact_type {
            MigrationEvidenceArtifactType::Program => {
                if let Ok(program) = serde_json::from_value::<MigrationProgram>(event.payload.clone()) {
                    graph_triples.extend(projection::program_triples(&program));
                    state.programs.insert(program.program_id.clone(), program);
                }
            }
            MigrationEvidenceArtifactType::Object => {
                if let Ok(object) = serde_json::from_value::<MigrationObject>(event.payload.clone()) {
                    graph_triples.extend(projection::object_triples(&object));
                    state.objects.insert(object.object_id.clone(), object);
                }
            }
            MigrationEvidenceArtifactType::TransformationRule => {
                if let Ok(rule) = serde_json::from_value::<TransformationRule>(event.payload.clone()) {
                    graph_triples.extend(projection::rule_triples(&event.object_id, event.value_key.as_deref(), &rule));
                    if let Some(object_index) = object_index {
                        if let Some(value_key) = &event.value_key {
                            push_unique(
                                object_index
                                    .value_key_rules
                                    .entry(value_key.clone())
                                    .or_default(),
                                rule.rule_id.clone(),
                            );
                        } else {
                            push_unique(&mut object_index.object_level_rule_ids, rule.rule_id.clone());
                        }
                    }
                    state.rules.insert(rule.rule_id.clone(), rule);
                }
            }
            MigrationEvidenceArtifactType::ExecutionEvent => {
                if let Ok(execution) = serde_json::from_value::<ExecutionEvent>(event.payload.clone()) {
                    graph_triples.extend(projection::execution_triples(&execution));
                    if let Some(object_index) = object_index {
                        if let Some(value_key) = &event.value_key {
                            push_unique(
                                object_index
                                    .value_key_execution_ids
                                    .entry(value_key.clone())
                                    .or_default(),
                                execution.execution_id.clone(),
                            );
                        } else {
                            push_unique(
                                &mut object_index.object_level_execution_ids,
                                execution.execution_id.clone(),
                            );
                        }
                    }
                    state.executions.insert(execution.execution_id.clone(), execution);
                }
            }
            MigrationEvidenceArtifactType::ExceptionRecord => {
                if let Ok(exception) = serde_json::from_value::<ExceptionRecord>(event.payload.clone()) {
                    graph_triples.extend(projection::exception_triples(&exception));
                    if let Some(object_index) = object_index {
                        if let Some(value_key) = &event.value_key {
                            push_unique(
                                object_index
                                    .value_key_exception_ids
                                    .entry(value_key.clone())
                                    .or_default(),
                                exception.exception_id.clone(),
                            );
                        } else {
                            push_unique(
                                &mut object_index.object_level_exception_ids,
                                exception.exception_id.clone(),
                            );
                        }
                    }
                    state.exceptions.insert(exception.exception_id.clone(), exception);
                }
            }
            MigrationEvidenceArtifactType::ControlResult => {
                if let Ok(control) = serde_json::from_value::<ControlResult>(event.payload.clone()) {
                    graph_triples.extend(projection::control_triples(&control));
                    if let Some(object_index) = object_index {
                        if let Some(value_key) = &event.value_key {
                            push_unique(
                                object_index
                                    .value_key_control_ids
                                    .entry(value_key.clone())
                                    .or_default(),
                                control.control_id.clone(),
                            );
                        } else {
                            push_unique(&mut object_index.object_level_control_ids, control.control_id.clone());
                        }
                    }
                    state.controls.insert(control.control_id.clone(), control);
                }
            }
            MigrationEvidenceArtifactType::ApprovalEvent => {
                if let Ok(approval) = serde_json::from_value::<ApprovalEvent>(event.payload.clone()) {
                    graph_triples.extend(projection::approval_triples(&approval));
                    if let Some(object_index) = object_index {
                        if let Some(value_key) = &event.value_key {
                            push_unique(
                                object_index
                                    .value_key_approval_ids
                                    .entry(value_key.clone())
                                    .or_default(),
                                approval.approval_id.clone(),
                            );
                        } else {
                            push_unique(
                                &mut object_index.object_level_approval_ids,
                                approval.approval_id.clone(),
                            );
                        }
                    }
                    state.approvals.insert(approval.approval_id.clone(), approval);
                }
            }
            MigrationEvidenceArtifactType::EvidencePacket => {
                if let Ok(packet) = serde_json::from_value::<EvidencePacket>(event.payload.clone()) {
                    graph_triples.extend(projection::packet_triples(&packet));
                    if let Some(object_index) = object_index {
                        let key = event.value_key.clone().unwrap_or_else(|| packet.value_key.clone());
                        push_unique(
                            object_index
                                .value_key_packet_ids
                                .entry(key)
                                .or_default(),
                            packet.packet_id.clone(),
                        );
                    }
                    state.packets.insert(packet.packet_id.clone(), packet);
                }
            }
        }
    }

    touched_program_ids.sort();
    touched_program_ids.dedup();
    touched_object_ids.sort();
    touched_object_ids.dedup();

    AppliedEventSummary {
        ingested_count: events.len(),
        program_ids: touched_program_ids,
        object_ids: touched_object_ids,
    }
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|existing| existing == &value) {
        items.push(value);
    }
}

fn collect_by_ids<T: Clone>(
    store: &HashMap<String, T>,
    value_specific: Option<&Vec<String>>,
    object_level: &Vec<String>,
) -> Vec<T> {
    let mut results = Vec::new();
    let mut seen = HashMap::new();
    for id in value_specific.into_iter().flatten().chain(object_level.iter()) {
        if seen.insert(id.clone(), true).is_none() {
            if let Some(item) = store.get(id) {
                results.push(item.clone());
            }
        }
    }
    results
}

fn collect_all_by_ids<T: Clone>(store: &HashMap<String, T>, index: &ObjectIndex) -> Vec<T> {
    let mut ids = Vec::new();
    ids.extend(index.object_level_control_ids.clone());
    ids.extend(index.value_key_control_ids.values().flatten().cloned());
    ids.extend(index.object_level_exception_ids.clone());
    ids.extend(index.value_key_exception_ids.values().flatten().cloned());
    ids.extend(index.object_level_approval_ids.clone());
    ids.extend(index.value_key_approval_ids.values().flatten().cloned());
    ids.sort();
    ids.dedup();
    ids.into_iter().filter_map(|id| store.get(&id).cloned()).collect()
}

fn primary_execution(events: Vec<ExecutionEvent>) -> Option<ExecutionEvent> {
    let mut non_verification = events
        .iter()
        .filter(|event| !event.stage.eq_ignore_ascii_case("verification"))
        .cloned()
        .collect::<Vec<_>>();
    if !non_verification.is_empty() {
        return non_verification
            .drain(..)
            .max_by_key(|event| event.happened_at);
    }

    events.into_iter().max_by_key(|event| event.happened_at)
}

fn build_narrative(explanation: &ValueExplanation) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "{} maps to {}",
        explanation.source_field.field_path, explanation.target_field.field_path
    ));
    if let Some(rule) = &explanation.transformation_rule {
        parts.push(format!("via rule '{}'", rule.name));
    }
    if let Some(execution) = &explanation.execution_event {
        parts.push(format!("during {} stage '{}'", execution.tool_name, execution.stage));
    }
    if !explanation.controls.is_empty() {
        parts.push(format!("with {} control result(s)", explanation.controls.len()));
    }
    if !explanation.approvals.is_empty() {
        parts.push(format!("and {} approval event(s)", explanation.approvals.len()));
    }
    parts.join(" ")
}

fn find_existing_packet(
    state: &crate::store::TraceabilityState,
    object: &MigrationObject,
    requested_value_key: Option<&str>,
) -> Option<EvidencePacket> {
    let index = state.object_indexes.get(&object.object_id)?;

    let mut candidate_ids = Vec::new();
    match requested_value_key {
        Some(value_key) => {
            for key in value_key_candidates(object, value_key) {
                if let Some(ids) = index.value_key_packet_ids.get(&key) {
                    candidate_ids.extend(ids.iter().cloned());
                }
            }
        }
        None => {
            candidate_ids.extend(index.object_level_packet_ids.iter().cloned());
            candidate_ids.extend(index.value_key_packet_ids.values().flatten().cloned());
        }
    }

    candidate_ids.sort();
    candidate_ids.dedup();

    candidate_ids
        .into_iter()
        .filter_map(|id| state.packets.get(&id).cloned())
        .max_by_key(|packet| packet.generated_at)
}

fn value_key_candidates(object: &MigrationObject, raw: &str) -> Vec<String> {
    let mut keys = vec![raw.to_string()];
    if !raw.contains("::") {
        if let Some(record_id) = object.target_record_id.as_deref() {
            keys.push(format!("{record_id}::{raw}"));
        }
        if let Some(record_id) = object.source_record_id.as_deref() {
            keys.push(format!("{record_id}::{raw}"));
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

#[derive(Clone)]
pub struct TraceabilityServiceImpl {
    manager: TraceabilityManager,
    started_at: Instant,
}

impl TraceabilityServiceImpl {
    pub fn new(manager: TraceabilityManager) -> Self {
        Self {
            manager,
            started_at: Instant::now(),
        }
    }
}

#[tonic::async_trait]
impl TraceabilityService for TraceabilityServiceImpl {
    async fn ingest_events(
        &self,
        request: Request<IngestEventsRequest>,
    ) -> Result<Response<IngestEventsResponse>, Status> {
        let events = request
            .into_inner()
            .event_json
            .into_iter()
            .map(|item| deserialize::<MigrationEvidenceEvent>(&item))
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal_status)?;
        let (ingested_count, program_ids, object_ids) = self
            .manager
            .ingest_events(events)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(IngestEventsResponse {
            ingested_count: ingested_count as i64,
            program_ids,
            object_ids,
        }))
    }

    async fn explain_value(
        &self,
        request: Request<ExplainValueRequest>,
    ) -> Result<Response<ValueExplanationReply>, Status> {
        let req = request.into_inner();
        let explanation = self
            .manager
            .explain_value(ValueLocator {
                program_id: req.program_id,
                object_id: req.object_id,
                target_field_path: req.target_field_path,
                target_record_id: if req.target_record_id.is_empty() { None } else { Some(req.target_record_id) },
                source_record_id: if req.source_record_id.is_empty() { None } else { Some(req.source_record_id) },
            })
            .await
            .map_err(internal_status)?;
        Ok(Response::new(ValueExplanationReply {
            value_explanation_json: serialize(&explanation).map_err(internal_status)?,
        }))
    }

    async fn get_evidence_packet(
        &self,
        request: Request<GetEvidencePacketRequest>,
    ) -> Result<Response<EvidencePacketReply>, Status> {
        let req = request.into_inner();
        let packet = self
            .manager
            .evidence_packet_for_object(
                &req.object_id,
                if req.value_key.is_empty() { None } else { Some(req.value_key.as_str()) },
            )
            .await
            .map_err(internal_status)?;
        Ok(Response::new(EvidencePacketReply {
            evidence_packet_json: serialize(&packet).map_err(internal_status)?,
        }))
    }

    async fn get_controls(
        &self,
        request: Request<GetObjectRequest>,
    ) -> Result<Response<ControlsReply>, Status> {
        let req = request.into_inner();
        let controls = self.manager.controls_for_object(&req.object_id).await.map_err(internal_status)?;
        Ok(Response::new(ControlsReply {
            control_json: controls
                .into_iter()
                .map(|control| serialize(&control))
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal_status)?,
        }))
    }

    async fn get_exceptions(
        &self,
        request: Request<GetProgramRequest>,
    ) -> Result<Response<ExceptionsReply>, Status> {
        let req = request.into_inner();
        let exceptions = self.manager.exceptions_for_program(&req.program_id).await.map_err(internal_status)?;
        Ok(Response::new(ExceptionsReply {
            exception_json: exceptions
                .into_iter()
                .map(|item| serialize(&item))
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal_status)?,
        }))
    }

    async fn get_approvals(
        &self,
        request: Request<GetProgramRequest>,
    ) -> Result<Response<ApprovalsReply>, Status> {
        let req = request.into_inner();
        let approvals = self.manager.approvals_for_program(&req.program_id).await.map_err(internal_status)?;
        Ok(Response::new(ApprovalsReply {
            approval_json: approvals
                .into_iter()
                .map(|item| serialize(&item))
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal_status)?,
        }))
    }

    async fn get_runtime_status(
        &self,
        _request: Request<RuntimeStatusRequest>,
    ) -> Result<Response<RuntimeStatusReply>, Status> {
        let status = self.manager.runtime_status().await.map_err(internal_status)?;
        Ok(Response::new(RuntimeStatusReply {
            runtime_status_json: serialize(&status).map_err(internal_status)?,
        }))
    }

    async fn rebuild_read_models(
        &self,
        _request: Request<RebuildReadModelsRequest>,
    ) -> Result<Response<RebuildReadModelsResponse>, Status> {
        let summary = self
            .manager
            .rebuild_read_models()
            .await
            .map_err(internal_status)?;
        Ok(Response::new(RebuildReadModelsResponse {
            rebuild_summary_json: serialize(&summary).map_err(internal_status)?,
        }))
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            healthy: true,
            service_name: "arcxa-traceability".to_string(),
            uptime_seconds: self.started_at.elapsed().as_secs() as i64,
        }))
    }
}

fn serialize<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("failed to serialize response")
}

fn deserialize<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).context("failed to deserialize request payload")
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::migration_evidence::{
        ApprovalStatus, ControlStatus, ExecutionStatus, MigrationConnectorVendor,
        MigrationEvidenceArtifactType, MigrationObjectType, TransformationRuleType,
    };
    use serde_json::json;
    use tempfile::tempdir;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[5u8; 32])
    }

    #[tokio::test]
    async fn explain_value_returns_signed_evidence_packet() {
        let temp = tempdir().unwrap();
        let store = PersistedTraceabilityStore::open(temp.path().join("traceability.json"))
            .await
            .unwrap();
        let manager = TraceabilityManager::new(
            store,
            signing_key(),
            GraphProjectionConfig::default(),
            EventBusRuntimeMonitor::direct(),
        );

        let program = MigrationProgram {
            program_id: "program-1".to_string(),
            name: "RISE wave 1".to_string(),
            customer_name: None,
            source_landscape: None,
            target_landscape: None,
            tags: vec![],
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let object = MigrationObject {
            object_id: "object-1".to_string(),
            program_id: "program-1".to_string(),
            object_type: MigrationObjectType::BusinessObject,
            name: "SalesOrder".to_string(),
            description: None,
            source_record_id: Some("SO-1".to_string()),
            target_record_id: Some("SO-1".to_string()),
            tags: vec![],
            metadata: HashMap::new(),
        };
        let rule = TransformationRule {
            rule_id: "rule-1".to_string(),
            rule_type: TransformationRuleType::Mapping,
            name: "Map net amount".to_string(),
            description: None,
            source_fields: vec![graphica_core::migration_evidence::SourceFieldRef {
                system: "ECC".to_string(),
                object_name: "VBAK".to_string(),
                field_name: "NETWR".to_string(),
                field_path: "$.amount".to_string(),
                semantic_type: None,
                record_id: Some("SO-1".to_string()),
            }],
            target_fields: vec![graphica_core::migration_evidence::TargetFieldRef {
                system: "S4".to_string(),
                object_name: "A_SalesOrder".to_string(),
                field_name: "NetAmount".to_string(),
                field_path: "$.amount".to_string(),
                semantic_type: None,
                record_id: Some("SO-1".to_string()),
            }],
            expression: Some("NETWR * 1.0".to_string()),
            filter_predicate: None,
            default_value: None,
            aggregation: None,
            metadata: HashMap::new(),
        };
        let execution = ExecutionEvent {
            execution_id: "exec-1".to_string(),
            program_id: "program-1".to_string(),
            object_id: "object-1".to_string(),
            connector_run_id: "run-1".to_string(),
            tool_name: "ibm_rapid_move".to_string(),
            tool_run_id: "ibm-run-1".to_string(),
            stage: "load".to_string(),
            status: ExecutionStatus::Succeeded,
            happened_at: Utc::now(),
            source_snapshot_ref: None,
            target_snapshot_ref: None,
            records_examined: Some(1),
            records_affected: Some(1),
            metadata: HashMap::new(),
        };
        let control = ControlResult {
            control_id: "control-1".to_string(),
            program_id: "program-1".to_string(),
            object_id: "object-1".to_string(),
            control_name: "match amount".to_string(),
            control_type: "reconciliation".to_string(),
            status: ControlStatus::Passed,
            summary: "matched".to_string(),
            expected_value: Some(json!(10)),
            actual_value: Some(json!(10)),
            tolerance: Some(0.0),
            executed_at: Utc::now(),
            evidence_refs: vec![],
            metadata: HashMap::new(),
        };
        let approval = ApprovalEvent {
            approval_id: "approval-1".to_string(),
            program_id: "program-1".to_string(),
            object_id: "object-1".to_string(),
            approver_role: "data_owner".to_string(),
            approver_id: "owner-1".to_string(),
            status: ApprovalStatus::Approved,
            comment: None,
            approved_at: Utc::now(),
            evidence_refs: vec![],
            attestation_ref: None,
            metadata: HashMap::new(),
        };

        manager
            .ingest_events(vec![
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-1",
                    "run-1",
                    MigrationConnectorVendor::IbmRapidMove,
                    "program-1",
                    "object-1",
                    MigrationEvidenceArtifactType::Program,
                    None,
                    serde_json::to_value(program).unwrap(),
                ),
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-1",
                    "run-1",
                    MigrationConnectorVendor::IbmRapidMove,
                    "program-1",
                    "object-1",
                    MigrationEvidenceArtifactType::Object,
                    None,
                    serde_json::to_value(object).unwrap(),
                ),
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-1",
                    "run-1",
                    MigrationConnectorVendor::IbmRapidMove,
                    "program-1",
                    "object-1",
                    MigrationEvidenceArtifactType::TransformationRule,
                    Some("SO-1::$.amount".to_string()),
                    serde_json::to_value(rule).unwrap(),
                ),
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-1",
                    "run-1",
                    MigrationConnectorVendor::IbmRapidMove,
                    "program-1",
                    "object-1",
                    MigrationEvidenceArtifactType::ExecutionEvent,
                    Some("SO-1::$.amount".to_string()),
                    serde_json::to_value(execution).unwrap(),
                ),
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-1",
                    "run-1",
                    MigrationConnectorVendor::SapHana,
                    "program-1",
                    "object-1",
                    MigrationEvidenceArtifactType::ControlResult,
                    Some("SO-1::$.amount".to_string()),
                    serde_json::to_value(control).unwrap(),
                ),
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-1",
                    "run-1",
                    MigrationConnectorVendor::Generic,
                    "program-1",
                    "object-1",
                    MigrationEvidenceArtifactType::ApprovalEvent,
                    None,
                    serde_json::to_value(approval).unwrap(),
                ),
            ])
            .await
            .unwrap();

        let explanation = manager
            .explain_value(ValueLocator {
                program_id: "program-1".to_string(),
                object_id: "object-1".to_string(),
                target_field_path: "$.amount".to_string(),
                target_record_id: Some("SO-1".to_string()),
                source_record_id: Some("SO-1".to_string()),
            })
            .await
            .unwrap();

        assert_eq!(explanation.source_field.field_name, "NETWR");
        assert_eq!(explanation.target_field.field_name, "NetAmount");
        assert_eq!(explanation.controls.len(), 1);
        assert_eq!(explanation.approvals.len(), 1);
        assert!(explanation.evidence_packet_id.is_some());

        let packet = manager
            .evidence_packet_for_object("object-1", Some("$.amount"))
            .await
            .unwrap();
        assert!(packet.signature.is_some());
        assert!(verify_evidence_packet_signature(&packet, packet.signature.as_ref().unwrap()));

        let packet_again = manager
            .evidence_packet_for_object("object-1", Some("$.amount"))
            .await
            .unwrap();
        assert_eq!(
            packet_again.packet_id, packet.packet_id,
            "field-path lookups should resolve the already persisted packet"
        );
    }

    #[tokio::test]
    async fn rebuild_read_models_replays_persisted_events_and_sets_runtime_metadata() {
        let temp = tempdir().unwrap();
        let store = PersistedTraceabilityStore::open_rocksdb(temp.path().join("traceability.db"), None)
            .await
            .unwrap();
        let manager = TraceabilityManager::new(
            store,
            signing_key(),
            GraphProjectionConfig::default(),
            EventBusRuntimeMonitor::direct(),
        );

        let program = MigrationProgram {
            program_id: "program-rebuild".to_string(),
            name: "RISE rebuild".to_string(),
            customer_name: None,
            source_landscape: None,
            target_landscape: None,
            tags: vec![],
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let object = MigrationObject {
            object_id: "object-rebuild".to_string(),
            program_id: "program-rebuild".to_string(),
            object_type: MigrationObjectType::BusinessObject,
            name: "BusinessPartner".to_string(),
            description: None,
            source_record_id: Some("BP-1".to_string()),
            target_record_id: Some("BP-1".to_string()),
            tags: vec![],
            metadata: HashMap::new(),
        };

        manager
            .ingest_events(vec![
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-rebuild",
                    "run-rebuild",
                    MigrationConnectorVendor::IbmRapidMove,
                    "program-rebuild",
                    "object-rebuild",
                    MigrationEvidenceArtifactType::Program,
                    None,
                    serde_json::to_value(program).unwrap(),
                ),
                graphica_core::migration_evidence::MigrationEvidenceEvent::new(
                    "connector-rebuild",
                    "run-rebuild",
                    MigrationConnectorVendor::IbmRapidMove,
                    "program-rebuild",
                    "object-rebuild",
                    MigrationEvidenceArtifactType::Object,
                    None,
                    serde_json::to_value(object).unwrap(),
                ),
            ])
            .await
            .unwrap();

        let before = manager.runtime_status().await.unwrap();
        assert_eq!(before.read_models.programs, 1);
        assert_eq!(before.read_models.objects, 1);
        assert_eq!(before.read_models.event_log_entries, 2);
        assert!(before.last_rebuild_at.is_none());

        let rebuild = manager.rebuild_read_models().await.unwrap();
        assert_eq!(rebuild.replayed_event_count, 2);
        assert_eq!(rebuild.touched_program_count, 1);
        assert_eq!(rebuild.touched_object_count, 1);

        let after = manager.runtime_status().await.unwrap();
        assert_eq!(after.read_models.programs, 1);
        assert_eq!(after.read_models.objects, 1);
        assert_eq!(after.read_models.event_log_entries, 2);
        assert_eq!(after.last_event_sequence, 2);
        assert!(after.last_rebuild_at.is_some());
    }
}
