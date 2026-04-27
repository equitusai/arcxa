import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';

import * as sosValidationApi from '@/api/sosValidation';

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

export function useSosInterfaces() {
  return useQuery({
    queryKey: ['sos', 'interfaces'],
    queryFn: sosValidationApi.listSosInterfaces,
    staleTime: 60 * 1000,
  });
}

export function useSosContracts() {
  return useQuery({
    queryKey: ['sos', 'contracts'],
    queryFn: sosValidationApi.listSosContracts,
    staleTime: 60 * 1000,
  });
}

export function useSosSystems() {
  return useQuery({
    queryKey: ['sos', 'systems'],
    queryFn: () => sosValidationApi.listSosSystems(),
    staleTime: 60 * 1000,
  });
}

export function useSosPolicies() {
  return useQuery({
    queryKey: ['sos', 'policies'],
    queryFn: () => sosValidationApi.listSosPolicies(),
    staleTime: 60 * 1000,
  });
}

export function useSosCompatibilityMatrix() {
  return useQuery({
    queryKey: ['sos', 'compatibility-matrix'],
    queryFn: sosValidationApi.getCompatibilityMatrix,
    staleTime: 30 * 1000,
  });
}

function invalidateSosCatalogQueries(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: ['sos', 'systems'] });
  queryClient.invalidateQueries({ queryKey: ['sos', 'interfaces'] });
  queryClient.invalidateQueries({ queryKey: ['sos', 'contracts'] });
  queryClient.invalidateQueries({ queryKey: ['sos', 'compatibility-matrix'] });
}

function invalidateSosPolicyQueries(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: ['sos', 'policies'] });
}

function invalidateAllSosQueries(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({ queryKey: ['sos'] });
}

