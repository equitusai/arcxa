import React from 'react';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SosOperationsPanel } from './SosOperationsPanel';
import { useAuthStore } from '@/stores/auth';

const mockReconcile = vi.fn();
const mockRotateContractKey = vi.fn();
const mockRotatePolicyKey = vi.fn();

vi.mock('@/hooks/useSosValidation', () => ({
  useSosContracts: () => ({
    data: [
      {
        contract_id: 'contract-1',
        revision: 3,
        contract_name: 'Contract 1',
        provider_interface_id: 'iface.provider',
        consumer_interface_id: 'iface.consumer',
        sla_metrics: [],
        transformation_rules: {},
        tags: [],
        approved: true,
        signed: true,
        lifecycle_state: 'signed',
        approval_status: 'approved',
        approved_by: 'reviewer-1',
        signed_by: 'signer-1',
        signature: {
          signature_id: 'signature-1',
          contract_id: 'contract-1',
          contract_revision: 3,
          contract_revision_ref: 'contract:contract-1@3',
          payload_hash: 'sha256:abc',
          payload_hash_algorithm: 'sha256',
          signature_algorithm: 'ed25519',
          signature: 'sig',
          public_key: 'pub',
          key_fingerprint: 'sha256:contract-key',
          signing_key_source: 'secret_store',
          signed_by: 'signer-1',
          signed_at: '2026-04-23T00:00:00Z',
          evidence_ids: ['evidence-1'],
          policy_refs: ['policy:gate'],
          signature_verified: true,
          metadata: {},
        },
        created_at: '2026-04-22T00:00:00Z',
        updated_at: '2026-04-22T00:00:00Z',
      },
      {
        contract_id: 'contract-2',
        revision: 4,
        contract_name: 'Contract 2',
        provider_interface_id: 'iface.provider.2',
        consumer_interface_id: 'iface.consumer.2',
        sla_metrics: [],
        transformation_rules: {},
        tags: [],
        approved: false,
        signed: false,
        lifecycle_state: 'draft',
        approval_status: 'pending',
        approval_requested_by: 'operator-queue',
        approval_requested_at: '2026-04-24T15:30:00Z',
        created_at: '2026-04-24T14:00:00Z',
        updated_at: '2026-04-24T15:30:00Z',
      },
    ],
    isLoading: false,
    error: null,
  }),
  useSosPolicies: () => ({
    data: {
      policies: [
        {
          policy_id: 'policy-1',
          revision: 2,
          policy_name: 'Policy 1',
          target_type: 'interface_pair',
          stages: ['contract_approval'],
          enforcement_level: 'mandatory',
          severity: 'high',
          sparql_query: 'ASK { ?s ?p ?o }',
          context: {},
          tags: [],
          ontology_refs: [],
          shape_refs: [],
          active: true,
          lifecycle_state: 'active',
          approval_status: 'approved',
          approval_requested_by: 'operator-1',
          approved_by: 'reviewer-2',
          attestation: {
            attestation_id: 'attestation-1',
            policy_id: 'policy-1',
            policy_revision: 2,
            policy_revision_ref: 'policy:policy-1@2',
            payload_hash: 'sha256:abc',
            payload_hash_algorithm: 'sha256',
            signature_algorithm: 'ed25519',
            signature: 'sig',
            public_key: 'pub',
            key_fingerprint: 'sha256:key',
            signing_key_source: 'secret_store',
            trust_mode: 'software',
            attested_by: 'reviewer-2',
            attested_at: '2026-04-22T00:00:00Z',
            evidence_ids: ['evidence-1'],
            policy_refs: ['policy:policy-1'],
            attestation_verified: true,
            metadata: {},
          },
          created_at: '2026-04-22T00:00:00Z',
          updated_at: '2026-04-22T00:00:00Z',
        },
        {
          policy_id: 'policy-2',
          revision: 5,
          policy_name: 'Policy 2',
          target_type: 'contract',
          stages: ['contract_signing'],
          enforcement_level: 'mandatory',
          severity: 'medium',
          sparql_query: 'ASK { ?contract ?p ?o }',
          context: {},
          tags: [],
          ontology_refs: [],
          shape_refs: [],
          active: true,
          lifecycle_state: 'dry_run',
          approval_status: 'pending',
          approval_requested_by: 'operator-rollout',
          approval_requested_at: '2026-04-25T09:00:00Z',
          created_at: '2026-04-24T00:00:00Z',
          updated_at: '2026-04-25T09:00:00Z',
        },
      ],
      total: 2,
      offset: 0,
      limit: 50,
    },
    isLoading: false,
    error: null,
  }),
  useSosContractApprovalRequests: () => ({
    data: {
      requests: [
        {
          request_id: 'contract-request-1',
          contract_id: 'contract-1',
          contract_revision: 3,
          approval_type: 'manual',
          requested_lifecycle_state: 'approved',
          status: 'approved',
          note: 'Ready for rollout',
          requested_by: 'operator-1',
          requested_at: '2026-04-22T00:00:00Z',
          metadata: {},
          evidence: [],
        },
      ],
      total: 1,
      offset: 0,
      limit: 10,
    },
    isLoading: false,
  }),
  useSosContractSignatures: () => ({
    data: {
      signatures: [
        {
          signature_id: 'signature-1',
          contract_id: 'contract-1',
          contract_revision: 3,
          contract_revision_ref: 'contract:contract-1@3',
          payload_hash: 'sha256:abc',
          payload_hash_algorithm: 'sha256',
          signature_algorithm: 'ed25519',
          signature: 'sig',
          public_key: 'pub',
          key_fingerprint: 'sha256:key',
          signing_key_source: 'secret_store',
          signed_by: 'signer-1',
          signed_at: '2026-04-22T00:00:00Z',
          evidence_ids: ['evidence-1'],
          policy_refs: ['policy:gate'],
          signature_verified: true,
          metadata: {},
        },
      ],
      total: 1,
      limit: 10,
    },
    isLoading: false,
  }),
  useSosContractSigningKeyStatus: () => ({
    data: {
      signing_key_ref: 'sos/contracts/signing-key',
      signing_key_source: 'secret_store',
      signing_key_version: 'v3',
      key_fingerprint: 'sha256:contract-key',
      public_key: 'pub',
      supports_rotation: true,
      rotation_next_due_at: '2026-04-20T00:00:00Z',
      tags: [],
      metadata: {},
    },
    isLoading: false,
  }),
  useSosPolicyApprovalRequests: () => ({
    data: {
      requests: [
        {
          request_id: 'policy-request-1',
          policy_id: 'policy-1',
          policy_revision: 2,
          policy_revision_ref: 'policy:policy-1@2',
          approval_type: 'manual',
          requested_lifecycle_state: 'active',
          status: 'approved',
          note: 'Looks good',
          requested_by: 'operator-1',
          requested_at: '2026-04-22T00:00:00Z',
          metadata: {},
          evidence: [],
        },
      ],
      total: 1,
      offset: 0,
      limit: 10,
    },
    isLoading: false,
  }),
  useSosPolicyAttestations: () => ({
    data: {
      attestations: [
        {
          attestation_id: 'attestation-1',
          policy_id: 'policy-1',
          policy_revision: 2,
          policy_revision_ref: 'policy:policy-1@2',
          payload_hash: 'sha256:abc',
          payload_hash_algorithm: 'sha256',
          signature_algorithm: 'ed25519',
          signature: 'sig',
          public_key: 'pub',
          key_fingerprint: 'sha256:policy-key',
          signing_key_source: 'secret_store',
          trust_mode: 'software',
          attested_by: 'reviewer-2',
          attested_at: '2026-04-22T00:00:00Z',
          evidence_ids: ['evidence-1'],
          policy_refs: ['policy:policy-1'],
          attestation_verified: true,
          metadata: {},
        },
      ],
      total: 1,
      limit: 10,
    },
    isLoading: false,
  }),
  useSosPolicySigningKeyStatus: () => ({
    data: {
      signing_key_ref: 'sos/policies/signing-key',
      signing_key_source: 'secret_store',
      signing_key_version: 'v4',
      key_fingerprint: 'sha256:policy-key',
      public_key: 'pub',
      supports_rotation: true,
      trust_mode: 'software',
      rotation_next_due_at: '2026-05-01T00:00:00Z',
      tags: [],
      metadata: {},
    },
    isLoading: false,
  }),
  useReconcileSosRuntime: () => ({
    mutateAsync: mockReconcile,
    isPending: false,
  }),
  useRotateSosContractSigningKey: () => ({
    mutateAsync: mockRotateContractKey,
    isPending: false,
  }),
  useRotateSosPolicySigningKey: () => ({
    mutateAsync: mockRotatePolicyKey,
    isPending: false,
  }),
}));

