import { describe, expect, it } from 'vitest';

import type { SosValidationReport } from '@/api/sosValidation';
import {
  deriveHistoryInsights,
  deriveLineageInsights,
} from '@/components/sos/sosReportInsights';

function buildReport(overrides: Partial<SosValidationReport>): SosValidationReport {
  return {
    report_id: 'report-1',
    validation_id: 'validation-1',
    subject_type: 'interface_pair',
    subject_key: 'interface_pair:iface.provider:iface.consumer',
    validation_type: 'interface_compatibility',
    passed: true,
    confidence: 0.8,
    checks: [],
    validated_at: '2026-04-23T12:00:00Z',
    previous_report_id: null,
    change_summary: {
      resolved_checks: [],
      new_failures: [],
      confidence_delta: 0,
      schema_or_policy_version_changed: false,
    },
    workflow_execution_id: null,
    workflow_step_id: null,
    ontology_refs: [],
    shape_refs: [],
    policy_refs: [],
    schema_hashes: {},
    ...overrides,
  };
}

describe('sosReportInsights', () => {
  it('derives history summaries from loaded persisted reports', () => {
    const reports = [
      buildReport({
        report_id: 'report-3',
        validation_id: 'validation-3',
        passed: true,
        confidence: 0.9,
        validated_at: '2026-04-25T12:00:00Z',
        previous_report_id: 'report-2',
        change_summary: {
          resolved_checks: ['schema.mismatch'],
          new_failures: [],
          confidence_delta: 0.4,
          schema_or_policy_version_changed: false,
        },
      }),
      buildReport({
        report_id: 'report-1',
        validation_id: 'validation-1',
        passed: true,
        confidence: 0.7,
        validated_at: '2026-04-23T12:00:00Z',
      }),
      buildReport({
        report_id: 'report-2',
        validation_id: 'validation-2',
        passed: false,
        confidence: 0.5,
        validated_at: '2026-04-24T12:00:00Z',
        previous_report_id: 'report-1',
        change_summary: {
          resolved_checks: [],
          new_failures: ['schema.mismatch'],
          confidence_delta: -0.2,
          schema_or_policy_version_changed: true,
        },
      }),
    ];

    const insights = deriveHistoryInsights(reports);

    expect(insights.timeline.map((entry) => entry.report.report_id)).toEqual([
      'report-1',
      'report-2',
      'report-3',
    ]);
    expect(insights.trend).toBe('mixed');
    expect(insights.passRate).toBeCloseTo(2 / 3);
    expect(insights.netConfidenceDelta).toBeCloseTo(0.2);
    expect(insights.statusFlipCount).toBe(2);
    expect(insights.versionChangeCount).toBe(1);
    expect(insights.totalNewFailures).toBe(1);
    expect(insights.totalResolvedChecks).toBe(1);
    expect(insights.timeline[1]?.movement).toBe('regressed');
    expect(insights.timeline[2]?.movement).toBe('improved');
  });

  it('builds a readable lineage progression and preserves branch context', () => {
    const reports = [
      buildReport({
        report_id: 'report-4',
        validation_id: 'validation-4',
        validated_at: '2026-04-26T12:00:00Z',
        previous_report_id: 'report-2',
      }),
      buildReport({
        report_id: 'report-1',
        validation_id: 'validation-1',
        validated_at: '2026-04-23T12:00:00Z',
      }),
      buildReport({
        report_id: 'report-3',
        validation_id: 'validation-3',
        validated_at: '2026-04-25T12:00:00Z',
        previous_report_id: 'report-2',
      }),
      buildReport({
        report_id: 'report-2',
        validation_id: 'validation-2',
        validated_at: '2026-04-24T12:00:00Z',
        previous_report_id: 'report-1',
      }),
    ];

    const insights = deriveLineageInsights(reports, [
      {
        from_report_id: 'report-1',
        to_report_id: 'report-2',
        relationship: 'supersedes',
      },
      {
        from_report_id: 'report-2',
        to_report_id: 'report-3',
        relationship: 'supersedes',
      },
      {
        from_report_id: 'report-2',
        to_report_id: 'report-4',
        relationship: 'fork_candidate',
      },
    ]);

    expect(insights.orderedReports.map((report) => report.report_id)).toEqual([
      'report-1',
      'report-2',
      'report-3',
      'report-4',
    ]);
    expect(insights.rootCount).toBe(1);
    expect(insights.headCount).toBe(2);
    expect(insights.branchPointCount).toBe(1);
    expect(insights.explicitEdgeCount).toBe(3);
    expect(insights.inferredEdgeCount).toBe(0);

    const branchEntry = insights.progression.find(
      (entry) => entry.report.report_id === 'report-2'
    );
    const branchChild = insights.progression.find(
      (entry) => entry.report.report_id === 'report-4'
    );

    expect(branchEntry?.isBranchPoint).toBe(true);
    expect(branchChild?.primaryUpstreamReportId).toBe('report-2');
    expect(branchChild?.primaryUpstreamRelationship).toBe('fork_candidate');
  });
});
