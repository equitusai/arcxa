import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { MigrationEvidence } from './MigrationEvidence';

const mockExplain = vi.fn();
const mockPacket = vi.fn();
const mockControls = vi.fn();
const mockExceptions = vi.fn();
const mockApprovals = vi.fn();
const mockUpsertConnector = vi.fn();
const mockRunConnector = vi.fn();
const mockRefetchRuntimeStatus = vi.fn();
const mockRebuildReadModels = vi.fn();

vi.mock('@/hooks/useMigrationEvidence', () => ({
  useExplainMigrationValue: () => ({
    mutateAsync: mockExplain,
    isPending: false,
  }),
  useLookupMigrationEvidencePacket: () => ({
    mutateAsync: mockPacket,
    isPending: false,
  }),
  useLookupMigrationObjectControls: () => ({
    mutateAsync: mockControls,
    isPending: false,
  }),
  useLookupMigrationProgramExceptions: () => ({
    mutateAsync: mockExceptions,
    isPending: false,
  }),
  useLookupMigrationProgramApprovals: () => ({
    mutateAsync: mockApprovals,
    isPending: false,
  }),
  useUpsertMigrationConnector: () => ({
    mutateAsync: mockUpsertConnector,
    isPending: false,
  }),
  useRunMigrationConnector: () => ({
    mutateAsync: mockRunConnector,
    isPending: false,
  }),
  useMigrationRuntimeStatus: () => ({
    data: {
      status: {
        backend: 'rocks_db',
        replay_supported: true,
        event_log_available: true,
        read_models: {
          programs: 1,
          objects: 1,
          rules: 1,
          executions: 1,
          exceptions: 0,
          controls: 0,
          approvals: 0,
          packets: 0,
          object_indexes: 1,
          program_object_links: 1,
          event_log_entries: 6,
        },
        event_bus: {
          mode: 'direct',
          async_delivery_enabled: false,
          consumer_state: 'disabled',
          processed_message_count: 0,
          malformed_message_count: 0,
          retry_attempt_count: 0,
          lag_state: 'unknown',
          estimated_lag_message_count: null,
          broker_reachability: 'reachable',
          discovered_broker_count: 3,
          assigned_partitions: [0, 1],
          topic_partition_count: 2,
          partition_lag: [
            { partition: 0, current_offset: 10, high_watermark: 10, estimated_lag_message_count: 0 },
            { partition: 1, current_offset: 8, high_watermark: 8, estimated_lag_message_count: 0 },
          ],
          last_retry_at: null,
          lag_observed_at: null,
          last_state_changed_at: '2026-04-30T00:00:00Z',
          startup_completed_at: null,
          startup_failure_reason: null,
          last_assignment_at: '2026-04-30T00:00:00Z',
          last_broker_probe_at: '2026-04-30T00:00:00Z',
          lag_diagnostics: 'consumer is caught up across 2 assigned partition(s)',
        },
        last_event_sequence: 6,
        updated_at: '2026-04-30T00:00:00Z',
        last_rebuild_at: null,
        legacy_imported_at: null,
      },
      ingestion_status: {
        connector_store: {
          backend: 'rocks_db',
          health: 'healthy',
          connector_count: 2,
          writable: true,
          updated_at: '2026-04-30T00:00:00Z',
          last_successful_write_at: '2026-04-30T00:00:00Z',
          legacy_imported_at: null,
          last_error: null,
        },
        delivery_mode: 'direct',
        verification_service_configured: true,
        updated_at: '2026-04-30T00:00:00Z',
      },
    },
    isFetching: false,
    refetch: mockRefetchRuntimeStatus,
  }),
  useRebuildMigrationReadModels: () => ({
    mutateAsync: mockRebuildReadModels,
    isPending: false,
  }),
}));

function renderPage(initialEntry = '/migration-evidence') {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <Routes>
        <Route path="/migration-evidence" element={<MigrationEvidence />} />
      </Routes>
    </MemoryRouter>
  );
}

