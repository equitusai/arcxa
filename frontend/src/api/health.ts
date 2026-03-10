/**
 * Health & Monitoring API
 */

import { apiClient } from './client';

// Health endpoints are at root level, not under /api/v1
const HEALTH_BASE_URL = import.meta.env.VITE_API_BASE_URL?.replace('/api/v1', '') || 'http://localhost:8080';

export async function getHealth(): Promise<{ status: string }> {
  const response = await apiClient.get(`${HEALTH_BASE_URL}/health`);
  return response.data;
}

export async function getLiveness(): Promise<{ status: string }> {
  const response = await apiClient.get(`${HEALTH_BASE_URL}/health/live`);
  return response.data;
}

export async function getReadiness(): Promise<{ status: string }> {
  const response = await apiClient.get(`${HEALTH_BASE_URL}/health/ready`);
  return response.data;
}

export async function getStorageHealth(): Promise<any> {
  const response = await apiClient.get(`${HEALTH_BASE_URL}/health/storage`);
  return response.data;
}

export async function getMetrics(): Promise<string> {
  const response = await apiClient.get(`${HEALTH_BASE_URL}/metrics`, { responseType: 'text' });
  return response.data;
}
