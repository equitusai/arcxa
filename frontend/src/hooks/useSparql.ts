/**
 * SPARQL Query Hooks
 *
 * React Query hooks for SPARQL execution, templates, and query management
 */

import { useState, useEffect, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as sparqlApi from '@/api/sparql';
import type { SparqlQueryHistoryItem, SavedSparqlQuery } from '@/api/types';

/**
 * Execute SPARQL query mutation
 */
export function useSparqlQuery() {
  return useMutation({
    mutationFn: async (sparql: string) => {
      const startTime = performance.now();
      const result = await sparqlApi.executeSparqlQuery(sparql);
      const executionTime = performance.now() - startTime;

      // Add to history
      const historyItem: SparqlQueryHistoryItem = {
        id: crypto.randomUUID(),
        query: sparql,
        timestamp: new Date().toISOString(),
        results_count: result.results.length,
        execution_time_ms: executionTime,
        success: true,
      };
      addToHistory(historyItem);

      return { ...result, executionTime };
    },
    onSuccess: (data) => {
      toast.success(`✅ Query executed successfully`, {
        description: `${data.results.length} results in ${data.executionTime.toFixed(0)}ms`,
      });
    },
    onError: (error: Error) => {
      toast.error('❌ Query execution failed', {
        description: error.message,
      });

      // Add error to history
      const historyItem: SparqlQueryHistoryItem = {
        id: crypto.randomUUID(),
        query: '', // Would need to capture from mutation context
        timestamp: new Date().toISOString(),
        results_count: 0,
        execution_time_ms: 0,
        success: false,
        error: error.message,
      };
      addToHistory(historyItem);
    },
  });
}

/**
 * Validate SPARQL query
 */
export function useSparqlValidation(query: string) {
  return useQuery({
    queryKey: ['sparql', 'validate', query],
    queryFn: () => sparqlApi.validateSparqlQuery(query),
    enabled: query.length > 0,
    staleTime: 1000, // Re-validate after 1 second
  });
}

/**
 * Get SPARQL templates
 */
export function useSparqlTemplates() {
  return useQuery({
    queryKey: ['sparql', 'templates'],
    queryFn: sparqlApi.getSparqlTemplates,
    staleTime: Infinity, // Templates don't change
  });
}

/**
 * Get saved queries
 */
export function useSavedQueries() {
  return useQuery({
    queryKey: ['sparql', 'saved'],
    queryFn: sparqlApi.getSavedQueries,
    staleTime: 30000, // 30 seconds
  });
}

/**
 * Save query mutation
 */
export function useSaveQuery() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sparqlApi.saveSparqlQuery,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sparql', 'saved'] });
      toast.success('✅ Query saved successfully');
    },
    onError: (error: Error) => {
      toast.error('❌ Failed to save query', {
        description: error.message,
      });
    },
  });
}

/**
 * Delete saved query mutation
 */
export function useDeleteSavedQuery() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: sparqlApi.deleteSavedQuery,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sparql', 'saved'] });
      toast.success('Query deleted');
    },
    onError: (error: Error) => {
      toast.error('Failed to delete query', {
        description: error.message,
      });
    },
  });
}

/**
 * Update saved query mutation
 */
export function useUpdateSavedQuery() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, updates }: { id: string; updates: Partial<SavedSparqlQuery> }) =>
      sparqlApi.updateSavedQuery(id, updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sparql', 'saved'] });
      toast.success('Query updated');
    },
    onError: (error: Error) => {
      toast.error('Failed to update query', {
        description: error.message,
      });
    },
  });
}

/**
 * Query history hook (localStorage-based)
 */
export function useQueryHistory() {
  const [history, setHistory] = useState<SparqlQueryHistoryItem[]>([]);

  useEffect(() => {
    loadHistory();
  }, []);

  const loadHistory = () => {
    const saved = localStorage.getItem('sparql_query_history');
    if (saved) {
      try {
        setHistory(JSON.parse(saved));
      } catch (e) {
        console.error('Failed to load query history:', e);
      }
    }
  };

  const clearHistory = useCallback(() => {
    localStorage.removeItem('sparql_query_history');
    setHistory([]);
    toast.success('Query history cleared');
  }, []);

  return { history, clearHistory, refresh: loadHistory };
}

/**
 * Query mode preference (localStorage)
 */
export function useQueryMode() {
  const [mode, setModeState] = useState<'beginner' | 'builder' | 'expert'>(() => {
    const saved = localStorage.getItem('sparql_mode');
    return (saved as any) || 'beginner';
  });

  const setMode = useCallback((newMode: 'beginner' | 'builder' | 'expert') => {
    setModeState(newMode);
    localStorage.setItem('sparql_mode', newMode);
  }, []);

  return { mode, setMode };
}

/**
 * Table density preference
 */
export function useTableDensity() {
  const [density, setDensityState] = useState<'compact' | 'comfortable'>(() => {
    const saved = localStorage.getItem('sparql_table_density');
    return (saved as any) || 'comfortable';
  });

  const setDensity = useCallback((newDensity: 'compact' | 'comfortable') => {
    setDensityState(newDensity);
    localStorage.setItem('sparql_table_density', newDensity);
  }, []);

  return { density, setDensity };
}

// ============================================================================
// Helper Functions
// ============================================================================

const MAX_HISTORY_SIZE = 50;

function addToHistory(item: SparqlQueryHistoryItem) {
  const saved = localStorage.getItem('sparql_query_history');
  let history: SparqlQueryHistoryItem[] = saved ? JSON.parse(saved) : [];

  // Add to front
  history.unshift(item);

  // Limit size
  if (history.length > MAX_HISTORY_SIZE) {
    history = history.slice(0, MAX_HISTORY_SIZE);
  }

  localStorage.setItem('sparql_query_history', JSON.stringify(history));
}
