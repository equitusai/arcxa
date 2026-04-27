import { api } from './client';

export interface SosErrorResponse {
  error: string;
  message: string;
  details?: unknown;
}

export interface SosInterfaceRecord {
  system_id: string;
  interface_id: string;
  interface_name: string;
  direction: string;
  protocol: string;
  data_format: string;
  schema: unknown;
  coordinate_system?: string | null;
  unit_system?: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface SosSlaMetric {
  name: string;
  value: number;
  operator: string;
  unit?: string | null;
}

export interface SosDataContract {
  contract_id: string;
  revision?: number;
  contract_name: string;
  provider_interface_id: string;
  consumer_interface_id: string;
  sla_metrics: SosSlaMetric[];
  transformation_rules: Record<string, unknown>;
  description?: string | null;
  tags: string[];
  approved: boolean;
  signed: boolean;
  lifecycle_state?: string;
  approval_status?: string;
  approval_requested_by?: string | null;
  approval_requested_at?: string | null;
  approved_by?: string | null;
  approved_at?: string | null;
  signed_by?: string | null;
  signed_at?: string | null;
  signature?: SosContractSignature | null;
  created_by?: string;
  updated_by?: string;
  rejected_by?: string | null;
  rejected_at?: string | null;
  rejection_reason?: string | null;
  superseded_by_revision?: number | null;
  created_at: string;
  updated_at: string;
}

export interface LookupContractByPairParams {
  providerInterfaceId: string;
  consumerInterfaceId: string;
}

export interface SosValidationCheck {
  check_name: string;
  passed: boolean;
  severity: string;
  description: string;
  details?: unknown;
}

export interface SosValidationResponse {
  validation_id: string;
  passed: boolean;
  checks: SosValidationCheck[];
  confidence: number;
  validated_at: string;
  report_id?: string | null;
}

export interface SosCompatibilityDetail {
  aspect: string;
  compatible: boolean;
  explanation: string;
}

export interface SosCompatibilityScore {
  provider_interface_id: string;
  consumer_interface_id: string;
  score: number;
  details: SosCompatibilityDetail[];
}

export interface SosCompatibilityMatrixResponse {
  matrix: SosCompatibilityScore[];
  metadata?: {
    total_interfaces: number;
    total_candidate_pairs: number;
    evaluated_pairs: number;
    remaining_candidate_pairs: number;
    truncated: boolean;
    requested_evaluation_budget?: number | null;
    applied_evaluation_budget: number;
    server_evaluation_budget: number;
  };
  generated_at: string;
}

export interface SosSystemRecord {
  system_id: string;
  system_name: string;
  system_type: string;
  vendor: string;
  version: string;
  classification: string;
  description?: string | null;
  deployment: Record<string, unknown>;
  capabilities: Record<string, unknown>;
  tags: string[];
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface SosSystemsResponse {
  systems: SosSystemRecord[];
  total: number;
  offset: number;
  limit: number;
}

export interface ListSosSystemsParams {
  systemType?: string;
  vendor?: string;
  classification?: string;
  tags?: string;
  active?: boolean;
  offset?: number;
  limit?: number;
}

export interface CreateSosSystemRequest {
  system_id: string;
  system_name: string;
  system_type: string;
  vendor: string;
  version: string;
  classification: string;
  description?: string | null;
  deployment?: Record<string, unknown>;
  capabilities?: Record<string, unknown>;
  tags?: string[];
}

export interface UpdateSosSystemRequest {
  system_name?: string;
  version?: string;
  classification?: string;
  description?: string;
  deployment?: Record<string, unknown>;
  capabilities?: Record<string, unknown>;
  tags?: string[];
  active?: boolean;
}

export interface SosInterfaceDefinition {
  interface_id: string;
  interface_name: string;
  direction: string;
  protocol: string;
  data_format: string;
  schema: unknown;
  coordinate_system?: string | null;
  unit_system?: string | null;
  metadata: Record<string, unknown>;
}

export interface CreateSosInterfaceRequest {
  system_id: string;
  interface: SosInterfaceDefinition;
}

export interface UpdateSosInterfaceRequest {
  interface_name?: string;
  direction?: string;
  schema?: unknown;
  coordinate_system?: string;
  unit_system?: string;
  metadata?: Record<string, unknown>;
}

export interface CreateSosContractRequest {
  contract_id: string;
  contract_name: string;
  provider_interface_id: string;
  consumer_interface_id: string;
  sla_metrics: SosSlaMetric[];
  transformation_rules?: Record<string, unknown>;
  description?: string | null;
  tags?: string[];
}

export interface UpdateSosContractRequest {
  contract_name?: string;
  sla_metrics?: SosSlaMetric[];
  transformation_rules?: Record<string, unknown>;
  description?: string;
  tags?: string[];
  approved?: boolean;
}

export async function listSosInterfaces(): Promise<SosInterfaceRecord[]> {
  return api.get<SosInterfaceRecord[]>('/sos/interfaces');
}

export async function listSosContracts(): Promise<SosDataContract[]> {
  return api.get<SosDataContract[]>('/sos/contracts');
}

export async function lookupContractByInterfacePair(
  params: LookupContractByPairParams
): Promise<SosDataContract> {
  return api.get<SosDataContract>('/sos/contracts/lookup', {
    params: {
      provider_interface_id: params.providerInterfaceId,
      consumer_interface_id: params.consumerInterfaceId,
    },
  });
}

export async function validateInterfaceCompatibility(
  params: LookupContractByPairParams
): Promise<SosValidationResponse> {
  return api.post<SosValidationResponse>('/sos/validate', {
    type: 'interface_compatibility',
    provider_interface_id: params.providerInterfaceId,
    consumer_interface_id: params.consumerInterfaceId,
  });
}

export async function getCompatibilityMatrix(): Promise<SosCompatibilityMatrixResponse> {
  return api.get<SosCompatibilityMatrixResponse>('/sos/compatibility-matrix');
}

export interface SosValidationChangeSummary {
  resolved_checks: string[];
  new_failures: string[];
  confidence_delta: number;
  schema_or_policy_version_changed: boolean;
}

export interface SosValidationReport {
  report_id: string;
  validation_id: string;
  subject_type: string;
  subject_key: string;
  validation_type: string;
  passed: boolean;
  confidence: number;
  checks: SosValidationCheck[];
  validated_at: string;
  previous_report_id?: string | null;
  change_summary: SosValidationChangeSummary;
  workflow_execution_id?: string | null;
  workflow_step_id?: string | null;
  ontology_refs: string[];
  shape_refs: string[];
  policy_refs: string[];
  schema_hashes: Record<string, string>;
}

export interface SosValidationHistoryResponse {
  subject_type: string;
  subject_key: string;
  reports: SosValidationReport[];
}

export interface SosValidationLineageEdge {
  from_report_id: string;
  to_report_id: string;
  relationship: string;
}

export interface SosValidationLineageResponse {
  subject_type: string;
  subject_key: string;
  reports: SosValidationReport[];
  edges: SosValidationLineageEdge[];
}

export interface ValidationSubjectQuery {
  subjectKey: string;
  subjectType?: string;
  limit?: number;
}

export interface SosPolicyRecord {
  policy_id: string;
  revision?: number;
  policy_ref?: string;
  policy_revision_ref?: string;
  policy_name: string;
  description?: string | null;
  lifecycle_state?: string;
  approval_status?: string;
  approval_requested_by?: string | null;
  approval_requested_at?: string | null;
  approved_by?: string | null;
  approved_at?: string | null;
  rejected_by?: string | null;
  rejected_at?: string | null;
  rejection_reason?: string | null;
  target_type: string;
  target_key?: string | null;
  stages: string[];
  enforcement_level: string;
  severity: string;
  sparql_query: string;
  context: Record<string, unknown>;
  tags: string[];
  ontology_refs: string[];
  shape_refs: string[];
  active: boolean;
  provider_interface_id?: string | null;
  consumer_interface_id?: string | null;
  contract_id?: string | null;
  source_system_id?: string | null;
  target_system_id?: string | null;
  interface_id?: string | null;
  attestation?: SosPolicyAttestation | null;
  created_by?: string;
  updated_by?: string;
  superseded_by_revision?: number | null;
  created_at: string;
  updated_at: string;
}

export interface SosPoliciesResponse {
  policies: SosPolicyRecord[];
  total: number;
  offset: number;
  limit: number;
}

export interface ListSosPoliciesParams {
  targetType?: string;
  stage?: string;
  active?: boolean;
  offset?: number;
  limit?: number;
}

export interface CreateSosPolicyRequest {
  policy_id: string;
  policy_name: string;
  target_type: string;
  stages?: string[];
  enforcement_level?: string;
  severity?: string;
  sparql_query: string;
  context?: Record<string, unknown>;
  description?: string | null;
  tags?: string[];
  ontology_refs?: string[];
  shape_refs?: string[];
  active?: boolean;
  provider_interface_id?: string;
  consumer_interface_id?: string;
  contract_id?: string;
  source_system_id?: string;
  target_system_id?: string;
  interface_id?: string;
}

export interface UpdateSosPolicyRequest {
  policy_name?: string;
  target_type?: string;
  stages?: string[];
  enforcement_level?: string;
  severity?: string;
  sparql_query?: string;
  context?: Record<string, unknown>;
  description?: string | null;
  tags?: string[];
  ontology_refs?: string[];
  shape_refs?: string[];
  active?: boolean;
  provider_interface_id?: string;
  consumer_interface_id?: string;
  contract_id?: string;
  source_system_id?: string;
  target_system_id?: string;
  interface_id?: string;
}

export interface EvaluateSosPolicyRequest {
  stage?: string;
  context?: Record<string, unknown>;
}

export interface ValidateSosInterfaceSchemaRequest {
  interfaceId: string;
  data: unknown;
}

export interface SosDependencyGraphNode {
  id: string;
  kind: string;
  label: string;
  system_id?: string;
  system_type?: string;
}

export interface SosDependencyGraphEdge {
  from: string;
  to: string;
  kind: string;
  contract_id?: string;
}

export interface SosDependencyGraphResponse {
  generated_at: string;
  nodes: SosDependencyGraphNode[];
  edges: SosDependencyGraphEdge[];
  metadata?: {
    total_nodes: number;
    total_edges: number;
    returned_nodes: number;
    returned_edges: number;
    remaining_nodes: number;
    remaining_edges: number;
    truncated: boolean;
    requested_node_budget?: number | null;
    applied_node_budget: number;
    server_node_budget: number;
    requested_edge_budget?: number | null;
    applied_edge_budget: number;
    server_edge_budget: number;
  };
}

export interface SosWhatIfRequest {
  scenario: string;
  changes: unknown[];
  evaluation_budget?: number;
}

export interface SosWhatIfResponse {
  scenario_id: string;
  impact: string[];
  affected_entities: string[];
  recommendations: string[];
  metadata?: {
    total_candidate_evaluations: number;
    evaluated_candidate_evaluations: number;
    remaining_candidate_evaluations: number;
    truncated: boolean;
    requested_evaluation_budget?: number | null;
    applied_evaluation_budget: number;
    server_evaluation_budget: number;
  };
}

export interface ListSosContractApprovalRequestsParams {
  contractId: string;
  status?: string;
  offset?: number;
  limit?: number;
}

export interface SosContractApprovalEvidence {
  evidence_id: string;
  request_id: string;
  contract_id: string;
  contract_revision: number;
  evidence_type: string;
  report_id: string;
  added_by: string;
  added_at: string;
  note?: string | null;
  metadata: Record<string, unknown>;
}

export interface SosContractApprovalRequest {
  request_id: string;
  contract_id: string;
  contract_revision: number;
  approval_type: string;
  requested_lifecycle_state: string;
  status: string;
  note?: string | null;
  requested_by: string;
  requested_at: string;
  expires_at?: string | null;
  approved_by?: string | null;
  approved_at?: string | null;
  rejected_by?: string | null;
  rejected_at?: string | null;
  rejection_reason?: string | null;
  metadata: Record<string, unknown>;
  evidence: SosContractApprovalEvidence[];
}

export interface SosContractApprovalRequestsResponse {
  requests: SosContractApprovalRequest[];
  total: number;
  offset: number;
  limit: number;
}

export interface SosContractSignature {
  signature_id: string;
  contract_id: string;
  contract_revision: number;
  contract_revision_ref: string;
  payload_hash: string;
  payload_hash_algorithm: string;
  signature_algorithm: string;
  signature: string;
  public_key: string;
  key_fingerprint: string;
  signing_key_ref?: string | null;
  signing_key_version?: string | null;
  signing_key_source: string;
  signed_by: string;
  signed_at: string;
  approval_request_id?: string | null;
  evidence_ids: string[];
  policy_refs: string[];
  signature_verified: boolean;
  metadata: Record<string, unknown>;
}

export interface SosContractSignaturesResponse {
  signatures: SosContractSignature[];
  total: number;
  limit: number;
}

export interface SosContractSigningKeyStatus {
  signing_key_ref?: string | null;
  signing_key_source: string;
  signing_key_version?: string | null;
  public_key: string;
  key_fingerprint: string;
  supports_rotation: boolean;
  description?: string | null;
  tags: string[];
  owner?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  rotation_interval_days?: number | null;
  rotation_last_rotated_at?: string | null;
  rotation_next_due_at?: string | null;
  rotation_auto_rotate?: boolean | null;
  metadata: Record<string, unknown>;
}

export interface RotateSosContractSigningKeyRequest {
  reason?: string;
}

export interface RotateSosContractSigningKeyResponse {
  signing_key_ref: string;
  previous_signing_key_version?: string | null;
  previous_key_fingerprint?: string | null;
  current_signing_key_version: string;
  current_key_fingerprint: string;
  current_public_key: string;
  rotated_by: string;
  rotated_at: string;
  metadata: Record<string, unknown>;
}

export interface ListSosPolicyApprovalRequestsParams {
  policyId: string;
  status?: string;
  offset?: number;
  limit?: number;
}

export interface SosPolicyApprovalEvidence {
  evidence_id: string;
  request_id: string;
  policy_id: string;
  policy_revision: number;
  policy_revision_ref: string;
  evidence_type: string;
  report_id: string;
  added_by: string;
  added_at: string;
  note?: string | null;
  metadata: Record<string, unknown>;
}

export interface SosPolicyApprovalRequest {
  request_id: string;
  policy_id: string;
  policy_revision: number;
  policy_revision_ref: string;
  approval_type: string;
  requested_lifecycle_state: string;
  status: string;
  note?: string | null;
  requested_by: string;
  requested_at: string;
  expires_at?: string | null;
  approved_by?: string | null;
  approved_at?: string | null;
  rejected_by?: string | null;
  rejected_at?: string | null;
  rejection_reason?: string | null;
  metadata: Record<string, unknown>;
  evidence: SosPolicyApprovalEvidence[];
}

export interface SosPolicyApprovalRequestsResponse {
  requests: SosPolicyApprovalRequest[];
  total: number;
  offset: number;
  limit: number;
}

export interface SosPolicyAttestation {
  attestation_id: string;
  policy_id: string;
  policy_revision: number;
  policy_revision_ref: string;
  payload_hash: string;
  payload_hash_algorithm: string;
  signature_algorithm: string;
  signature: string;
  public_key: string;
  key_fingerprint: string;
  signing_key_ref?: string | null;
  signing_key_version?: string | null;
  signing_key_source: string;
  trust_mode: string;
  trust_provider?: string | null;
  external_key_ref?: string | null;
  trust_attestation_ref?: string | null;
  attested_by: string;
  attested_at: string;
  approval_request_id?: string | null;
  evidence_ids: string[];
  policy_refs: string[];
  attestation_verified: boolean;
  metadata: Record<string, unknown>;
}

export interface SosPolicyAttestationsResponse {
  attestations: SosPolicyAttestation[];
  total: number;
  limit: number;
}

export interface SosPolicySigningKeyStatus {
  signing_key_ref?: string | null;
  signing_key_source: string;
  signing_key_version?: string | null;
  public_key: string;
  key_fingerprint: string;
  supports_rotation: boolean;
  trust_mode: string;
  trust_provider?: string | null;
  external_key_ref?: string | null;
  trust_attestation_ref?: string | null;
  description?: string | null;
  tags: string[];
  owner?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
  rotation_interval_days?: number | null;
  rotation_last_rotated_at?: string | null;
  rotation_next_due_at?: string | null;
  rotation_auto_rotate?: boolean | null;
  metadata: Record<string, unknown>;
}

export interface RotateSosPolicySigningKeyRequest {
  reason?: string;
  trust_mode?: string;
  trust_provider?: string;
  external_key_ref?: string;
  trust_attestation_ref?: string;
}

export interface RotateSosPolicySigningKeyResponse {
  signing_key_ref: string;
  previous_signing_key_version?: string | null;
  previous_key_fingerprint?: string | null;
  current_signing_key_version: string;
  current_key_fingerprint: string;
  current_public_key: string;
  rotated_by: string;
  rotated_at: string;
  trust_mode: string;
  trust_provider?: string | null;
  external_key_ref?: string | null;
  trust_attestation_ref?: string | null;
  metadata: Record<string, unknown>;
}

export interface SosReconcileRequest {
  include_ontology_sync?: boolean;
}

export interface SosReconcileResponse {
  triggered_by: string;
  include_ontology_sync: boolean;
  ontology_registry_available: boolean;
  ontology_sync_performed: boolean;
  graph_reconcile_performed: boolean;
  system_count: number;
  interface_count: number;
  contract_count: number;
  policy_count: number;
  started_at: string;
  completed_at: string;
  duration_ms: number;
}

export function buildInterfacePairSubjectKey(
  providerInterfaceId: string,
  consumerInterfaceId: string
): string {
  return `interface_pair:${providerInterfaceId}:${consumerInterfaceId}`;
}

export async function listSosSystems(
  params: ListSosSystemsParams = {}
): Promise<SosSystemsResponse> {
  return api.get<SosSystemsResponse>('/sos/systems', {
    params: {
      system_type: params.systemType,
      vendor: params.vendor,
      classification: params.classification,
      tags: params.tags,
      active: params.active,
      offset: params.offset,
      limit: params.limit,
    },
  });
}

export async function createSosSystem(
  request: CreateSosSystemRequest
): Promise<SosSystemRecord> {
  return api.post<SosSystemRecord>('/sos/systems', {
    ...request,
    deployment: request.deployment ?? {},
    capabilities: request.capabilities ?? {},
    tags: request.tags ?? [],
  });
}

export async function updateSosSystem(params: {
  id: string;
  request: UpdateSosSystemRequest;
}): Promise<SosSystemRecord> {
  return api.put<SosSystemRecord>(`/sos/systems/${encodeURIComponent(params.id)}`, params.request);
}

export async function deleteSosSystem(id: string): Promise<void> {
  return api.delete<void>(`/sos/systems/${encodeURIComponent(id)}`);
}

export async function listSosInterfacesForSystem(
  systemId: string
): Promise<SosInterfaceRecord[]> {
  return api.get<SosInterfaceRecord[]>(
    `/sos/systems/${encodeURIComponent(systemId)}/interfaces`
  );
}

export async function createSosInterface(
  request: CreateSosInterfaceRequest
): Promise<SosInterfaceRecord> {
  return api.post<SosInterfaceRecord>('/sos/interfaces', request);
}

export async function updateSosInterface(params: {
  id: string;
  request: UpdateSosInterfaceRequest;
}): Promise<SosInterfaceRecord> {
  return api.put<SosInterfaceRecord>(
    `/sos/interfaces/${encodeURIComponent(params.id)}`,
    params.request
  );
}

export async function deleteSosInterface(id: string): Promise<void> {
  return api.delete<void>(`/sos/interfaces/${encodeURIComponent(id)}`);
}

export async function createSosContract(
  request: CreateSosContractRequest
): Promise<SosDataContract> {
  return api.post<SosDataContract>('/sos/contracts', {
    ...request,
    transformation_rules: request.transformation_rules ?? {},
    tags: request.tags ?? [],
  });
}

export async function updateSosContract(params: {
  id: string;
  request: UpdateSosContractRequest;
}): Promise<SosDataContract> {
  return api.put<SosDataContract>(
    `/sos/contracts/${encodeURIComponent(params.id)}`,
    params.request
  );
}

export async function deleteSosContract(id: string): Promise<void> {
  return api.delete<void>(`/sos/contracts/${encodeURIComponent(id)}`);
}

export async function approveSosContract(id: string): Promise<SosDataContract> {
  return api.post<SosDataContract>(`/sos/contracts/${encodeURIComponent(id)}/approve`);
}

export async function signSosContract(id: string): Promise<SosDataContract> {
  return api.post<SosDataContract>(`/sos/contracts/${encodeURIComponent(id)}/sign`);
}

export async function getValidationReport(
  reportId: string
): Promise<SosValidationReport> {
  return api.get<SosValidationReport>(
    `/sos/validation-reports/${encodeURIComponent(reportId)}`
  );
}

export async function getValidationHistory(
  query: ValidationSubjectQuery
): Promise<SosValidationHistoryResponse> {
  return api.get<SosValidationHistoryResponse>('/sos/validation-history', {
    params: {
      subject_key: query.subjectKey,
      subject_type: query.subjectType,
      limit: query.limit,
    },
  });
}

export async function getValidationLineage(
  query: ValidationSubjectQuery
): Promise<SosValidationLineageResponse> {
  return api.get<SosValidationLineageResponse>('/sos/validation-lineage', {
    params: {
      subject_key: query.subjectKey,
      subject_type: query.subjectType,
      limit: query.limit,
    },
  });
}

export async function listSosPolicies(
  params: ListSosPoliciesParams = {}
): Promise<SosPoliciesResponse> {
  return api.get<SosPoliciesResponse>('/sos/policies', {
    params: {
      target_type: params.targetType,
      stage: params.stage,
      active: params.active,
      offset: params.offset,
      limit: params.limit,
    },
  });
}

export async function createSosPolicy(
  request: CreateSosPolicyRequest
): Promise<SosPolicyRecord> {
  return api.post<SosPolicyRecord>('/sos/policies', {
    ...request,
    stages: request.stages ?? ['pre_execution'],
    enforcement_level: request.enforcement_level ?? 'mandatory',
    severity: request.severity ?? 'medium',
    context: request.context ?? {},
    tags: request.tags ?? [],
    ontology_refs: request.ontology_refs ?? [],
    shape_refs: request.shape_refs ?? [],
    active: request.active ?? true,
  });
}

export async function updateSosPolicy(params: {
  id: string;
  request: UpdateSosPolicyRequest;
}): Promise<SosPolicyRecord> {
  return api.put<SosPolicyRecord>(
    `/sos/policies/${encodeURIComponent(params.id)}`,
    params.request
  );
}

export async function deleteSosPolicy(id: string): Promise<void> {
  return api.delete<void>(`/sos/policies/${encodeURIComponent(id)}`);
}

export async function validateSosPolicy(params: {
  id: string;
  request?: EvaluateSosPolicyRequest;
}): Promise<SosValidationResponse> {
  return api.post<SosValidationResponse>(
    `/sos/policies/${encodeURIComponent(params.id)}/validate`,
    {
      stage: params.request?.stage,
      context: params.request?.context ?? {},
    }
  );
}

export async function validateSosPolicyDryRun(params: {
  id: string;
  request?: EvaluateSosPolicyRequest;
}): Promise<SosValidationResponse> {
  return api.post<SosValidationResponse>(
    `/sos/policies/${encodeURIComponent(params.id)}/validate/dry-run`,
    {
      stage: params.request?.stage,
      context: params.request?.context ?? {},
    }
  );
}

export async function validateSosInterfaceSchema(
  request: ValidateSosInterfaceSchemaRequest
): Promise<SosValidationResponse> {
  return api.post<SosValidationResponse>(
    `/sos/interfaces/${encodeURIComponent(request.interfaceId)}/validate-schema`,
    request.data
  );
}

export async function getSosDependencyGraph(): Promise<SosDependencyGraphResponse> {
  return api.get<SosDependencyGraphResponse>('/sos/dependency-graph');
}

export async function runSosWhatIfAnalysis(
  request: SosWhatIfRequest
): Promise<SosWhatIfResponse> {
  return api.post<SosWhatIfResponse>('/sos/what-if', request);
}

export async function listSosContractApprovalRequests(
  params: ListSosContractApprovalRequestsParams
): Promise<SosContractApprovalRequestsResponse> {
  return api.get<SosContractApprovalRequestsResponse>(
    `/sos/contracts/${encodeURIComponent(params.contractId)}/approval-requests`,
    {
      params: {
        status: params.status,
        offset: params.offset,
        limit: params.limit,
      },
    }
  );
}

export async function listSosContractSignatures(
  contractId: string,
  limit?: number
): Promise<SosContractSignaturesResponse> {
  return api.get<SosContractSignaturesResponse>(
    `/sos/contracts/${encodeURIComponent(contractId)}/signatures`,
    {
      params: {
        limit,
      },
    }
  );
}

export async function getSosContractSigningKeyStatus(): Promise<SosContractSigningKeyStatus> {
  return api.get<SosContractSigningKeyStatus>('/sos/contracts/signing-key');
}

export async function rotateSosContractSigningKey(
  request: RotateSosContractSigningKeyRequest = {}
): Promise<RotateSosContractSigningKeyResponse> {
  return api.post<RotateSosContractSigningKeyResponse>(
    '/sos/contracts/signing-key/rotate',
    request
  );
}

export async function listSosPolicyApprovalRequests(
  params: ListSosPolicyApprovalRequestsParams
): Promise<SosPolicyApprovalRequestsResponse> {
  return api.get<SosPolicyApprovalRequestsResponse>(
    `/sos/policies/${encodeURIComponent(params.policyId)}/approval-requests`,
    {
      params: {
        status: params.status,
        offset: params.offset,
        limit: params.limit,
      },
    }
  );
}

export async function listSosPolicyAttestations(
  policyId: string,
  limit?: number
): Promise<SosPolicyAttestationsResponse> {
  return api.get<SosPolicyAttestationsResponse>(
    `/sos/policies/${encodeURIComponent(policyId)}/attestations`,
    {
      params: {
        limit,
      },
    }
  );
}

export async function getSosPolicySigningKeyStatus(): Promise<SosPolicySigningKeyStatus> {
  return api.get<SosPolicySigningKeyStatus>('/sos/policies/signing-key');
}

export async function rotateSosPolicySigningKey(
  request: RotateSosPolicySigningKeyRequest = {}
): Promise<RotateSosPolicySigningKeyResponse> {
  return api.post<RotateSosPolicySigningKeyResponse>(
    '/sos/policies/signing-key/rotate',
    request
  );
}

export async function reconcileSosRuntime(
  request: SosReconcileRequest = {}
): Promise<SosReconcileResponse> {
  return api.post<SosReconcileResponse>('/sos/reconcile', request);
}
