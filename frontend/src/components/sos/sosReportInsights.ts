import type {
  SosValidationLineageEdge,
  SosValidationReport,
} from "@/api/sosValidation";

export type SosHistoryTrend = "improving" | "degrading" | "mixed" | "steady";
export type SosReportMovement = "improved" | "regressed" | "mixed" | "steady";

export interface SosHistoryTimelineEntry {
  report: SosValidationReport;
  newFailureCount: number;
  resolvedCount: number;
  confidenceDelta: number;
  versionChanged: boolean;
  movement: SosReportMovement;
  isBaseline: boolean;
  isLatest: boolean;
}

export interface SosHistoryInsights {
  chronologicalReports: SosValidationReport[];
  totalReports: number;
  passCount: number;
  failureCount: number;
  passRate: number;
  averageConfidence: number;
  baselineReport: SosValidationReport | null;
  latestReport: SosValidationReport | null;
  netConfidenceDelta: number;
  statusFlipCount: number;
  versionChangeCount: number;
  totalNewFailures: number;
  totalResolvedChecks: number;
  repeatedNewFailures: string[];
  repeatedResolvedChecks: string[];
  trend: SosHistoryTrend;
  timeline: SosHistoryTimelineEntry[];
}

export interface SosLineageProgressionEntry {
  report: SosValidationReport;
  sequence: number;
  upstreamCount: number;
  downstreamCount: number;
  relationshipFromPrevious: string | null;
  primaryUpstreamReportId: string | null;
  primaryUpstreamRelationship: string | null;
  isRoot: boolean;
  isHead: boolean;
  isBranchPoint: boolean;
  isMergePoint: boolean;
}

export interface SosLineageInsights {
  orderedReports: SosValidationReport[];
  progression: SosLineageProgressionEntry[];
  totalReports: number;
  explicitEdgeCount: number;
  inferredEdgeCount: number;
  rootCount: number;
  headCount: number;
  branchPointCount: number;
  mergePointCount: number;
  componentCount: number;
  relationshipCounts: Array<{ relationship: string; count: number }>;
  baselineReport: SosValidationReport | null;
  latestReport: SosValidationReport | null;
}

export function deriveHistoryInsights(
  reports: SosValidationReport[],
): SosHistoryInsights {
  const chronologicalReports = sortReportsByValidatedAt(reports, "asc");
  const totalReports = chronologicalReports.length;
  const passCount = chronologicalReports.filter(
    (report) => report.passed,
  ).length;
  const failureCount = totalReports - passCount;
  const averageConfidence =
    totalReports > 0
      ? chronologicalReports.reduce(
          (sum, report) => sum + report.confidence,
          0,
        ) / totalReports
      : 0;
  const baselineReport = chronologicalReports[0] ?? null;
  const latestReport =
    totalReports > 0
      ? chronologicalReports[chronologicalReports.length - 1]
      : null;
  const netConfidenceDelta =
    baselineReport && latestReport
      ? latestReport.confidence - baselineReport.confidence
      : 0;

  let statusFlipCount = 0;
  let versionChangeCount = 0;
  let totalNewFailures = 0;
  let totalResolvedChecks = 0;

  const newFailureCounts = new Map<string, number>();
  const resolvedCounts = new Map<string, number>();

  chronologicalReports.forEach((report, index) => {
    if (
      index > 0 &&
      chronologicalReports[index - 1]?.passed !== report.passed
    ) {
      statusFlipCount += 1;
    }

    const newFailures = report.change_summary.new_failures;
    const resolvedChecks = report.change_summary.resolved_checks;

    totalNewFailures += newFailures.length;
    totalResolvedChecks += resolvedChecks.length;

    if (report.change_summary.schema_or_policy_version_changed) {
      versionChangeCount += 1;
    }

    newFailures.forEach((signal) => {
      newFailureCounts.set(signal, (newFailureCounts.get(signal) ?? 0) + 1);
    });

    resolvedChecks.forEach((signal) => {
      resolvedCounts.set(signal, (resolvedCounts.get(signal) ?? 0) + 1);
    });
  });

  const repeatedNewFailures = summarizeRepeatedSignals(newFailureCounts);
  const repeatedResolvedChecks = summarizeRepeatedSignals(resolvedCounts);

  const timeline = chronologicalReports.map((report, index) => ({
    report,
    newFailureCount: report.change_summary.new_failures.length,
    resolvedCount: report.change_summary.resolved_checks.length,
    confidenceDelta: report.change_summary.confidence_delta,
    versionChanged: report.change_summary.schema_or_policy_version_changed,
    movement: getReportMovement(report),
    isBaseline: index === 0,
    isLatest: index === chronologicalReports.length - 1,
  }));

  return {
    chronologicalReports,
    totalReports,
    passCount,
    failureCount,
    passRate: totalReports > 0 ? passCount / totalReports : 0,
    averageConfidence,
    baselineReport,
    latestReport,
    netConfidenceDelta,
    statusFlipCount,
    versionChangeCount,
    totalNewFailures,
    totalResolvedChecks,
    repeatedNewFailures,
    repeatedResolvedChecks,
    trend: getHistoryTrend({
      baselineReport,
      latestReport,
      netConfidenceDelta,
      statusFlipCount,
      totalNewFailures,
      totalResolvedChecks,
      versionChangeCount,
    }),
    timeline,
  };
}

