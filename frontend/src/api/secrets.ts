/**
 * Secrets API
 *
 * Stores and retrieves credentials in the backend secret store.
 */

import { api } from './client';

export interface PutSecretRequest {
  value: Record<string, any>;
  description?: string;
  tags?: string[];
  store?: string;
}

export interface PutSecretResponse {
  path: string;
  version: string;
  store: string;
  created_at: string;
}

const VAULT_SECRET_REF_PREFIX = 'vault://';

export function buildDatasourceSecretRef(datasourceName: string): string {
  return `vault://datasources/${datasourceName}/credentials`;
}

export function secretRefToApiPath(secretRefOrPath: string): string {
  if (secretRefOrPath.startsWith(VAULT_SECRET_REF_PREFIX)) {
    return secretRefOrPath.slice(VAULT_SECRET_REF_PREFIX.length);
  }

  return secretRefOrPath;
}

export async function putSecret(
  path: string,
  request: PutSecretRequest
): Promise<PutSecretResponse> {
  const encodedPath = encodeURIComponent(secretRefToApiPath(path));
  return api.put(`/secrets/${encodedPath}`, request);
}

export async function storeDatasourceCredentials(
  path: string,
  credentials: Record<string, any>,
  description?: string
): Promise<PutSecretResponse> {
  return putSecret(path, {
    value: credentials,
    description,
    store: 'default',
  });
}
