/**
 * Admin & Monitoring API
 */

import api from './client';
import { CacheStats, TemporalStats, WalStatus, AuditQueryRequest, AuditQueryResponse } from './types';

// Cache Management
export async function getCacheStats(): Promise<CacheStats> {
  return api.get<CacheStats>('/orchestration/cache/stats');
}

export async function clearModelCache(): Promise<void> {
  return api.post('/orchestration/cache/clear');
}

// Temporal Management
export async function getTemporalStats(): Promise<TemporalStats> {
  return api.get<TemporalStats>('/admin/temporal/stats');
}

export async function getTemporalSummary(): Promise<any> {
  return api.get('/admin/temporal/summary');
}

export async function analyzeTemporalChains(): Promise<any> {
  return api.get('/admin/temporal/analyze');
}

export async function createTemporalCheckpoint(): Promise<void> {
  return api.post('/admin/temporal/checkpoint');
}

export async function compactTemporalIndexes(): Promise<void> {
  return api.post('/admin/temporal/compact');
}

export async function clearTemporalCache(): Promise<void> {
  return api.post('/admin/temporal/cache/clear');
}

// WAL Management
export async function getWalStatus(): Promise<WalStatus> {
  return api.get<WalStatus>('/admin/wal/status');
}

export async function getWalOperations(): Promise<any[]> {
  return api.get<any[]>('/admin/wal/operations');
}

export async function triggerWalReplay(): Promise<void> {
  return api.post('/admin/wal/replay');
}

// Audit
export async function queryAuditLogs(query: AuditQueryRequest): Promise<AuditQueryResponse> {
  return api.post<AuditQueryResponse>('/admin/audit/query', query);
}

export async function exportAuditLogs(params: any): Promise<Blob> {
  return api.post('/admin/audit/export', params, {
    responseType: 'blob',
  });
}