export function deriveLineageInsights(
  reports: SosValidationReport[],
  edges: SosValidationLineageEdge[],
): SosLineageInsights {
  const reportById = new Map(
    reports.map((report) => [report.report_id, report]),
  );
  const adjacency = new Map<string, Set<string>>();
  const reverseAdjacency = new Map<string, Set<string>>();
  const relationshipByPair = new Map<string, string>();
  const explicitEdgeKeys = new Set<string>();
  const inferredEdgeKeys = new Set<string>();

  const addRelationship = (
    fromId: string,
    toId: string,
    relationship: string,
    source: "explicit" | "inferred",
  ) => {
    if (!reportById.has(fromId) || !reportById.has(toId) || fromId === toId) {
      return;
    }

    const key = getPairKey(fromId, toId);
    if (!adjacency.has(fromId)) {
      adjacency.set(fromId, new Set());
    }
    if (!reverseAdjacency.has(toId)) {
      reverseAdjacency.set(toId, new Set());
    }

    adjacency.get(fromId)?.add(toId);
    reverseAdjacency.get(toId)?.add(fromId);

    if (!relationshipByPair.has(key) || source === "explicit") {
      relationshipByPair.set(key, relationship);
    }

    if (source === "explicit") {
      explicitEdgeKeys.add(key);
    } else if (!explicitEdgeKeys.has(key)) {
      inferredEdgeKeys.add(key);
    }
  };

  edges.forEach((edge) => {
    const normalized = normalizeLineageEdge(edge, reportById);
    addRelationship(
      normalized.fromId,
      normalized.toId,
      edge.relationship,
      "explicit",
    );
  });

  reports.forEach((report) => {
    if (
      report.previous_report_id &&
      reportById.has(report.previous_report_id)
    ) {
      addRelationship(
        report.previous_report_id,
        report.report_id,
        "previous_report",
        "inferred",
      );
    }
  });

  const orderedReports = buildOrderedLineageReports(
    reports,
    adjacency,
    reverseAdjacency,
  );
  const relationshipCounts = Array.from(relationshipByPair.values()).reduce<
    Map<string, number>
  >((accumulator, relationship) => {
    accumulator.set(relationship, (accumulator.get(relationship) ?? 0) + 1);
    return accumulator;
  }, new Map<string, number>());

  const progression = orderedReports.map((report, index) => {
    const upstreamCount = reverseAdjacency.get(report.report_id)?.size ?? 0;
    const downstreamCount = adjacency.get(report.report_id)?.size ?? 0;
    const previousReport = index > 0 ? orderedReports[index - 1] : undefined;
    const primaryUpstreamReportId =
      Array.from(reverseAdjacency.get(report.report_id) ?? [])
        .map((upstreamId) => reportById.get(upstreamId))
        .filter((upstream): upstream is SosValidationReport =>
          Boolean(upstream),
        )
        .sort(compareReportsAscending)[0]?.report_id ?? null;
    const relationshipFromPrevious = previousReport
      ? (relationshipByPair.get(
          getPairKey(previousReport.report_id, report.report_id),
        ) ?? null)
      : null;
    const primaryUpstreamRelationship = primaryUpstreamReportId
      ? (relationshipByPair.get(
          getPairKey(primaryUpstreamReportId, report.report_id),
        ) ?? null)
      : null;

    return {
      report,
      sequence: index + 1,
      upstreamCount,
      downstreamCount,
      relationshipFromPrevious,
      primaryUpstreamReportId,
      primaryUpstreamRelationship,
      isRoot: upstreamCount === 0,
      isHead: downstreamCount === 0,
      isBranchPoint: downstreamCount > 1,
      isMergePoint: upstreamCount > 1,
    };
  });

  return {
    orderedReports,
    progression,
    totalReports: orderedReports.length,
    explicitEdgeCount: explicitEdgeKeys.size,
    inferredEdgeCount: inferredEdgeKeys.size,
    rootCount: progression.filter((entry) => entry.isRoot).length,
    headCount: progression.filter((entry) => entry.isHead).length,
    branchPointCount: progression.filter((entry) => entry.isBranchPoint).length,
    mergePointCount: progression.filter((entry) => entry.isMergePoint).length,
    componentCount: countLineageComponents(
      reports,
      adjacency,
      reverseAdjacency,
    ),
    relationshipCounts: Array.from(relationshipCounts.entries())
      .map(([relationship, count]) => ({ relationship, count }))
      .sort(
        (left, right) =>
          right.count - left.count ||
          left.relationship.localeCompare(right.relationship),
      ),
    baselineReport: orderedReports[0] ?? null,
    latestReport:
      orderedReports.length > 0
        ? orderedReports[orderedReports.length - 1]
        : null,
  };
}

