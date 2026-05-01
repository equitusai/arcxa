import { api } from './client';

export interface MigrationEvidenceErrorResponse {
  error: string;
  details?: Record<string, string>;
}

export type MigrationConnectorVendor =
  | 'ibm_rapid_move'
  | 'snp_crystal_bridge'
  | 'smart_shift'
  | 'sap_hana'
  | 'sap_ecc'
  | 'sap_s4'
  | 'generic';

export type MigrationConnectorRole = 'migration_artifact_source' | 'verification_source';
export type ConnectorTransport = 'http_json' | 'sap_hana_sql' | 'manual_drop';
export type ConnectorAuthKind = 'none' | 'bearer' | 'api_key' | 'basic';
export type MigrationObjectType =
  | 'table'
  | 'business_object'
  | 'api_entity'
  | 'interface'
  | 'custom_code_artifact'
  | 'record'
  | 'field_group';
export type TransformationRuleType =
  | 'mapping'
  | 'conversion'
  | 'harmonization'
  | 'filter'
  | 'default_value'
  | 'aggregation'
  | 'enrichment';
export type ExecutionStatus = 'succeeded' | 'failed' | 'partial' | 'running';
export type ExceptionSeverity = 'info' | 'warning' | 'error' | 'critical';
export type ExceptionStatus = 'open' | 'overridden' | 'remediated' | 'accepted';
export type ControlStatus = 'passed' | 'failed' | 'warning' | 'not_run';
export type ApprovalStatus = 'pending' | 'approved' | 'rejected' | 'waived';
export type MigrationEvidenceDeliveryMode = 'direct' | 'kafka';

export interface ConnectorAuth {
  kind: ConnectorAuthKind;
  secret_ref?: string | null;
  token?: string | null;
  api_key?: string | null;
  header_name?: string | null;
  username?: string | null;
  password?: string | null;
}

export interface ConnectorEndpoint {
  base_url: string;
  path: string;
  method: string;
  headers: Record<string, string>;
}

export interface MigrationConnector {
  connector_id: string;
  name: string;
  vendor: MigrationConnectorVendor;
  role: MigrationConnectorRole;
  transport: ConnectorTransport;
  program_id: string;
  endpoint: ConnectorEndpoint;
  auth: ConnectorAuth;
  schedule?: string | null;
  enabled: boolean;
  metadata: Record<string, string>;
  created_at: string;
  updated_at: string;
}

export interface SourceFieldRef {
  system: string;
  object_name: string;
  field_name: string;
  field_path: string;
  semantic_type?: string | null;
  record_id?: string | null;
}

export interface TargetFieldRef {
  system: string;
  object_name: string;
  field_name: string;
  field_path: string;
  semantic_type?: string | null;
  record_id?: string | null;
}

export interface TransformationRule {
  rule_id: string;
  rule_type: TransformationRuleType;
  name: string;
  description?: string | null;
  source_fields: SourceFieldRef[];
  target_fields: TargetFieldRef[];
  expression?: string | null;
  filter_predicate?: string | null;
  default_value?: unknown;
  aggregation?: string | null;
  metadata: Record<string, string>;
}

export interface ExecutionEvent {
  execution_id: string;
  program_id: string;
  object_id: string;
  connector_run_id: string;
  tool_name: string;
  tool_run_id: string;
  stage: string;
  status: ExecutionStatus;
  happened_at: string;
  source_snapshot_ref?: string | null;
  target_snapshot_ref?: string | null;
  records_examined?: number | null;
  records_affected?: number | null;
  metadata: Record<string, string>;
}

export interface ExceptionRecord {
  exception_id: string;
  program_id: string;
  object_id: string;
  severity: ExceptionSeverity;
  status: ExceptionStatus;
  category: string;
  message: string;
  source_value?: unknown;
  target_value?: unknown;
  remediation?: string | null;
  detected_at: string;
  resolved_at?: string | null;
  metadata: Record<string, string>;
}

export interface ControlResult {
  control_id: string;
  program_id: string;
  object_id: string;
  control_name: string;
  control_type: string;
  status: ControlStatus;
  summary: string;
  expected_value?: unknown;
  actual_value?: unknown;
  tolerance?: number | null;
  executed_at: string;
  evidence_refs: string[];
  metadata: Record<string, string>;
}

export interface ApprovalEvent {
  approval_id: string;
  program_id: string;
  object_id: string;
  approver_role: string;
  approver_id: string;
  status: ApprovalStatus;
  comment?: string | null;
  approved_at: string;
  evidence_refs: string[];
  attestation_ref?: string | null;
  metadata: Record<string, string>;
}

