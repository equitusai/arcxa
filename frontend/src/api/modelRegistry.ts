/**
 * ML Model Registry API (Legacy - use models.ts instead)
 * @deprecated Use models.ts for new code
 */

import api from './client';
import { ModelMetadata, RegisterModelRequest } from './types';

export async function listMlModels(): Promise<ModelMetadata[]> {
  return api.get<ModelMetadata[]>('/orchestration/models');
}

export async function getMlModel(modelId: string): Promise<ModelMetadata> {
  return api.get<ModelMetadata>(`/orchestration/models/${modelId}`);
}

export async function registerMlModel(request: RegisterModelRequest): Promise<ModelMetadata> {
  return api.post<ModelMetadata>('/orchestration/models', request);
}

export async function deleteMlModel(modelId: string): Promise<void> {
  return api.delete(`/orchestration/models/${modelId}`);
}