describe('SosOperationsPanel', () => {
  beforeEach(() => {
    mockReconcile.mockReset();
    mockRotateContractKey.mockReset();
    mockRotatePolicyKey.mockReset();

    mockReconcile.mockResolvedValue({
      triggered_by: 'admin',
      include_ontology_sync: true,
      ontology_registry_available: false,
      ontology_sync_performed: false,
      graph_reconcile_performed: true,
      system_count: 4,
      interface_count: 8,
      contract_count: 2,
      policy_count: 3,
      started_at: '2026-04-22T00:00:00Z',
      completed_at: '2026-04-22T00:00:01Z',
      duration_ms: 1000,
    });
    mockRotateContractKey.mockResolvedValue({
      current_signing_key_version: 'v4',
    });
    mockRotatePolicyKey.mockResolvedValue({
      current_signing_key_version: 'v5',
    });

    useAuthStore.getState().setAuth('token', {
      id: 'admin-1',
      username: 'admin',
      role: 'Admin',
      created_at: '2026-04-22T00:00:00Z',
    });
  });

  it('exposes reconcile, signing-key rotation, and governance audit views in one operator workspace', async () => {
    render(<SosOperationsPanel />);

    expect(screen.getByText('Recovery Controls')).toBeTruthy();
    expect(screen.getByText('Governance Overview')).toBeTruthy();
    expect(screen.getByText('Pending Approval Queue')).toBeTruthy();
    expect(screen.getByText('Recent Trust Feed')).toBeTruthy();
    expect(screen.getByText('Contract Governance Audit')).toBeTruthy();
    expect(screen.getByText('Policy Governance Audit')).toBeTruthy();
    expect(screen.getByText('contract-request-1')).toBeTruthy();
    expect(screen.getByText('policy-request-1')).toBeTruthy();
    expect(screen.getByText('Contract 2')).toBeTruthy();
    expect(screen.getByText('Policy 2')).toBeTruthy();
    expect(screen.getAllByText('signer-1').length).toBeGreaterThan(0);
    expect(screen.getAllByText('reviewer-2').length).toBeGreaterThan(0);
    expect(screen.getByText('Contracts')).toBeTruthy();
    expect(screen.getByText('Policies')).toBeTruthy();
    expect(screen.getByText('Keys Due')).toBeTruthy();
    expect(screen.getByText('Protected Revisions')).toBeTruthy();
    expect(screen.getByText('Needs Attention')).toBeTruthy();
    expect(screen.getByText('operator-queue')).toBeTruthy();
    expect(screen.getByText('operator-rollout')).toBeTruthy();
    expect(screen.getAllByText('contract:contract-1@3').length).toBeGreaterThan(0);
    expect(screen.getAllByText('policy:policy-1@2').length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole('button', { name: 'Run Reconcile' }));

    await waitFor(() => {
      expect(mockReconcile).toHaveBeenCalledWith({ include_ontology_sync: true });
    });

    const rotateButtons = screen.getAllByRole('button', { name: 'Rotate Key' });
    const rotationInputs = screen.getAllByLabelText('Rotation Note');

    fireEvent.change(rotationInputs[0], { target: { value: 'Quarterly contract rotation' } });
    fireEvent.click(rotateButtons[0]);

    await waitFor(() => {
      expect(mockRotateContractKey).toHaveBeenCalledWith({
        reason: 'Quarterly contract rotation',
      });
    });

    fireEvent.change(rotationInputs[1], { target: { value: 'Policy custody update' } });
    fireEvent.change(
      screen.getByPlaceholderText('Trust mode (for example software or external_reference)'),
      {
        target: { value: 'external_reference' },
      }
    );
    fireEvent.change(screen.getByPlaceholderText('Trust provider'), {
      target: { value: 'aws-kms' },
    });
    fireEvent.change(screen.getByPlaceholderText('External key ref'), {
      target: { value: 'arn:aws:kms:us-east-1:123:key/example' },
    });
    fireEvent.change(screen.getByPlaceholderText('External trust attestation ref'), {
      target: { value: 'attestation://policy/2026-04' },
    });
    fireEvent.click(rotateButtons[1]);

    await waitFor(() => {
      expect(mockRotatePolicyKey).toHaveBeenCalledWith({
        reason: 'Policy custody update',
        trust_mode: 'external_reference',
        trust_provider: 'aws-kms',
        external_key_ref: 'arn:aws:kms:us-east-1:123:key/example',
        trust_attestation_ref: 'attestation://policy/2026-04',
      });
    });

    const signaturesSection = screen
      .getAllByText('Signatures')[0]
      .closest('div.rounded-sm.border');
    expect(signaturesSection).toBeTruthy();
    expect(within(signaturesSection as HTMLElement).getByText('Verified')).toBeTruthy();
  });
});
