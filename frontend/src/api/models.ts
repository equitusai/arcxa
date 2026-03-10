/**
 * Model Registry API
 */

import api from './client';
import type {
  ModelMetadata,
  ModelSummary,
  RegisterModelRequest,
  CircuitBreakerStatus,
  ModelCacheStats,
  ModelInvocationRequest,
  ModelInvocationResponse,
} from './types';

// ============================================================================
// Model Registration & Management
// ============================================================================

/**
 * Register a new model in the orchestration registry
 */
export async function registerModel(request: RegisterModelRequest): Promise<ModelMetadata> {
  return api.post<ModelMetadata>('/orchestration/models', request);
}

/**
 * List all registered models
 */
export async function listModels(): Promise<ModelSummary[]> {
  return api.get<ModelSummary[]>('/orchestration/models');
}

/**
 * Get model details by ID
 */
export async function getModel(modelId: string): Promise<ModelMetadata> {
  return api.get<ModelMetadata>(`/orchestration/models/${modelId}`);
}

/**
 * Update model metadata
 */
export async function updateModel(
  modelId: string,
  request: Partial<RegisterModelRequest>
): Promise<ModelMetadata> {
  return api.put<ModelMetadata>(`/orchestration/models/${modelId}`, request);
}

/**
 * Delete model from registry
 */
export async function deleteModel(modelId: string): Promise<void> {
  return api.delete(`/orchestration/models/${modelId}`);
}

// ============================================================================
// Model Operations
// ============================================================================

/**
 * Invoke model with features
 */
export async function invokeModel(request: ModelInvocationRequest): Promise<ModelInvocationResponse> {
  return api.post<ModelInvocationResponse>('/orchestration/invoke', request);
}

/**
 * Test model endpoint connectivity
 */
export async function testModelEndpoint(url: string, protocol: string): Promise<{ success: boolean; latency_ms?: number; error?: string }> {
  try {
    const start = Date.now();

    // For HTTP, try a simple GET/HEAD request
    if (protocol === 'http') {
      const response = await fetch(url, {
        method: 'HEAD',
        mode: 'no-cors',
      });
      const latency = Date.now() - start;
      return { success: true, latency_ms: latency };
    }

    // For other protocols, we'd need different testing logic
    return { success: false, error: `Testing not implemented for protocol: ${protocol}` };
  } catch (error: any) {
    return { success: false, error: error.message };
  }
}

// ============================================================================
// Circuit Breaker & Cache Management
// ============================================================================

/**
 * Get circuit breaker status for a model
 */
export async function getCircuitBreakerStatus(modelId: string): Promise<CircuitBreakerStatus> {
  return api.get<CircuitBreakerStatus>(`/orchestration/circuit-breaker/${modelId}`);
}

/**
 * Reset circuit breaker for a model
 */
export async function resetCircuitBreaker(modelId: string): Promise<void> {
  return api.post(`/orchestration/circuit-breaker/${modelId}/reset`);
}

/**
 * Get cache statistics
 */
export async function getCacheStats(): Promise<ModelCacheStats> {
  return api.get<ModelCacheStats>('/orchestration/cache/stats');
}

/**
 * Clear model cache
 */
export async function clearCache(): Promise<void> {
  return api.post('/orchestration/cache/clear');
}