describe('MigrationEvidence', () => {
  beforeEach(() => {
    mockExplain.mockReset();
    mockPacket.mockReset();
    mockControls.mockReset();
    mockExceptions.mockReset();
    mockApprovals.mockReset();
    mockUpsertConnector.mockReset();
    mockRunConnector.mockReset();
    mockRefetchRuntimeStatus.mockReset();
    mockRebuildReadModels.mockReset();

    mockExplain.mockResolvedValue({
      explanation: {
        explanation_id: 'exp-1',
        locator: {
          program_id: 'program-rise-1',
          object_id: 'object-sales-order',
          target_field_path: '$.amount',
          target_record_id: 'SO-1',
          source_record_id: 'SO-1',
        },
        source_field: {
          system: 'SAP ECC',
          object_name: 'VBAK',
          field_name: 'NETWR',
          field_path: '$.amount',
          record_id: 'SO-1',
        },
        target_field: {
          system: 'SAP S/4HANA',
          object_name: 'A_SalesOrder',
          field_name: 'NetAmount',
          field_path: '$.amount',
          record_id: 'SO-1',
        },
        source_value: 100,
        target_value: 103,
        transformation_rule: {
          rule_id: 'rule-net-amount',
          rule_type: 'mapping',
          name: 'Normalize net amount',
          source_fields: [],
          target_fields: [],
          metadata: {},
        },
        execution_event: {
          execution_id: 'exec-1',
          program_id: 'program-rise-1',
          object_id: 'object-sales-order',
          connector_run_id: 'run-1',
          tool_name: 'ibm_rapid_move',
          tool_run_id: 'rm-run-1',
          stage: 'load',
          status: 'succeeded',
          happened_at: '2026-04-30T00:00:00Z',
          metadata: {},
        },
        exceptions: [],
        controls: [],
        approvals: [],
        evidence_packet_id: 'packet-1',
        graph_refs: [],
        confidence_summary: 'Traceable through IBM Rapid Move artifacts.',
        generated_at: '2026-04-30T00:00:00Z',
      },
    });

    mockPacket.mockResolvedValue({ packet: { packet_id: 'packet-1', signature: null, metadata: {} } });
    mockControls.mockResolvedValue({ controls: [] });
    mockExceptions.mockResolvedValue({ exceptions: [] });
    mockApprovals.mockResolvedValue({ approvals: [] });
    mockUpsertConnector.mockResolvedValue({ connector: { connector_id: 'ibm-artifacts', name: 'IBM Rapid Move Artifact Ingestion' } });
    mockRunConnector.mockResolvedValue({
      summary: {
        connector_id: 'ibm-artifacts',
        ingested_event_count: 6,
        delivery_mode: 'direct',
        traceability_acknowledged: true,
      },
    });
    mockRefetchRuntimeStatus.mockResolvedValue(undefined);
    mockRebuildReadModels.mockResolvedValue({ summary: { replayed_event_count: 6 } });
  });

  it('hydrates the explain form from URL state', () => {
    renderPage('/migration-evidence?tab=explain&program=program-rise-1&object=object-sales-order&field=$.amount&targetRecord=SO-1&sourceRecord=SO-1');

    expect((screen.getByLabelText('Program ID') as HTMLInputElement).value).toBe('program-rise-1');
    expect((screen.getByLabelText('Object ID') as HTMLInputElement).value).toBe('object-sales-order');
    expect((screen.getByLabelText('Target Field Path') as HTMLInputElement).value).toBe('$.amount');
    expect(screen.getByRole('button', { name: 'Explain Value' })).toBeTruthy();
  });

  it('explains a value and derives the packet lookup key for the audit bundle', async () => {
    renderPage('/migration-evidence');

    fireEvent.click(screen.getByRole('button', { name: 'Explain Value' }));

    await waitFor(() => {
      expect(mockExplain).toHaveBeenCalledWith({
        programId: 'program-rise-1',
        objectId: 'object-sales-order',
        targetFieldPath: '$.amount',
        targetRecordId: 'SO-1',
        sourceRecordId: 'SO-1',
      });
    });
    expect(screen.getByText('Explanation Summary')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Load Audit Bundle' }));

    await waitFor(() => {
      expect(mockPacket).toHaveBeenCalledWith({
        objectId: 'object-sales-order',
        valueKey: 'SO-1::$.amount',
      });
    });
    expect(mockControls).toHaveBeenCalledWith('object-sales-order');
    expect(mockExceptions).toHaveBeenCalledWith('program-rise-1');
    expect(mockApprovals).toHaveBeenCalledWith('program-rise-1');
    expect(screen.getByText('Signed Evidence Packet')).toBeTruthy();
  });

  it('shows runtime status and lets operators refresh or rebuild read models', async () => {
    renderPage('/migration-evidence?tab=connectors');

    expect(screen.getAllByText('RocksDB').length).toBeGreaterThan(0);
    expect(screen.getByText('supported')).toBeTruthy();
    expect(screen.getByText('Direct')).toBeTruthy();
    expect(screen.getByText('disabled')).toBeTruthy();
    expect(screen.getByText('reachable')).toBeTruthy();
    expect(screen.getByText('0, 1')).toBeTruthy();
    expect(screen.getByText('healthy')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Refresh Status' }));
    await waitFor(() => {
      expect(mockRefetchRuntimeStatus).toHaveBeenCalled();
    });

    fireEvent.click(screen.getByRole('button', { name: 'Rebuild Read Models' }));
    await waitFor(() => {
      expect(mockRebuildReadModels).toHaveBeenCalled();
    });
  });
});
