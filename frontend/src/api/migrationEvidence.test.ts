import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./client', () => ({
  api: {
    get: vi.fn(),
    post: vi.fn(),
  },
}));

import { api } from './client';
import {
  explainMigrationValue,
  getMigrationEvidencePacket,
  getMigrationObjectControls,
  getMigrationProgramApprovals,
  getMigrationProgramExceptions,
  getMigrationRuntimeStatus,
  rebuildMigrationReadModels,
  runMigrationConnector,
  upsertMigrationConnector,
} from './migrationEvidence';

describe('migrationEvidence api helpers', () => {
  beforeEach(() => {
    vi.mocked(api.get).mockReset();
    vi.mocked(api.post).mockReset();
  });

  it('posts connector payloads directly to the migration-evidence connector endpoint', async () => {
    vi.mocked(api.post).mockResolvedValue({ connector: { connector_id: 'ibm-artifacts' } });

    await upsertMigrationConnector({ connector_id: 'ibm-artifacts' });

    expect(api.post).toHaveBeenCalledWith('/migration-evidence/connectors', {
      connector_id: 'ibm-artifacts',
    });
  });

  it('posts connector runs against the connector-specific endpoint', async () => {
    vi.mocked(api.post).mockResolvedValue({ summary: { connector_id: 'ibm-artifacts' } });

    await runMigrationConnector({
      connectorId: 'ibm-artifacts',
      request: { run_label: 'wave-1' },
    });

    expect(api.post).toHaveBeenCalledWith(
      '/migration-evidence/connectors/ibm-artifacts/runs',
      { run_label: 'wave-1' }
    );
  });

  it('requests explain-value using the coordinator query parameter names', async () => {
    vi.mocked(api.get).mockResolvedValue({ explanation: { explanation_id: 'exp-1' } });

    await explainMigrationValue({
      programId: 'program-rise-1',
      objectId: 'object-sales-order',
      targetFieldPath: '$.amount',
      targetRecordId: 'SO-1',
      sourceRecordId: 'SO-1',
    });

    expect(api.get).toHaveBeenCalledWith('/migration-evidence/values/explain', {
      params: {
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        target_field_path: '$.amount',
        target_record_id: 'SO-1',
        source_record_id: 'SO-1',
      },
    });
  });

  it('loads packet, controls, exceptions, and approvals from the dedicated endpoints', async () => {
    vi.mocked(api.get).mockResolvedValue({});

    await getMigrationEvidencePacket({
      objectId: 'object-sales-order',
      valueKey: 'SO-1::$.amount',
    });
    await getMigrationObjectControls('object-sales-order');
    await getMigrationProgramExceptions('program-rise-1');
    await getMigrationProgramApprovals('program-rise-1');

    expect(api.get).toHaveBeenNthCalledWith(
      1,
      '/migration-evidence/objects/object-sales-order/evidence-packet',
      {
        params: { value_key: 'SO-1::$.amount' },
      }
    );
    expect(api.get).toHaveBeenNthCalledWith(
      2,
      '/migration-evidence/objects/object-sales-order/controls'
    );
    expect(api.get).toHaveBeenNthCalledWith(
      3,
      '/migration-evidence/programs/program-rise-1/exceptions'
    );
    expect(api.get).toHaveBeenNthCalledWith(
      4,
      '/migration-evidence/programs/program-rise-1/approvals'
    );
  });

  it('loads runtime status and posts rebuild requests through the dedicated runtime endpoints', async () => {
    vi.mocked(api.get).mockResolvedValue({ status: { backend: 'rocks_db' } });
    vi.mocked(api.post).mockResolvedValue({ summary: { replayed_event_count: 6 } });

    await getMigrationRuntimeStatus();
    await rebuildMigrationReadModels();

    expect(api.get).toHaveBeenCalledWith('/migration-evidence/runtime/status');
    expect(api.post).toHaveBeenCalledWith('/migration-evidence/runtime/rebuild', {});
  });
});