export function sortReportsByValidatedAt(
  reports: SosValidationReport[],
  direction: "asc" | "desc" = "desc",
): SosValidationReport[] {
  const sorted = [...reports].sort((left, right) => {
    const timestampDiff =
      safeTimestamp(left.validated_at) - safeTimestamp(right.validated_at);

    if (timestampDiff !== 0) {
      return timestampDiff;
    }

    return left.report_id.localeCompare(right.report_id);
  });

  return direction === "asc" ? sorted : sorted.reverse();
}

function compareReportsAscending(
  left: SosValidationReport,
  right: SosValidationReport,
): number {
  const timestampDiff =
    safeTimestamp(left.validated_at) - safeTimestamp(right.validated_at);

  if (timestampDiff !== 0) {
    return timestampDiff;
  }

  return left.report_id.localeCompare(right.report_id);
}

function normalizeLineageEdge(
  edge: SosValidationLineageEdge,
  reportById: Map<string, SosValidationReport>,
): { fromId: string; toId: string } {
  const fromReport = reportById.get(edge.from_report_id);
  const toReport = reportById.get(edge.to_report_id);

  if (toReport?.previous_report_id === edge.from_report_id) {
    return {
      fromId: edge.from_report_id,
      toId: edge.to_report_id,
    };
  }

  if (fromReport?.previous_report_id === edge.to_report_id) {
    return {
      fromId: edge.to_report_id,
      toId: edge.from_report_id,
    };
  }

  if (fromReport && toReport) {
    return safeTimestamp(fromReport.validated_at) <=
      safeTimestamp(toReport.validated_at)
      ? { fromId: edge.from_report_id, toId: edge.to_report_id }
      : { fromId: edge.to_report_id, toId: edge.from_report_id };
  }

  return {
    fromId: edge.from_report_id,
    toId: edge.to_report_id,
  };
}

function buildOrderedLineageReports(
  reports: SosValidationReport[],
  adjacency: Map<string, Set<string>>,
  reverseAdjacency: Map<string, Set<string>>,
): SosValidationReport[] {
  const orderedByTime = sortReportsByValidatedAt(reports, "asc");
  const reportById = new Map(
    orderedByTime.map((report) => [report.report_id, report]),
  );
  const inDegree = new Map<string, number>();

  orderedByTime.forEach((report) => {
    inDegree.set(
      report.report_id,
      reverseAdjacency.get(report.report_id)?.size ?? 0,
    );
  });

  const queue = orderedByTime
    .filter((report) => (inDegree.get(report.report_id) ?? 0) === 0)
    .map((report) => report.report_id);
  const visited = new Set<string>();
  const orderedIds: string[] = [];

  while (queue.length > 0) {
    queue.sort((left, right) => {
      const leftReport = reportById.get(left);
      const rightReport = reportById.get(right);
      return (
        safeTimestamp(leftReport?.validated_at) -
        safeTimestamp(rightReport?.validated_at)
      );
    });

    const currentId = queue.shift();
    if (!currentId || visited.has(currentId)) {
      continue;
    }

    visited.add(currentId);
    orderedIds.push(currentId);

    const downstream = Array.from(adjacency.get(currentId) ?? []);
    downstream.forEach((nextId) => {
      const remaining = (inDegree.get(nextId) ?? 0) - 1;
      inDegree.set(nextId, remaining);
      if (remaining <= 0) {
        queue.push(nextId);
      }
    });
  }

  orderedByTime.forEach((report) => {
    if (!visited.has(report.report_id)) {
      orderedIds.push(report.report_id);
    }
  });

  return orderedIds
    .map((reportId) => reportById.get(reportId))
    .filter((report): report is SosValidationReport => Boolean(report));
}

