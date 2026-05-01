import { useMutation, useQuery } from '@tanstack/react-query';
import { toast } from 'sonner';

import * as migrationEvidenceApi from '@/api/migrationEvidence';

function getErrorMessage(error: unknown): string {
  const apiError = error as {
    message?: string;
    response?: {
      data?: {
        message?: string;
        error?: string;
      };
    };
  };

  return (
    apiError.response?.data?.message ||
    apiError.response?.data?.error ||
    apiError.message ||
    'Request failed'
  );
}

export function useUpsertMigrationConnector() {
  return useMutation({
    mutationFn: migrationEvidenceApi.upsertMigrationConnector,
    onSuccess: (response) => {
      toast.success('Connector saved', {
        description: `${response.connector.name} is ready for evidence ingestion.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to save connector', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useRunMigrationConnector() {
  return useMutation({
    mutationFn: migrationEvidenceApi.runMigrationConnector,
    onSuccess: (response) => {
      toast.success('Connector run started', {
        description: `${response.summary.connector_id} captured ${response.summary.ingested_event_count} artifact(s).`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to run connector', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useExplainMigrationValue() {
  return useMutation({
    mutationFn: migrationEvidenceApi.explainMigrationValue,
    onError: (error: unknown) => {
      toast.error('Failed to explain migrated value', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupMigrationEvidencePacket() {
  return useMutation({
    mutationFn: migrationEvidenceApi.getMigrationEvidencePacket,
    onError: (error: unknown) => {
      toast.error('Failed to load evidence packet', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupMigrationObjectControls() {
  return useMutation({
    mutationFn: migrationEvidenceApi.getMigrationObjectControls,
    onError: (error: unknown) => {
      toast.error('Failed to load controls', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupMigrationProgramExceptions() {
  return useMutation({
    mutationFn: migrationEvidenceApi.getMigrationProgramExceptions,
    onError: (error: unknown) => {
      toast.error('Failed to load exceptions', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupMigrationProgramApprovals() {
  return useMutation({
    mutationFn: migrationEvidenceApi.getMigrationProgramApprovals,
    onError: (error: unknown) => {
      toast.error('Failed to load approvals', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useMigrationRuntimeStatus() {
  return useQuery({
    queryKey: ['migration-evidence', 'runtime-status'],
    queryFn: migrationEvidenceApi.getMigrationRuntimeStatus,
    staleTime: 30 * 1000,
  });
}

export function useRebuildMigrationReadModels() {
  return useMutation({
    mutationFn: migrationEvidenceApi.rebuildMigrationReadModels,
    onSuccess: (response) => {
      toast.success('Read models rebuilt', {
        description: `${response.summary.replayed_event_count} event(s) replayed into the migration evidence graph.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to rebuild read models', {
        description: getErrorMessage(error),
      });
    },
  });
}
