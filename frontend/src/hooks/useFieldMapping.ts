/**
 * useFieldMapping Hook
 *
 * Manages field mapping workflow state and API interactions.
 * Provides a clean interface for components to interact with the Field Mapping API.
 */

import { useState, useCallback } from 'react';
import { useAuthStore } from '@/stores/auth';
import {
  analyzeForMapping,
  getMappingSession,
  reviewMappings,
  applyMappings,
  importDataWithMappings,
  type AnalyzeForMappingRequest,
  type MappingSession,
  type FieldMappingDecision,
  type ReviewMappingsResponse,
  type ApplyMappingsResponse,
  type ImportDataResponse,
  type ImportDataRequest,
} from '@/api/field-mapping';

interface UseFieldMappingOptions {
  onAnalysisComplete?: (sessionId: string) => void;
  onReviewComplete?: (response: ReviewMappingsResponse) => void;
  onApplyComplete?: (response: ApplyMappingsResponse) => void;
  onImportComplete?: (response: ImportDataResponse) => void;
  onError?: (error: Error) => void;
}

export function useFieldMapping(options: UseFieldMappingOptions = {}) {
  const [session, setSession] = useState<MappingSession | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [currentStep, setCurrentStep] = useState<
    'idle' | 'analyzing' | 'reviewing' | 'applying' | 'importing' | 'complete'
  >('idle');

  /**
   * Start field mapping analysis for a datasource
   */
  const startAnalysis = useCallback(
    async (
      datasourceId: string,
      request: Omit<AnalyzeForMappingRequest, 'user_id'> & { user_id?: string }
    ) => {
      setLoading(true);
      setError(null);
      setCurrentStep('analyzing');

      try {
        // Start analysis
        console.log('[useFieldMapping] Starting analysis...', { datasourceId, request });
        const analysisResponse = await analyzeForMapping(datasourceId, {
          user_id: request.user_id || useAuthStore.getState().user?.id || 'anonymous',
          sample_size: request.sample_size || 1000,
          auto_approve_threshold: request.auto_approve_threshold || 0.95,
          min_confidence: request.min_confidence || 0.5,
          max_candidates: request.max_candidates || 10,
          ...request,
        });
        console.log('[useFieldMapping] Analysis response:', analysisResponse);

        // Fetch full session details
        const sessionData = await getMappingSession(analysisResponse.session_id);
        console.log('[useFieldMapping] Session data:', sessionData);
        setSession(sessionData);
        setCurrentStep('reviewing');
        console.log('[useFieldMapping] Set step to reviewing');

        options.onAnalysisComplete?.(analysisResponse.session_id);

        return sessionData;
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to start analysis');
        setError(error);
        setCurrentStep('idle');
        options.onError?.(error);
        throw error;
      } finally {
        setLoading(false);
      }
    },
    [options]
  );

  /**
   * Load an existing mapping session
   */
  const loadSession = useCallback(
    async (sessionId: string) => {
      setLoading(true);
      setError(null);

      try {
        const sessionData = await getMappingSession(sessionId);
        setSession(sessionData);

        // Set appropriate step based on session status
        switch (sessionData.status) {
          case 'draft':
          case 'pending_review':
            setCurrentStep('reviewing');
            break;
          case 'approved':
            setCurrentStep('applying');
            break;
          case 'applied':
          case 'active':
            setCurrentStep('complete');
            break;
        }

        return sessionData;
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to load session');
        setError(error);
        options.onError?.(error);
        throw error;
      } finally {
        setLoading(false);
      }
    },
    [options]
  );

  /**
   * Submit field mapping review decisions
   */
  const submitReview = useCallback(
    async (
      sessionId: string,
      decisions: FieldMappingDecision[],
      userId: string,
      finalize: boolean = true
    ) => {
      if (!session) {
        throw new Error('No active session');
      }

      setLoading(true);
      setError(null);

      try {
        const response = await reviewMappings(sessionId, {
          field_mappings: decisions,
          reviewed_by: userId,
          finalize,
        });

        // Reload session to get updated state
        const updatedSession = await getMappingSession(sessionId);
        setSession(updatedSession);

        if (response.ready_to_apply) {
          setCurrentStep('applying');
        }

        options.onReviewComplete?.(response);

        return response;
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to submit review');
        setError(error);
        options.onError?.(error);
        throw error;
      } finally {
        setLoading(false);
      }
    },
    [session, options]
  );

  /**
   * Apply approved mappings to RDF store
   */
  const applyToRdf = useCallback(
    async (sessionId: string, createDefaultImport: boolean = true) => {
      if (!session) {
        throw new Error('No active session');
      }

      setLoading(true);
      setError(null);
      setCurrentStep('applying');

      try {
        const response = await applyMappings(sessionId, {
          create_default_import: createDefaultImport,
        });

        // Reload session to get updated state
        const updatedSession = await getMappingSession(sessionId);
        setSession(updatedSession);
        setCurrentStep('complete');

        options.onApplyComplete?.(response);

        return response;
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to apply mappings');
        setError(error);
        setCurrentStep('reviewing');
        options.onError?.(error);
        throw error;
      } finally {
        setLoading(false);
      }
    },
    [session, options]
  );

  /**
   * Import data using approved field mappings
   */
  const importData = useCallback(
    async (
      sessionId: string,
      request: Omit<ImportDataRequest, 'user_id'> & { user_id?: string }
    ) => {
      if (!session) {
        throw new Error('No active session');
      }

      setLoading(true);
      setError(null);
      setCurrentStep('importing');

      try {
        const response = await importDataWithMappings(sessionId, {
          user_id: request.user_id || useAuthStore.getState().user?.id || 'anonymous',
          batch_size: request.batch_size || 1000,
          ...request,
        });

        options.onImportComplete?.(response);

        return response;
      } catch (err) {
        const error = err instanceof Error ? err : new Error('Failed to import data');
        setError(error);
        setCurrentStep('complete');
        options.onError?.(error);
        throw error;
      } finally {
        setLoading(false);
      }
    },
    [session, options]
  );

  /**
   * Reset hook state
   */
  const reset = useCallback(() => {
    setSession(null);
    setError(null);
    setLoading(false);
    setCurrentStep('idle');
  }, []);

  /**
   * Skip field mapping
   */
  const skip = useCallback(() => {
    reset();
  }, [reset]);

  return {
    // State
    session,
    loading,
    error,
    currentStep,

    // Actions
    startAnalysis,
    loadSession,
    submitReview,
    applyToRdf,
    importData,
    reset,
    skip,

    // Computed properties
    isAnalyzing: currentStep === 'analyzing',
    isReviewing: currentStep === 'reviewing',
    isApplying: currentStep === 'applying',
    isImporting: currentStep === 'importing',
    isComplete: currentStep === 'complete',
    canReview: session?.status === 'pending_review' || session?.status === 'draft',
    canApply: session?.status === 'approved',
    canImport: session?.status === 'active',
  };
}
