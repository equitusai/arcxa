/**
 * Rule Management API
 */

import api from './client';

export async function loadRule(ruleId: string, wasmCode: ArrayBuffer): Promise<void> {
  return api.post(`/orchestration/rules/${ruleId}`, wasmCode, {
    headers: { 'Content-Type': 'application/wasm' },
  });
}

export async function executeRule(ruleId: string, input: any): Promise<any> {
  return api.post(`/orchestration/rules/${ruleId}/execute`, input);
}

export async function clearRuleCache(): Promise<void> {
  return api.post('/orchestration/rules/cache/clear');
}
