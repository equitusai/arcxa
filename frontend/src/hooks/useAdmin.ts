import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as adminApi from '@/api/admin';
import { AuditQueryRequest } from '@/api/types';

// Cache Management
export function useCacheStats() {
  return useQuery({
    queryKey: ['admin', 'cache', 'stats'],
    queryFn: () => adminApi.getCacheStats(),
    staleTime: 30 * 1000,
  });
}

export function useClearModelCache() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => adminApi.clearModelCache(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin', 'cache'] });
      toast.success('Model cache cleared successfully');
    },
    onError: (error: any) => {
      console.error('Failed to clear model cache:', error);
      toast.error('Failed to clear model cache');
    },
  });
}

// Temporal Management
export function useTemporalStats() {
  return useQuery({
    queryKey: ['admin', 'temporal', 'stats'],
    queryFn: () => adminApi.getTemporalStats(),
    staleTime: 1 * 60 * 1000,
  });
}

export function useTemporalSummary() {
  return useQuery({
    queryKey: ['admin', 'temporal', 'summary'],
    queryFn: () => adminApi.getTemporalSummary(),
    staleTime: 1 * 60 * 1000,
  });
}

export function useAnalyzeTemporalChains() {
  return useMutation({
    mutationFn: () => adminApi.analyzeTemporalChains(),
    onError: (error: any) => {
      console.error('Temporal chain analysis failed:', error);
      toast.error('Failed to analyze temporal chains');
    },
  });
}

export function useCreateTemporalCheckpoint() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => adminApi.createTemporalCheckpoint(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin', 'temporal'] });
      toast.success('Temporal checkpoint created successfully');
    },
    onError: (error: any) => {
      console.error('Failed to create temporal checkpoint:', error);
      toast.error('Failed to create temporal checkpoint');
    },
  });
}

export function useCompactTemporalIndexes() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => adminApi.compactTemporalIndexes(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin', 'temporal'] });
      toast.success('Temporal indexes compacted successfully');
    },
    onError: (error: any) => {
      console.error('Failed to compact temporal indexes:', error);
      toast.error('Failed to compact temporal indexes');
    },
  });
}

export function useClearTemporalCache() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => adminApi.clearTemporalCache(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin', 'temporal'] });
      toast.success('Temporal cache cleared successfully');
    },
    onError: (error: any) => {
      console.error('Failed to clear temporal cache:', error);
      toast.error('Failed to clear temporal cache');
    },
  });
}

// WAL Management
export function useWalStatus() {
  return useQuery({
    queryKey: ['admin', 'wal', 'status'],
    queryFn: () => adminApi.getWalStatus(),
    staleTime: 30 * 1000,
  });
}

export function useWalOperations() {
  return useQuery({
    queryKey: ['admin', 'wal', 'operations'],
    queryFn: () => adminApi.getWalOperations(),
    staleTime: 30 * 1000,
  });
}

export function useTriggerWalReplay() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => adminApi.triggerWalReplay(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['admin', 'wal'] });
      toast.success('WAL replay triggered successfully');
    },
    onError: (error: any) => {
      console.error('Failed to trigger WAL replay:', error);
      toast.error('Failed to trigger WAL replay');
    },
  });
}

// Audit
export function useRecentAuditLogs(limit: number = 10) {
  return useQuery({
    queryKey: ['admin', 'audit', 'recent', limit],
    queryFn: () => adminApi.queryAuditLogs({ limit }),
    staleTime: 30 * 1000,
    refetchInterval: 30 * 1000, // Auto-refresh every 30 seconds
  });
}

export function useAuditActivityHistory(hours: number = 24) {
  return useQuery({
    queryKey: ['admin', 'audit', 'history', hours],
    queryFn: () => {
      const now = new Date();
      const startTime = new Date(now.getTime() - hours * 60 * 60 * 1000);

      return adminApi.queryAuditLogs({
        start_time: startTime.toISOString(),
        end_time: now.toISOString(),
        limit: 1000, // Get up to 1000 events for the period
      });
    },
    staleTime: 60 * 1000, // 1 minute
    refetchInterval: 60 * 1000, // Refresh every minute
  });
}

export function useQueryAuditLogs() {
  return useMutation({
    mutationFn: (query: AuditQueryRequest) => adminApi.queryAuditLogs(query),
    onError: (error: any) => {
      console.error('Audit log query failed:', error);
      toast.error('Failed to query audit logs');
    },
  });
}

export function useExportAuditLogs() {
  return useMutation({
    mutationFn: (params: any) => adminApi.exportAuditLogs(params),
    onSuccess: () => {
      toast.success('Audit logs exported successfully');
    },
    onError: (error: any) => {
      console.error('Audit log export failed:', error);
      toast.error('Failed to export audit logs');
    },
  });
}