export interface EvidencePacketSignature {
  algorithm: string;
  payload_hash_algorithm: string;
  payload_hash: string;
  public_key: string;
  key_fingerprint: string;
  signature: string;
  signed_at: string;
}

export interface EvidencePacket {
  packet_id: string;
  program_id: string;
  object_id: string;
  value_key: string;
  generated_at: string;
  source_field: SourceFieldRef;
  target_field: TargetFieldRef;
  transformation_rule?: TransformationRule | null;
  execution_event?: ExecutionEvent | null;
  exceptions: ExceptionRecord[];
  controls: ControlResult[];
  approvals: ApprovalEvent[];
  graph_refs: string[];
  narrative?: string | null;
  signature?: EvidencePacketSignature | null;
  metadata: Record<string, string>;
}

export interface ValueLocator {
  program_id: string;
  object_id: string;
  target_field_path: string;
  target_record_id?: string | null;
  source_record_id?: string | null;
}

export interface ValueExplanation {
  explanation_id: string;
  locator: ValueLocator;
  source_field: SourceFieldRef;
  target_field: TargetFieldRef;
  source_value?: unknown;
  target_value?: unknown;
  transformation_rule?: TransformationRule | null;
  execution_event?: ExecutionEvent | null;
  exceptions: ExceptionRecord[];
  controls: ControlResult[];
  approvals: ApprovalEvent[];
  evidence_packet_id?: string | null;
  graph_refs: string[];
  confidence_summary?: string | null;
  generated_at: string;
}

export interface ConnectorRunSummary {
  run_id: string;
  connector_id: string;
  ingested_event_count: number;
  delivery_mode: MigrationEvidenceDeliveryMode;
  traceability_acknowledged: boolean;
  touched_program_ids: string[];
  touched_object_ids: string[];
  started_at: string;
  completed_at: string;
}

export interface UpsertMigrationConnectorResponse {
  connector: MigrationConnector;
}

export interface RunMigrationConnectorResponse {
  summary: ConnectorRunSummary;
  ingested_events: Array<Record<string, unknown>>;
}

export interface ExplainValueResponse {
  explanation: ValueExplanation;
}

export interface EvidencePacketResponse {
  packet: EvidencePacket;
}

export interface ObjectControlsResponse {
  controls: ControlResult[];
}

export interface ProgramExceptionsResponse {
  exceptions: ExceptionRecord[];
}

export interface ProgramApprovalsResponse {
  approvals: ApprovalEvent[];
}

export interface TraceabilityReadModelCounts {
  programs: number;
  objects: number;
  rules: number;
  executions: number;
  exceptions: number;
  controls: number;
  approvals: number;
  packets: number;
  object_indexes: number;
  program_object_links: number;
  event_log_entries: number;
}

export type MigrationEvidenceEventBusMode = 'direct' | 'kafka';
export type MigrationEvidenceEventConsumerState = 'disabled' | 'running' | 'recovering' | 'stopped';
export type MigrationEvidenceEventBusLagState = 'unknown' | 'caught_up' | 'backlog';
export type MigrationEvidenceBrokerReachability = 'unknown' | 'reachable' | 'degraded' | 'unreachable';
export type ConnectorStoreBackend = 'unknown' | 'file' | 'rocks_db';
export type ConnectorStoreHealth = 'unknown' | 'healthy' | 'degraded' | 'unavailable';

export interface MigrationEvidencePartitionLagStatus {
  partition: number;
  current_offset?: number | null;
  high_watermark?: number | null;
  estimated_lag_message_count?: number | null;
}

export interface MigrationEvidenceEventBusStatus {
  mode: MigrationEvidenceEventBusMode;
  async_delivery_enabled: boolean;
  consumer_state: MigrationEvidenceEventConsumerState;
  bootstrap_servers?: string | null;
  topic?: string | null;
  consumer_group?: string | null;
  processed_message_count: number;
  malformed_message_count: number;
  retry_attempt_count: number;
  lag_state: MigrationEvidenceEventBusLagState;
  estimated_lag_message_count?: number | null;
  broker_reachability: MigrationEvidenceBrokerReachability;
  discovered_broker_count?: number | null;
  assigned_partitions: number[];
  topic_partition_count?: number | null;
  partition_lag: MigrationEvidencePartitionLagStatus[];
  last_consumed_at?: string | null;
  last_successful_ingest_at?: string | null;
  last_retry_at?: string | null;
  lag_observed_at?: string | null;
  last_state_changed_at?: string | null;
  startup_completed_at?: string | null;
  startup_failure_reason?: string | null;
  last_assignment_at?: string | null;
  last_broker_probe_at?: string | null;
  lag_diagnostics?: string | null;
  last_error?: string | null;
}