export function useLookupSosContract() {
  return useMutation({
    mutationFn: sosValidationApi.lookupContractByInterfacePair,
    onError: (error: unknown) => {
      toast.error('Contract lookup failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useValidateInterfacePair() {
  return useMutation({
    mutationFn: sosValidationApi.validateInterfaceCompatibility,
    onError: (error: unknown) => {
      toast.error('Validation failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupValidationReport() {
  return useMutation({
    mutationFn: sosValidationApi.getValidationReport,
    onError: (error: unknown) => {
      toast.error('Failed to load validation report', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupValidationHistory() {
  return useMutation({
    mutationFn: sosValidationApi.getValidationHistory,
    onError: (error: unknown) => {
      toast.error('Failed to load validation history', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupValidationLineage() {
  return useMutation({
    mutationFn: sosValidationApi.getValidationLineage,
    onError: (error: unknown) => {
      toast.error('Failed to load validation lineage', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useCreateSosSystem() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.createSosSystem,
    onSuccess: (system) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('System saved', {
        description: `${system.system_name} is now in the SoS catalog.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to save system', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useUpdateSosSystem() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.updateSosSystem,
    onSuccess: (system) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('System updated', {
        description: `${system.system_name} was updated.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to update system', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useDeleteSosSystem() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.deleteSosSystem,
    onSuccess: () => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('System deleted');
    },
    onError: (error: unknown) => {
      toast.error('Failed to delete system', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useCreateSosPolicy() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.createSosPolicy,
    onSuccess: (policy) => {
      invalidateSosPolicyQueries(queryClient);
      toast.success('Policy saved', {
        description: `${policy.policy_name} is now part of SoS governance.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to save policy', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useUpdateSosPolicy() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.updateSosPolicy,
    onSuccess: (policy) => {
      invalidateSosPolicyQueries(queryClient);
      toast.success('Policy updated', {
        description: `${policy.policy_name} was updated.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to update policy', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useDeleteSosPolicy() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.deleteSosPolicy,
    onSuccess: () => {
      invalidateSosPolicyQueries(queryClient);
      toast.success('Policy deleted');
    },
    onError: (error: unknown) => {
      toast.error('Failed to delete policy', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useValidateSosPolicy() {
  return useMutation({
    mutationFn: sosValidationApi.validateSosPolicy,
    onError: (error: unknown) => {
      toast.error('Policy validation failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useValidateSosPolicyDryRun() {
  return useMutation({
    mutationFn: sosValidationApi.validateSosPolicyDryRun,
    onError: (error: unknown) => {
      toast.error('Policy dry run failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useValidateSosInterfaceSchema() {
  return useMutation({
    mutationFn: sosValidationApi.validateSosInterfaceSchema,
    onError: (error: unknown) => {
      toast.error('Schema validation failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useLookupSosDependencyGraph() {
  return useMutation({
    mutationFn: sosValidationApi.getSosDependencyGraph,
    onError: (error: unknown) => {
      toast.error('Failed to load dependency graph', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useRunSosWhatIfAnalysis() {
  return useMutation({
    mutationFn: sosValidationApi.runSosWhatIfAnalysis,
    onError: (error: unknown) => {
      toast.error('What-if analysis failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useSosContractApprovalRequests(
  contractId: string | null,
  options: {
    status?: string;
    offset?: number;
    limit?: number;
  } = {}
) {
  return useQuery({
    queryKey: [
      'sos',
      'contracts',
      contractId,
      'approval-requests',
      options.status ?? null,
      options.offset ?? 0,
      options.limit ?? 10,
    ],
    enabled: Boolean(contractId),
    queryFn: () =>
      sosValidationApi.listSosContractApprovalRequests({
        contractId: contractId ?? '',
        status: options.status,
        offset: options.offset,
        limit: options.limit,
      }),
    staleTime: 30 * 1000,
  });
}

export function useSosContractSignatures(contractId: string | null, limit = 10) {
  return useQuery({
    queryKey: ['sos', 'contracts', contractId, 'signatures', limit],
    enabled: Boolean(contractId),
    queryFn: () => sosValidationApi.listSosContractSignatures(contractId ?? '', limit),
    staleTime: 30 * 1000,
  });
}

export function useSosContractSigningKeyStatus() {
  return useQuery({
    queryKey: ['sos', 'contracts', 'signing-key'],
    queryFn: sosValidationApi.getSosContractSigningKeyStatus,
    staleTime: 30 * 1000,
  });
}

export function useRotateSosContractSigningKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.rotateSosContractSigningKey,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['sos', 'contracts', 'signing-key'] });
      toast.success('Contract signing key rotated', {
        description: `Now serving version ${result.current_signing_key_version}.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to rotate contract signing key', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useSosPolicyApprovalRequests(
  policyId: string | null,
  options: {
    status?: string;
    offset?: number;
    limit?: number;
  } = {}
) {
  return useQuery({
    queryKey: [
      'sos',
      'policies',
      policyId,
      'approval-requests',
      options.status ?? null,
      options.offset ?? 0,
      options.limit ?? 10,
    ],
    enabled: Boolean(policyId),
    queryFn: () =>
      sosValidationApi.listSosPolicyApprovalRequests({
        policyId: policyId ?? '',
        status: options.status,
        offset: options.offset,
        limit: options.limit,
      }),
    staleTime: 30 * 1000,
  });
}

export function useSosPolicyAttestations(policyId: string | null, limit = 10) {
  return useQuery({
    queryKey: ['sos', 'policies', policyId, 'attestations', limit],
    enabled: Boolean(policyId),
    queryFn: () => sosValidationApi.listSosPolicyAttestations(policyId ?? '', limit),
    staleTime: 30 * 1000,
  });
}

export function useSosPolicySigningKeyStatus() {
  return useQuery({
    queryKey: ['sos', 'policies', 'signing-key'],
    queryFn: sosValidationApi.getSosPolicySigningKeyStatus,
    staleTime: 30 * 1000,
  });
}

export function useRotateSosPolicySigningKey() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.rotateSosPolicySigningKey,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['sos', 'policies', 'signing-key'] });
      toast.success('Policy signing key rotated', {
        description: `Now serving version ${result.current_signing_key_version}.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to rotate policy signing key', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useReconcileSosRuntime() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.reconcileSosRuntime,
    onSuccess: (result) => {
      invalidateAllSosQueries(queryClient);
      toast.success('SoS reconcile completed', {
        description: `${result.system_count} systems, ${result.interface_count} interfaces, ${result.contract_count} contracts, ${result.policy_count} policies.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('SoS reconcile failed', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useCreateSosInterface() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.createSosInterface,
    onSuccess: (record) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Interface saved', {
        description: `${record.interface_name} is now available for validation.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to save interface', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useUpdateSosInterface() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.updateSosInterface,
    onSuccess: (record) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Interface updated', {
        description: `${record.interface_name} was updated.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to update interface', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useDeleteSosInterface() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.deleteSosInterface,
    onSuccess: () => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Interface deleted');
    },
    onError: (error: unknown) => {
      toast.error('Failed to delete interface', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useCreateSosContract() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.createSosContract,
    onSuccess: (contract) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Contract saved', {
        description: `${contract.contract_name} is ready for governance review.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to save contract', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useUpdateSosContract() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.updateSosContract,
    onSuccess: (contract) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Contract updated', {
        description: `${contract.contract_name} was updated.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to update contract', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useDeleteSosContract() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.deleteSosContract,
    onSuccess: () => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Contract deleted');
    },
    onError: (error: unknown) => {
      toast.error('Failed to delete contract', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useApproveSosContract() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.approveSosContract,
    onSuccess: (contract) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Contract approved', {
        description: `${contract.contract_name} is approved.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to approve contract', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useSignSosContract() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sosValidationApi.signSosContract,
    onSuccess: (contract) => {
      invalidateSosCatalogQueries(queryClient);
      toast.success('Contract signed', {
        description: `${contract.contract_name} is now locked.`,
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to sign contract', {
        description: getErrorMessage(error),
      });
    },
  });
}