function countLineageComponents(
  reports: SosValidationReport[],
  adjacency: Map<string, Set<string>>,
  reverseAdjacency: Map<string, Set<string>>,
): number {
  const visited = new Set<string>();
  let components = 0;

  for (const report of reports) {
    if (visited.has(report.report_id)) {
      continue;
    }

    components += 1;
    const queue = [report.report_id];

    while (queue.length > 0) {
      const currentId = queue.shift();
      if (!currentId || visited.has(currentId)) {
        continue;
      }

      visited.add(currentId);
      const neighbors = [
        ...Array.from(adjacency.get(currentId) ?? []),
        ...Array.from(reverseAdjacency.get(currentId) ?? []),
      ];

      neighbors.forEach((neighborId) => {
        if (!visited.has(neighborId)) {
          queue.push(neighborId);
        }
      });
    }
  }

  return components;
}

function summarizeRepeatedSignals(counts: Map<string, number>): string[] {
  return Array.from(counts.entries())
    .filter(([, count]) => count > 1)
    .sort(
      (left, right) => right[1] - left[1] || left[0].localeCompare(right[0]),
    )
    .map(([signal, count]) => `${signal} x${count}`);
}

function getHistoryTrend(input: {
  baselineReport: SosValidationReport | null;
  latestReport: SosValidationReport | null;
  netConfidenceDelta: number;
  statusFlipCount: number;
  totalNewFailures: number;
  totalResolvedChecks: number;
  versionChangeCount: number;
}): SosHistoryTrend {
  const {
    baselineReport,
    latestReport,
    netConfidenceDelta,
    statusFlipCount,
    totalNewFailures,
    totalResolvedChecks,
    versionChangeCount,
  } = input;

  if (!baselineReport || !latestReport) {
    return "steady";
  }

  const introducedFailures = totalNewFailures > 0;
  const resolvedSignals = totalResolvedChecks > 0;
  const outcomeChanged = baselineReport.passed !== latestReport.passed;

  if (
    latestReport.passed &&
    (netConfidenceDelta > 0.001 ||
      (!outcomeChanged && resolvedSignals && !introducedFailures))
  ) {
    return introducedFailures && statusFlipCount > 0 ? "mixed" : "improving";
  }

  if (
    !latestReport.passed &&
    (netConfidenceDelta < -0.001 || (!resolvedSignals && introducedFailures))
  ) {
    return resolvedSignals || versionChangeCount > 0 ? "mixed" : "degrading";
  }

  if (
    statusFlipCount > 1 ||
    (introducedFailures && resolvedSignals) ||
    outcomeChanged
  ) {
    return "mixed";
  }

  return "steady";
}

function getReportMovement(report: SosValidationReport): SosReportMovement {
  const {
    confidence_delta: confidenceDelta,
    new_failures: newFailures,
    resolved_checks: resolved,
  } = report.change_summary;
  const hasNewFailures = newFailures.length > 0;
  const hasResolvedChecks = resolved.length > 0;

  if (hasNewFailures && hasResolvedChecks) {
    return "mixed";
  }
  if (hasNewFailures || confidenceDelta < -0.001) {
    return "regressed";
  }
  if (hasResolvedChecks || confidenceDelta > 0.001) {
    return "improved";
  }
  return "steady";
}

function getPairKey(fromId: string, toId: string): string {
  return `${fromId}::${toId}`;
}

function safeTimestamp(value: string | undefined | null): number {
  if (!value) {
    return 0;
  }

  const timestamp = new Date(value).getTime();
  return Number.isFinite(timestamp) ? timestamp : 0;
}
