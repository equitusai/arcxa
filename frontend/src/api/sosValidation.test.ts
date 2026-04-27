import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
    put: vi.fn(),
    delete: vi.fn(),
  },
}));

import { api } from './client';
import {
  approveSosContract,
  buildInterfacePairSubjectKey,
  createSosPolicy,
  createSosSystem,
  deleteSosContract,
  deleteSosPolicy,
  getSosContractSigningKeyStatus,
  getCompatibilityMatrix,
  getSosDependencyGraph,
  getSosPolicySigningKeyStatus,
  getValidationHistory,
  getValidationLineage,
  getValidationReport,
  listSosContracts,
  listSosContractApprovalRequests,
  listSosContractSignatures,
  listSosInterfaces,
  listSosPolicies,
  listSosPolicyApprovalRequests,
  listSosPolicyAttestations,
  listSosSystems,
  lookupContractByInterfacePair,
  reconcileSosRuntime,
  rotateSosContractSigningKey,
  rotateSosPolicySigningKey,
  runSosWhatIfAnalysis,
  signSosContract,
  updateSosPolicy,
  updateSosContract,
  validateInterfaceCompatibility,
  validateSosInterfaceSchema,
  validateSosPolicy,
  validateSosPolicyDryRun,
} from './sosValidation';

describe('sosValidation api helpers', () => {
  beforeEach(() => {
    vi.mocked(api.get).mockReset();
    vi.mocked(api.post).mockReset();
    vi.mocked(api.put).mockReset();
    vi.mocked(api.delete).mockReset();
  });

  it('loads the SoS interface catalogue from the dedicated endpoint', async () => {
    vi.mocked(api.get).mockResolvedValue([
      {
        system_id: 'sys.radar',
        interface_id: 'iface.radar.out',
        interface_name: 'Radar Track Output',
        direction: 'outbound',
        protocol: 'REST',
        data_format: 'JSON',
        schema: { type: 'object' },
        metadata: {},
        created_at: '2026-04-22T00:00:00Z',
        updated_at: '2026-04-22T00:00:00Z',
      },
    ]);

    await expect(listSosInterfaces()).resolves.toHaveLength(1);
    expect(api.get).toHaveBeenCalledWith('/sos/interfaces');
  });

  it('loads SoS contracts from the dedicated contract catalogue endpoint', async () => {
    vi.mocked(api.get).mockResolvedValue([
      {
        contract_id: 'contract.radar.to.c2',
        contract_name: 'Radar To C2',
        provider_interface_id: 'iface.radar.out',
        consumer_interface_id: 'iface.c2.in',
        sla_metrics: [],
        transformation_rules: {},
        tags: [],
        approved: true,
        signed: false,
        created_at: '2026-04-22T00:00:00Z',
        updated_at: '2026-04-22T00:00:00Z',
      },
    ]);

    await expect(listSosContracts()).resolves.toHaveLength(1);
    expect(api.get).toHaveBeenCalledWith('/sos/contracts');
  });

  it('looks up a contract by provider and consumer interface ids via query params', async () => {
    vi.mocked(api.get).mockResolvedValue({
      contract_id: 'contract.radar.to.c2',
      contract_name: 'Radar To C2',
      provider_interface_id: 'iface.radar.out',
      consumer_interface_id: 'iface.c2.in',
      sla_metrics: [],
      transformation_rules: {},
      tags: [],
      approved: true,
      signed: true,
      created_at: '2026-04-22T00:00:00Z',
      updated_at: '2026-04-22T00:00:00Z',
    });

    await lookupContractByInterfacePair({
      providerInterfaceId: 'iface.radar.out',
      consumerInterfaceId: 'iface.c2.in',
    });

    expect(api.get).toHaveBeenCalledWith('/sos/contracts/lookup', {
      params: {
        provider_interface_id: 'iface.radar.out',
        consumer_interface_id: 'iface.c2.in',
      },
    });
  });

  it('posts interface compatibility validation using the tagged request shape the coordinator expects', async () => {
    vi.mocked(api.post).mockResolvedValue({
      validation_id: 'validation-1',
      passed: true,
      checks: [],
      confidence: 1,
      validated_at: '2026-04-22T00:00:00Z',
      report_id: 'report-1',
    });

    await validateInterfaceCompatibility({
      providerInterfaceId: 'iface.radar.out',
      consumerInterfaceId: 'iface.c2.in',
    });

    expect(api.post).toHaveBeenCalledWith('/sos/validate', {
      type: 'interface_compatibility',
      provider_interface_id: 'iface.radar.out',
      consumer_interface_id: 'iface.c2.in',
    });
  });

  it('loads the coordinator-generated compatibility matrix', async () => {
    vi.mocked(api.get).mockResolvedValue({
      matrix: [],
      generated_at: '2026-04-22T00:00:00Z',
    });

    await getCompatibilityMatrix();

    expect(api.get).toHaveBeenCalledWith('/sos/compatibility-matrix');
  });

  it('builds canonical interface-pair subject keys for validation history lookups', () => {
    expect(buildInterfacePairSubjectKey('iface.radar.out', 'iface.c2.in')).toBe(
      'interface_pair:iface.radar.out:iface.c2.in'
    );
  });

  it('loads systems using the backend query parameter names', async () => {
    vi.mocked(api.get).mockResolvedValue({
      systems: [],
      total: 0,
      offset: 0,
      limit: 50,
    });

    await listSosSystems({
      systemType: 'mission.broker',
      active: true,
      limit: 10,
    });

    expect(api.get).toHaveBeenCalledWith('/sos/systems', {
      params: {
        system_type: 'mission.broker',
        vendor: undefined,
        classification: undefined,
        tags: undefined,
        active: true,
        offset: undefined,
        limit: 10,
      },
    });
  });

  it('creates systems with empty deployment and capability objects by default', async () => {
    vi.mocked(api.post).mockResolvedValue({
      system_id: 'system-1',
      system_name: 'Mission Broker',
      system_type: 'mission.broker',
      vendor: 'Graphica',
      version: '1.0.0',
      classification: 'UNCLASSIFIED',
      description: null,
      deployment: {},
      capabilities: {},
      tags: [],
      active: true,
      created_at: '2026-04-22T00:00:00Z',
      updated_at: '2026-04-22T00:00:00Z',
    });

    await createSosSystem({
      system_id: 'system-1',
      system_name: 'Mission Broker',
      system_type: 'mission.broker',
      vendor: 'Graphica',
      version: '1.0.0',
      classification: 'UNCLASSIFIED',
    });

    expect(api.post).toHaveBeenCalledWith('/sos/systems', {
      system_id: 'system-1',
      system_name: 'Mission Broker',
      system_type: 'mission.broker',
      vendor: 'Graphica',
      version: '1.0.0',
      classification: 'UNCLASSIFIED',
      deployment: {},
      capabilities: {},
      tags: [],
    });
  });

  it('loads validation report, history, and lineage using the persisted SoS endpoints', async () => {
    vi.mocked(api.get).mockResolvedValueOnce({
      report_id: 'report-1',
    });
    vi.mocked(api.get).mockResolvedValueOnce({
      subject_type: 'interface_pair',
      subject_key: 'interface_pair:iface.radar.out:iface.c2.in',
      reports: [],
    });
    vi.mocked(api.get).mockResolvedValueOnce({
      subject_type: 'interface_pair',
      subject_key: 'interface_pair:iface.radar.out:iface.c2.in',
      reports: [],
      edges: [],
    });

    await getValidationReport('report-1');
    await getValidationHistory({
      subjectKey: 'interface_pair:iface.radar.out:iface.c2.in',
      subjectType: 'interface_pair',
      limit: 25,
    });
    await getValidationLineage({
      subjectKey: 'interface_pair:iface.radar.out:iface.c2.in',
      subjectType: 'interface_pair',
      limit: 25,
    });

    expect(api.get).toHaveBeenNthCalledWith(1, '/sos/validation-reports/report-1');
    expect(api.get).toHaveBeenNthCalledWith(2, '/sos/validation-history', {
      params: {
        subject_key: 'interface_pair:iface.radar.out:iface.c2.in',
        subject_type: 'interface_pair',
        limit: 25,
      },
    });
    expect(api.get).toHaveBeenNthCalledWith(3, '/sos/validation-lineage', {
      params: {
        subject_key: 'interface_pair:iface.radar.out:iface.c2.in',
        subject_type: 'interface_pair',
        limit: 25,
      },
    });
  });

  it('updates, approves, signs, and deletes contracts through the dedicated endpoints', async () => {
    vi.mocked(api.put).mockResolvedValue({
      contract_id: 'contract-1',
    });
    vi.mocked(api.post).mockResolvedValue({ contract_id: 'contract-1' });
    vi.mocked(api.delete).mockResolvedValue(undefined);

    await updateSosContract({
      id: 'contract-1',
      request: { contract_name: 'Updated Contract' },
    });
    await approveSosContract('contract-1');
    await signSosContract('contract-1');
    await deleteSosContract('contract-1');

    expect(api.put).toHaveBeenCalledWith('/sos/contracts/contract-1', {
      contract_name: 'Updated Contract',
    });
    expect(api.post).toHaveBeenNthCalledWith(1, '/sos/contracts/contract-1/approve');
    expect(api.post).toHaveBeenNthCalledWith(2, '/sos/contracts/contract-1/sign');
    expect(api.delete).toHaveBeenCalledWith('/sos/contracts/contract-1');
  });

  it('lists and creates policies with backend-normalized defaults', async () => {
    vi.mocked(api.get).mockResolvedValue({
      policies: [],
      total: 0,
      offset: 0,
      limit: 50,
    });
    vi.mocked(api.post).mockResolvedValue({
      policy_id: 'policy-1',
    });

    await listSosPolicies({
      targetType: 'interface_pair',
      stage: 'pre_execution',
      active: true,
      limit: 20,
    });
    await createSosPolicy({
      policy_id: 'policy-1',
      policy_name: 'Pair Must Stay Json',
      target_type: 'interface_pair',
      sparql_query: 'ASK { ?s ?p ?o }',
      provider_interface_id: 'iface.provider',
      consumer_interface_id: 'iface.consumer',
    });

    expect(api.get).toHaveBeenCalledWith('/sos/policies', {
      params: {
        target_type: 'interface_pair',
        stage: 'pre_execution',
        active: true,
        offset: undefined,
        limit: 20,
      },
    });
    expect(api.post).toHaveBeenCalledWith('/sos/policies', {
      policy_id: 'policy-1',
      policy_name: 'Pair Must Stay Json',
      target_type: 'interface_pair',
      sparql_query: 'ASK { ?s ?p ?o }',
      provider_interface_id: 'iface.provider',
      consumer_interface_id: 'iface.consumer',
      stages: ['pre_execution'],
      enforcement_level: 'mandatory',
      severity: 'medium',
      context: {},
      tags: [],
      ontology_refs: [],
      shape_refs: [],
      active: true,
    });
  });

  it('updates, validates, dry-runs, and deletes policies through the governance endpoints', async () => {
    vi.mocked(api.put).mockResolvedValue({
      policy_id: 'policy-1',
    });
    vi.mocked(api.post)
      .mockResolvedValueOnce({
        validation_id: 'validation-1',
      })
      .mockResolvedValueOnce({
        validation_id: 'validation-2',
      });
    vi.mocked(api.delete).mockResolvedValue(undefined);

    await updateSosPolicy({
      id: 'policy-1',
      request: {
        policy_name: 'Updated Policy',
        severity: 'high',
      },
    });
    await validateSosPolicy({
      id: 'policy-1',
      request: {
        stage: 'post_execution',
        context: { threshold: 0.95 },
      },
    });
    await validateSosPolicyDryRun({
      id: 'policy-1',
      request: {
        context: { threshold: 0.8 },
      },
    });
    await deleteSosPolicy('policy-1');

    expect(api.put).toHaveBeenCalledWith('/sos/policies/policy-1', {
      policy_name: 'Updated Policy',
      severity: 'high',
    });
    expect(api.post).toHaveBeenNthCalledWith(1, '/sos/policies/policy-1/validate', {
      stage: 'post_execution',
      context: { threshold: 0.95 },
    });
    expect(api.post).toHaveBeenNthCalledWith(2, '/sos/policies/policy-1/validate/dry-run', {
      stage: undefined,
      context: { threshold: 0.8 },
    });
    expect(api.delete).toHaveBeenCalledWith('/sos/policies/policy-1');
  });

  it('posts interface payload schema validation to the interface-specific endpoint', async () => {
    vi.mocked(api.post).mockResolvedValue({
      validation_id: 'validation-schema',
    });

    await validateSosInterfaceSchema({
      interfaceId: 'iface.provider',
      data: {
        track_id: 'abc-123',
        latitude: 42.1,
      },
    });

    expect(api.post).toHaveBeenCalledWith('/sos/interfaces/iface.provider/validate-schema', {
      track_id: 'abc-123',
      latitude: 42.1,
    });
  });

  it('loads dependency graph and submits what-if analysis payloads', async () => {
    vi.mocked(api.get).mockResolvedValue({
      generated_at: '2026-04-22T00:00:00Z',
      nodes: [],
      edges: [],
    });
    vi.mocked(api.post).mockResolvedValue({
      scenario_id: 'scenario-1',
      impact: [],
      affected_entities: [],
      recommendations: [],
    });

    await getSosDependencyGraph();
    await runSosWhatIfAnalysis({
      scenario: 'Remove broker',
      changes: [
        {
          entity_type: 'system',
          operation: 'delete',
          system_id: 'sys.broker',
        },
      ],
    });

    expect(api.get).toHaveBeenCalledWith('/sos/dependency-graph');
    expect(api.post).toHaveBeenCalledWith('/sos/what-if', {
      scenario: 'Remove broker',
      changes: [
        {
          entity_type: 'system',
          operation: 'delete',
          system_id: 'sys.broker',
        },
      ],
    });
  });

  it('exposes SoS operator controls and governance audit endpoints without client-side scans', async () => {
    vi.mocked(api.get).mockResolvedValue({});
    vi.mocked(api.post).mockResolvedValue({});

    await listSosContractApprovalRequests({
      contractId: 'contract-1',
      status: 'pending',
      offset: 5,
      limit: 10,
    });
    await listSosContractSignatures('contract-1', 10);
    await getSosContractSigningKeyStatus();
    await listSosPolicyApprovalRequests({
      policyId: 'policy-1',
      status: 'approved',
      offset: 0,
      limit: 15,
    });
    await listSosPolicyAttestations('policy-1', 25);
    await getSosPolicySigningKeyStatus();
    await rotateSosContractSigningKey({ reason: 'Quarterly rotation' });
    await rotateSosPolicySigningKey({
      reason: 'Custody update',
      trust_mode: 'external_reference',
      trust_provider: 'aws-kms',
      external_key_ref: 'arn:aws:kms:us-east-1:123456789:key/example',
      trust_attestation_ref: 'attestation://policy-key/2026-04',
    });
    await reconcileSosRuntime({ include_ontology_sync: false });

    expect(api.get).toHaveBeenNthCalledWith(1, '/sos/contracts/contract-1/approval-requests', {
      params: {
        status: 'pending',
        offset: 5,
        limit: 10,
      },
    });
    expect(api.get).toHaveBeenNthCalledWith(2, '/sos/contracts/contract-1/signatures', {
      params: {
        limit: 10,
      },
    });
    expect(api.get).toHaveBeenNthCalledWith(3, '/sos/contracts/signing-key');
    expect(api.get).toHaveBeenNthCalledWith(4, '/sos/policies/policy-1/approval-requests', {
      params: {
        status: 'approved',
        offset: 0,
        limit: 15,
      },
    });
    expect(api.get).toHaveBeenNthCalledWith(5, '/sos/policies/policy-1/attestations', {
      params: {
        limit: 25,
      },
    });
    expect(api.get).toHaveBeenNthCalledWith(6, '/sos/policies/signing-key');
    expect(api.post).toHaveBeenNthCalledWith(1, '/sos/contracts/signing-key/rotate', {
      reason: 'Quarterly rotation',
    });
    expect(api.post).toHaveBeenNthCalledWith(2, '/sos/policies/signing-key/rotate', {
      reason: 'Custody update',
      trust_mode: 'external_reference',
      trust_provider: 'aws-kms',
      external_key_ref: 'arn:aws:kms:us-east-1:123456789:key/example',
      trust_attestation_ref: 'attestation://policy-key/2026-04',
    });
    expect(api.post).toHaveBeenNthCalledWith(3, '/sos/reconcile', {
      include_ontology_sync: false,
    });
  });
});
