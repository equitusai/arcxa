/**
 * React Query hooks for Ontology Management API
 *
 * Provides hooks for:
 * - Listing and fetching ontologies
 * - Registering and updating ontologies
 * - Activating/deactivating ontologies
 * - Deleting ontologies (soft and hard delete)
 * - Getting ontology tree structures
 * - Merging multiple ontologies
 * - Validating ontology content
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as ontologyApi from '../api/ontology';
import type {
  OntologyMetadata,
  RegisterOntologyRequest,
  OntologyTreeResponse,
  RegisteredOntology,
} from '../api/ontology';

// ============================================================================
// Query Hooks (Data Fetching)
// ============================================================================

/**
 * List all registered ontologies
 *
 * @param activeOnly - If true, only return active ontologies (default: false)
 */
export function useOntologies(activeOnly: boolean = false) {
  return useQuery<OntologyMetadata[]>({
    queryKey: ['ontologies', activeOnly ? 'active' : 'all'],
    queryFn: () => ontologyApi.listOntologies(activeOnly),
  });
}

/**
 * Get a specific ontology by ID
 *
 * @param id - Ontology ID
 */
export function useOntology(id: string | undefined) {
  return useQuery<RegisteredOntology>({
    queryKey: ['ontologies', id],
    queryFn: () => ontologyApi.getOntology(id!),
    enabled: !!id,
  });
}

/**
 * Get ontology tree structure for visualization
 *
 * @param id - Ontology ID
 * @param options - Tree generation options
 */
export function useOntologyTree(
  id: string | undefined,
  options?: {
    maxDepth?: number;
    includeProperties?: boolean;
    includeIndividuals?: boolean;
  }
) {
  return useQuery<OntologyTreeResponse>({
    queryKey: ['ontologies', id, 'tree', options],
    queryFn: () => ontologyApi.getOntologyTree(id!, options),
    enabled: !!id,
  });
}

/**
 * Get merged ontology combining multiple ontologies
 *
 * @param ontologyIds - Optional array of ontology IDs. If empty, merges all active ontologies
 */
export function useMergedOntology(ontologyIds?: string[]) {
  return useQuery<{
    content: string;
    size_bytes: number;
    included_ontologies: string[];
  }>({
    queryKey: ['ontologies', 'merged', ontologyIds],
    queryFn: () => ontologyApi.getMergedOntology(ontologyIds),
  });
}

// ============================================================================
// Mutation Hooks (Data Modification)
// ============================================================================

/**
 * Register a new custom ontology
 */
export function useRegisterOntology() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: RegisterOntologyRequest) =>
      ontologyApi.registerOntology(request),
    onSuccess: (result) => {
      // Invalidate ontologies list cache
      queryClient.invalidateQueries({ queryKey: ['ontologies'] });

      toast.success('Ontology registered successfully', {
        description: `${result.metadata.name} (${result.metadata.id})`,
      });
    },
    onError: (error: Error) => {
      toast.error('Failed to register ontology', {
        description: error.message,
      });
    },
  });
}

/**
 * Update an existing ontology
 */
export function useUpdateOntology() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      id,
      updates,
    }: {
      id: string;
      updates: Partial<RegisterOntologyRequest>;
    }) => ontologyApi.updateOntology(id, updates),
    onSuccess: (result, variables) => {
      // Invalidate specific ontology and list cache
      queryClient.invalidateQueries({ queryKey: ['ontologies', variables.id] });
      queryClient.invalidateQueries({ queryKey: ['ontologies'] });

      toast.success('Ontology updated successfully', {
        description: result.metadata.name,
      });
    },
    onError: (error: Error) => {
      toast.error('Failed to update ontology', {
        description: error.message,
      });
    },
  });
}

/**
 * Activate an ontology for use in field mapping
 */
export function useActivateOntology() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => ontologyApi.activateOntology(id),
    onSuccess: (_, id) => {
      // Invalidate ontology and lists cache
      queryClient.invalidateQueries({ queryKey: ['ontologies', id] });
      queryClient.invalidateQueries({ queryKey: ['ontologies'] });

      toast.success('Ontology activated', {
        description: `Ontology ${id} is now active`,
      });
    },
    onError: (error: Error, id) => {
      toast.error('Failed to activate ontology', {
        description: error.message,
      });
    },
  });
}

/**
 * Deactivate an ontology
 */
export function useDeactivateOntology() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => ontologyApi.deactivateOntology(id),
    onSuccess: (_, id) => {
      // Invalidate ontology and lists cache
      queryClient.invalidateQueries({ queryKey: ['ontologies', id] });
      queryClient.invalidateQueries({ queryKey: ['ontologies'] });

      toast.success('Ontology deactivated', {
        description: `Ontology ${id} is now inactive`,
      });
    },
    onError: (error: Error) => {
      toast.error('Failed to deactivate ontology', {
        description: error.message,
      });
    },
  });
}

/**
 * Delete an ontology
 *
 * @param permanent - If true, permanently delete. If false (default), soft delete (deactivate)
 */
export function useDeleteOntology() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, permanent = false }: { id: string; permanent?: boolean }) =>
      ontologyApi.deleteOntology(id, permanent),
    onSuccess: (_, variables) => {
      // Invalidate all ontologies cache
      queryClient.invalidateQueries({ queryKey: ['ontologies'] });

      toast.success(
        variables.permanent ? 'Ontology permanently deleted' : 'Ontology deactivated',
        {
          description: variables.permanent
            ? `Ontology ${variables.id} has been permanently removed`
            : `Ontology ${variables.id} has been deactivated`,
        }
      );
    },
    onError: (error: Error, variables) => {
      toast.error('Failed to delete ontology', {
        description: error.message,
      });
    },
  });
}

/**
 * Validate ontology content without registering
 */
export function useValidateOntology() {
  return useMutation({
    mutationFn: ({
      content,
      format = 'turtle',
    }: {
      content: string;
      format?: string;
    }) => ontologyApi.validateOntology(content, format),
    onSuccess: (result) => {
      if (result.valid && result.warnings.length === 0) {
        toast.success('Ontology validation passed', {
          description: 'The ontology syntax is valid',
        });
      } else if (result.valid && result.warnings.length > 0) {
        toast.warning('Ontology valid with warnings', {
          description: `${result.warnings.length} warning(s) found`,
        });
      } else {
        toast.error('Ontology validation failed', {
          description: `${result.errors.length} error(s) found`,
        });
      }
    },
    onError: (error: Error) => {
      toast.error('Validation request failed', {
        description: error.message,
      });
    },
  });
}