export interface ConnectorStoreRuntimeStatus {
  backend: ConnectorStoreBackend;
  health: ConnectorStoreHealth;
  connector_count: number;
  writable: boolean;
  updated_at: string;
  last_successful_write_at?: string | null;
  legacy_imported_at?: string | null;
  last_error?: string | null;
}

export interface EvidenceIngestionRuntimeStatus {
  connector_store: ConnectorStoreRuntimeStatus;
  delivery_mode: 'direct' | 'kafka';
  verification_service_configured: boolean;
  updated_at: string;
}

export interface TraceabilityRuntimeStatus {
  backend: 'file' | 'rocks_db';
  replay_supported: boolean;
  event_log_available: boolean;
  read_models: TraceabilityReadModelCounts;
  event_bus: MigrationEvidenceEventBusStatus;
  last_event_sequence: number;
  updated_at: string;
  last_rebuild_at?: string | null;
  legacy_imported_at?: string | null;
}

export interface TraceabilityRebuildSummary {
  backend: 'file' | 'rocks_db';
  replayed_event_count: number;
  touched_program_count: number;
  touched_object_count: number;
  rebuilt_at: string;
}

export interface MigrationEvidenceRuntimeStatusResponse {
  status: TraceabilityRuntimeStatus;
  ingestion_status?: EvidenceIngestionRuntimeStatus | null;
}

export interface MigrationEvidenceRebuildResponse {
  summary: TraceabilityRebuildSummary;
}

export interface ExplainMigrationValueParams {
  programId: string;
  objectId: string;
  targetFieldPath: string;
  targetRecordId?: string;
  sourceRecordId?: string;
}

export interface GetEvidencePacketParams {
  objectId: string;
  valueKey?: string;
}

export async function upsertMigrationConnector(
  connector: Record<string, unknown>
): Promise<UpsertMigrationConnectorResponse> {
  return api.post<UpsertMigrationConnectorResponse>('/migration-evidence/connectors', connector);
}

export async function runMigrationConnector(params: {
  connectorId: string;
  request: Record<string, unknown>;
}): Promise<RunMigrationConnectorResponse> {
  return api.post<RunMigrationConnectorResponse>(
    `/migration-evidence/connectors/${encodeURIComponent(params.connectorId)}/runs`,
    params.request
  );
}

export async function explainMigrationValue(
  params: ExplainMigrationValueParams
): Promise<ExplainValueResponse> {
  return api.get<ExplainValueResponse>('/migration-evidence/values/explain', {
    params: {
      program_id: params.programId,
      object_id: params.objectId,
      target_field_path: params.targetFieldPath,
      target_record_id: params.targetRecordId,
      source_record_id: params.sourceRecordId,
    },
  });
}

export async function getMigrationEvidencePacket(
  params: GetEvidencePacketParams
): Promise<EvidencePacketResponse> {
  return api.get<EvidencePacketResponse>(
    `/migration-evidence/objects/${encodeURIComponent(params.objectId)}/evidence-packet`,
    {
      params: {
        value_key: params.valueKey,
      },
    }
  );
}

export async function getMigrationObjectControls(
  objectId: string
): Promise<ObjectControlsResponse> {
  return api.get<ObjectControlsResponse>(
    `/migration-evidence/objects/${encodeURIComponent(objectId)}/controls`
  );
}

export async function getMigrationProgramExceptions(
  programId: string
): Promise<ProgramExceptionsResponse> {
  return api.get<ProgramExceptionsResponse>(
    `/migration-evidence/programs/${encodeURIComponent(programId)}/exceptions`
  );
}

export async function getMigrationProgramApprovals(
  programId: string
): Promise<ProgramApprovalsResponse> {
  return api.get<ProgramApprovalsResponse>(
    `/migration-evidence/programs/${encodeURIComponent(programId)}/approvals`
  );
}

export async function getMigrationRuntimeStatus(): Promise<MigrationEvidenceRuntimeStatusResponse> {
  return api.get<MigrationEvidenceRuntimeStatusResponse>('/migration-evidence/runtime/status');
}

export async function rebuildMigrationReadModels(): Promise<MigrationEvidenceRebuildResponse> {
  return api.post<MigrationEvidenceRebuildResponse>('/migration-evidence/runtime/rebuild', {});
}
