/**
 * Data Quality API
 *
 * Provides functions for quality scorecards, violations, and rule management
 */

import api from './client';
import {
  QualityScorecard,
  QualityViolation,
  QualityRule,
  CreateQualityRuleRequest,
  PaginationParams,
} from './types';

/**
 * Get quality scorecard for a dataset
 *
 * @param dataset - Dataset identifier
 * @returns QualityScorecard
 */
export async function getQualityScorecard(dataset: string): Promise<QualityScorecard> {
  return api.get<QualityScorecard>(`/quality/dataset/${dataset}/scorecard`);
}

/**
 * List quality violations
 *
 * @param params - Pagination and filter parameters
 * @returns Array of QualityViolations
 */
export async function listQualityViolations(
  params?: PaginationParams & { dataset?: string; severity?: string; resolved?: boolean }
): Promise<QualityViolation[]> {
  return api.get<QualityViolation[]>('/quality/violations', { params });
}

/**
 * Get quality rule by ID
 *
 * @param ruleId - Rule ID
 * @returns QualityRule
 */
export async function getQualityRule(ruleId: string): Promise<QualityRule> {
  return api.get<QualityRule>(`/quality/rules/${ruleId}`);
}

/**
 * Create quality rule
 *
 * @param request - Create quality rule request
 * @returns Created QualityRule
 */
export async function createQualityRule(
  request: CreateQualityRuleRequest
): Promise<QualityRule> {
  return api.post<QualityRule>('/quality/rules', request);
}
