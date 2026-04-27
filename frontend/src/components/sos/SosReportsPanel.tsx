import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { format } from "date-fns";
import {
  AlertTriangle,
  Clock3,
  FileSearch,
  GitBranch,
  Loader2,
  Search,
} from "lucide-react";

import {
  buildInterfacePairSubjectKey,
  type SosValidationCheck,
  type SosValidationHistoryResponse,
  type SosValidationLineageEdge,
  type SosValidationLineageResponse,
  type SosValidationReport,
} from "@/api/sosValidation";
import {
  deriveHistoryInsights,
  deriveLineageInsights,
  type SosHistoryInsights,
  type SosHistoryTimelineEntry,
  type SosLineageInsights,
  type SosLineageProgressionEntry,
} from "@/components/sos/sosReportInsights";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  useLookupValidationHistory,
  useLookupValidationLineage,
  useLookupValidationReport,
} from "@/hooks/useSosValidation";
import { cn } from "@/lib/utils";

const SUBJECT_TYPE_OPTIONS = [
  { value: "auto", label: "Auto-detect" },
  { value: "interface_pair", label: "Interface Pair" },
  { value: "contract", label: "Contract" },
  { value: "system_pair", label: "System Pair" },
  { value: "interface", label: "Interface" },
  { value: "policy", label: "Policy" },
] as const;

const HISTORY_LIMIT = 25;
const LINEAGE_LIMIT = 25;

interface SosReportsPanelProps {
  currentPair?: {
    providerInterfaceId: string;
    consumerInterfaceId: string;
  } | null;
  latestReportId?: string | null;
  seedSubjectType?: string;
  seedSubjectKey?: string;
}

export function SosReportsPanel({
  currentPair,
  latestReportId,
  seedSubjectType,
  seedSubjectKey,
}: SosReportsPanelProps) {
  const currentPairSubjectKey =
    currentPair?.providerInterfaceId && currentPair?.consumerInterfaceId
      ? buildInterfacePairSubjectKey(
          currentPair.providerInterfaceId,
          currentPair.consumerInterfaceId,
        )
      : "";

  const [subjectType, setSubjectType] = useState<string>(
    currentPairSubjectKey ? "interface_pair" : "auto",
  );
  const [subjectKey, setSubjectKey] = useState(currentPairSubjectKey);
  const [reportId, setReportId] = useState(latestReportId ?? "");
  const [reportResult, setReportResult] = useState<SosValidationReport | null>(
    null,
  );
  const [historyResult, setHistoryResult] =
    useState<SosValidationHistoryResponse | null>(null);
  const [lineageResult, setLineageResult] =
    useState<SosValidationLineageResponse | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [lineageError, setLineageError] = useState<string | null>(null);

  const lookupReport = useLookupValidationReport();
  const lookupHistory = useLookupValidationHistory();
  const lookupLineage = useLookupValidationLineage();
  const lastAutoReportIdRef = useRef<string | null>(null);
  const lastAutoSubjectRef = useRef<string | null>(null);

  const resolvedSubjectType = subjectType === "auto" ? undefined : subjectType;

  const loadReport = useCallback(
    async (nextReportId: string) => {
      setReportError(null);

      try {
        const report = await lookupReport.mutateAsync(nextReportId);
        setReportResult(report);
      } catch (error) {
        setReportError(getErrorMessage(error));
      }
    },
    [lookupReport],
  );

  const loadHistory = useCallback(
    async (nextSubjectKey: string, nextSubjectType?: string) => {
      setHistoryError(null);

      try {
        const history = await lookupHistory.mutateAsync({
          subjectKey: nextSubjectKey,
          subjectType: nextSubjectType,
          limit: HISTORY_LIMIT,
        });
        setHistoryResult(history);
      } catch (error) {
        setHistoryError(getErrorMessage(error));
      }
    },
    [lookupHistory],
  );

  const loadLineage = useCallback(
    async (nextSubjectKey: string, nextSubjectType?: string) => {
      setLineageError(null);

      try {
        const lineage = await lookupLineage.mutateAsync({
          subjectKey: nextSubjectKey,
          subjectType: nextSubjectType,
          limit: LINEAGE_LIMIT,
        });
        setLineageResult(lineage);
      } catch (error) {
        setLineageError(getErrorMessage(error));
      }
    },
    [lookupLineage],
  );

  useEffect(() => {
    if (latestReportId) {
      setReportId(latestReportId);
    }
  }, [latestReportId]);

  useEffect(() => {
    if (!latestReportId || lastAutoReportIdRef.current === latestReportId) {
      return;
    }

    lastAutoReportIdRef.current = latestReportId;
    setReportId(latestReportId);
    void loadReport(latestReportId);
  }, [latestReportId, loadReport]);

  useEffect(() => {
    if (!currentPairSubjectKey) {
      return;
    }

    setSubjectKey((current) => (current ? current : currentPairSubjectKey));
    setSubjectType((current) =>
      current === "auto" ? "interface_pair" : current,
    );
  }, [currentPairSubjectKey]);

  useEffect(() => {
    if (!seedSubjectKey) {
      return;
    }

    const nextSubjectType =
      seedSubjectType && seedSubjectType !== "auto"
        ? seedSubjectType
        : undefined;
    const subjectSignature = `${nextSubjectType ?? "auto"}::${seedSubjectKey}`;
    if (lastAutoSubjectRef.current === subjectSignature) {
      return;
    }

    lastAutoSubjectRef.current = subjectSignature;
    setSubjectKey(seedSubjectKey);
    setSubjectType(nextSubjectType ?? "auto");
    void loadHistory(seedSubjectKey, nextSubjectType);
    void loadLineage(seedSubjectKey, nextSubjectType);
  }, [seedSubjectKey, seedSubjectType, loadHistory, loadLineage]);

  const historyReports = historyResult?.reports ?? [];
  const lineageReports = lineageResult?.reports ?? [];
  const lineageEdges = lineageResult?.edges ?? [];

  const historyInsights = useMemo(
    () => deriveHistoryInsights(historyReports),
    [historyReports],
  );
  const lineageInsights = useMemo(
    () => deriveLineageInsights(lineageReports, lineageEdges),
    [lineageReports, lineageEdges],
  );

  const handleLoadReport = async () => {
    if (!reportId.trim()) {
      setReportError("Enter a persisted report id first.");
      return;
    }

    await loadReport(reportId.trim());
  };

  const handleLoadHistory = async () => {
    if (!subjectKey.trim()) {
      setHistoryError("Enter a normalized subject key first.");
      return;
    }

    await loadHistory(subjectKey.trim(), resolvedSubjectType);
  };

  const handleLoadLineage = async () => {
    if (!subjectKey.trim()) {
      setLineageError("Enter a normalized subject key first.");
      return;
    }

    await loadLineage(subjectKey.trim(), resolvedSubjectType);
  };

  const anyRequestPending =
    lookupReport.isPending ||
    lookupHistory.isPending ||
    lookupLineage.isPending;

  return (
    <div className="space-y-4">
      <div className="grid gap-4 xl:grid-cols-[420px,minmax(0,1fr)]">
        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <FileSearch className="h-4 w-4" />
                Persisted Report Lookup
              </CardTitle>
              <CardDescription>
                Load the exact persisted report payload returned by the
                coordinator after validation.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="sos-report-id">Report Id</Label>
                <Input
                  id="sos-report-id"
                  value={reportId}
                  onChange={(event) => setReportId(event.target.value)}
                  placeholder="report-id"
                  autoComplete="off"
                />
              </div>

              <div className="flex flex-wrap gap-2">
                <Button
                  onClick={handleLoadReport}
                  disabled={lookupReport.isPending}
                >
                  {lookupReport.isPending ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Search className="mr-2 h-4 w-4" />
                  )}
                  Load Report
                </Button>
                {latestReportId && (
                  <Button
                    variant="outline"
                    onClick={() => setReportId(latestReportId)}
                    disabled={lookupReport.isPending}
                  >
                    Use Latest Validation
                  </Button>
                )}
              </div>

              {latestReportId && (
                <div className="rounded-sm border border-border bg-background-secondary p-3 text-xs text-muted-foreground">
                  Latest report id from this session:{" "}
                  <span className="font-mono">{latestReportId}</span>
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <GitBranch className="h-4 w-4" />
                History And Lineage Subject
              </CardTitle>
              <CardDescription>
                Query persisted history using the normalized SoS validation
                subject key.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="sos-subject-type">Subject Type</Label>
                <Select value={subjectType} onValueChange={setSubjectType}>
                  <SelectTrigger id="sos-subject-type">
                    <SelectValue placeholder="Choose a subject type" />
                  </SelectTrigger>
                  <SelectContent>
                    {SUBJECT_TYPE_OPTIONS.map((option) => (
                      <SelectItem key={option.value} value={option.value}>
                        {option.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="sos-subject-key">Subject Key</Label>
                <Input
                  id="sos-subject-key"
                  value={subjectKey}
                  onChange={(event) => setSubjectKey(event.target.value)}
                  placeholder="interface_pair:provider-id:consumer-id"
                  autoComplete="off"
                />
                <p className="text-xs text-muted-foreground">
                  For interface compatibility, the canonical shape is{" "}
                  <span className="font-mono">
                    interface_pair:&lt;provider&gt;:&lt;consumer&gt;
                  </span>
                  .
                </p>
              </div>

              <div className="flex flex-wrap gap-2">
                <Button
                  onClick={handleLoadHistory}
                  disabled={anyRequestPending}
                >
                  {lookupHistory.isPending ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Clock3 className="mr-2 h-4 w-4" />
                  )}
                  Load History
                </Button>
                <Button
                  variant="outline"
                  onClick={handleLoadLineage}
                  disabled={anyRequestPending}
                >
                  {lookupLineage.isPending ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <GitBranch className="mr-2 h-4 w-4" />
                  )}
                  Load Lineage
                </Button>
                {currentPairSubjectKey && (
                  <Button
                    variant="outline"
                    onClick={() => {
                      setSubjectType("interface_pair");
                      setSubjectKey(currentPairSubjectKey);
                    }}
                    disabled={anyRequestPending}
                  >
                    Use Current Pair
                  </Button>
                )}
              </div>

              {currentPairSubjectKey && (
                <div className="rounded-sm border border-border bg-background-secondary p-3 text-xs text-muted-foreground">
                  Current workbench pair resolves to{" "}
                  <span className="font-mono">{currentPairSubjectKey}</span>
                </div>
              )}
            </CardContent>
          </Card>
        </div>

        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Persisted Report</CardTitle>
              <CardDescription>
                Full persisted report details, including change summary and
                references.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {lookupReport.isPending ? (
                <LoadingState label="Loading persisted validation report..." />
              ) : reportError ? (
                <InlineError message={reportError} />
              ) : reportResult ? (
                <ValidationReportDetails report={reportResult} />
              ) : (
                <EmptyState
                  title="No report loaded"
                  description="Use a report id from a recent validation result to inspect the persisted report payload."
                />
              )}
            </CardContent>
          </Card>

          <div className="grid gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Validation History</CardTitle>
                <CardDescription>
                  Trend-focused scan of the loaded report window, reordered
                  oldest to newest.
                </CardDescription>
              </CardHeader>
              <CardContent>
                {lookupHistory.isPending ? (
                  <LoadingState label="Loading validation history..." />
                ) : historyError ? (
                  <InlineError message={historyError} />
                ) : historyResult ? (
                  historyInsights.totalReports === 0 ? (
                    <EmptyState
                      title="No reports found"
                      description="This subject does not have any persisted validation history yet."
                    />
                  ) : (
                    <HistoryInsightsPanel insights={historyInsights} />
                  )
                ) : (
                  <EmptyState
                    title="No history loaded"
                    description="Load history for a subject key to see how validation outcomes changed over time."
                  />
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Validation Lineage</CardTitle>
                <CardDescription>
                  Thin progression view over the persisted report chain for the
                  selected subject.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                {lookupLineage.isPending ? (
                  <LoadingState label="Loading validation lineage..." />
                ) : lineageError ? (
                  <InlineError message={lineageError} />
                ) : lineageResult ? (
                  lineageInsights.totalReports === 0 ? (
                    <EmptyState
                      title="No lineage found"
                      description="No persisted lineage is available yet for this subject."
                    />
                  ) : (
                    <LineageInsightsPanel
                      insights={lineageInsights}
                      edges={lineageEdges}
                    />
                  )
                ) : (
                  <EmptyState
                    title="No lineage loaded"
                    description="Load lineage for a subject key to inspect the persisted report chain."
                  />
                )}
              </CardContent>
            </Card>
          </div>
        </div>
      </div>
    </div>
  );
}

function ValidationReportDetails({ report }: { report: SosValidationReport }) {
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={report.passed ? "success" : "destructive"}>
              {report.passed ? "Passed" : "Failed"}
            </Badge>
            <Badge variant="outline">{report.subject_type}</Badge>
            <Badge variant="outline">{report.validation_type}</Badge>
          </div>
          <div className="text-xs text-muted-foreground">
            Report id: {report.report_id}
          </div>
        </div>
        <div className="text-xs text-muted-foreground">
          {formatTimestamp(report.validated_at)}
        </div>
      </div>

      <div className="rounded-sm border border-border bg-background-secondary p-3 text-sm text-muted-foreground">
        {buildChangeNarrative(report)}
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <InfoBlock label="Subject Key" value={report.subject_key} mono />
        <InfoBlock
          label="Previous Report"
          value={report.previous_report_id ?? "None"}
          mono
        />
        <InfoBlock
          label="Confidence"
          value={formatPercent(report.confidence)}
        />
        <InfoBlock label="Validation Id" value={report.validation_id} mono />
      </div>

      <div className="space-y-2">
        <SectionEyebrow label="Change Summary" />
        <div className="grid gap-3 md:grid-cols-2">
          <InfoBlock
            label="Resolved Checks"
            value={formatList(report.change_summary.resolved_checks)}
          />
          <InfoBlock
            label="New Failures"
            value={formatList(report.change_summary.new_failures)}
          />
          <InfoBlock
            label="Confidence Delta"
            value={formatSignedConfidenceDelta(
              report.change_summary.confidence_delta,
            )}
          />
          <InfoBlock
            label="Schema / Policy Changed"
            value={
              report.change_summary.schema_or_policy_version_changed
                ? "Yes"
                : "No"
            }
          />
        </div>
      </div>

      <ValidationChecksList checks={report.checks} />

      <div className="grid gap-3 md:grid-cols-3">
        <InfoBlock
          label="Ontology Refs"
          value={formatList(report.ontology_refs)}
        />
        <InfoBlock label="Shape Refs" value={formatList(report.shape_refs)} />
        <InfoBlock label="Policy Refs" value={formatList(report.policy_refs)} />
      </div>

      {Object.keys(report.schema_hashes).length > 0 && (
        <div className="space-y-2">
          <SectionEyebrow label="Schema Hashes" />
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Artifact</TableHead>
                <TableHead>Hash</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {Object.entries(report.schema_hashes).map(([artifact, hash]) => (
                <TableRow key={artifact}>
                  <TableCell>{artifact}</TableCell>
                  <TableCell className="font-mono text-xs">{hash}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}

function HistoryInsightsPanel({ insights }: { insights: SosHistoryInsights }) {
  const windowLabel =
    insights.baselineReport && insights.latestReport
      ? `${formatShortTimestamp(insights.baselineReport.validated_at)} -> ${formatShortTimestamp(
          insights.latestReport.validated_at,
        )}`
      : "Single report window";

  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-2">
        <MetricBlock
          label="Loaded Reports"
          value={String(insights.totalReports)}
          caption={windowLabel}
        />
        <MetricBlock
          label="Pass Rate"
          value={formatPercent(insights.passRate)}
          caption={`${insights.passCount} pass / ${insights.failureCount} fail`}
        />
        <MetricBlock
          label="Net Confidence"
          value={formatSignedConfidenceDelta(insights.netConfidenceDelta)}
          caption={`Average ${formatPercent(insights.averageConfidence)}`}
          tone={
            insights.netConfidenceDelta > 0
              ? "positive"
              : insights.netConfidenceDelta < 0
                ? "negative"
                : "neutral"
          }
        />
        <MetricBlock
          label="Status Flips"
          value={String(insights.statusFlipCount)}
          caption={`${insights.versionChangeCount} schema / policy shifts`}
        />
      </div>

      <div className="rounded-sm border border-border bg-background-secondary p-3">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={historyTrendVariant(insights.trend)}>
            {historyTrendLabel(insights.trend)}
          </Badge>
          <span className="text-sm text-muted-foreground">
            {buildHistoryNarrative(insights)}
          </span>
        </div>
        {(insights.repeatedNewFailures.length > 0 ||
          insights.repeatedResolvedChecks.length > 0) && (
          <div className="mt-3 flex flex-wrap gap-2 text-xs">
            {insights.repeatedNewFailures.map((signal) => (
              <Badge key={`repeat-failure-${signal}`} variant="warning">
                Repeat failure: {signal}
              </Badge>
            ))}
            {insights.repeatedResolvedChecks.map((signal) => (
              <Badge key={`repeat-resolved-${signal}`} variant="success">
                Repeat resolution: {signal}
              </Badge>
            ))}
          </div>
        )}
      </div>

      <div className="space-y-3">
        <div>
          <SectionEyebrow label="Change Timeline" />
          <p className="mt-1 text-xs text-muted-foreground">
            Oldest to newest so each delta reads as a progression instead of a
            stack of isolated reports.
          </p>
        </div>
        <div className="space-y-3">
          {insights.timeline.map((entry) => (
            <HistoryTimelineItem key={entry.report.report_id} entry={entry} />
          ))}
        </div>
      </div>
    </div>
  );
}

function LineageInsightsPanel({
  insights,
  edges,
}: {
  insights: SosLineageInsights;
  edges: SosValidationLineageEdge[];
}) {
  const primaryRelationship = insights.relationshipCounts[0];
  const spanLabel =
    insights.baselineReport && insights.latestReport
      ? `${formatShortTimestamp(insights.baselineReport.validated_at)} -> ${formatShortTimestamp(
          insights.latestReport.validated_at,
        )}`
      : "Single report chain";

  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-2">
        <MetricBlock
          label="Reports"
          value={String(insights.totalReports)}
          caption={spanLabel}
        />
        <MetricBlock
          label="Roots / Heads"
          value={`${insights.rootCount} / ${insights.headCount}`}
          caption={`${insights.componentCount} connected lane${insights.componentCount === 1 ? "" : "s"}`}
        />
        <MetricBlock
          label="Branch / Merge"
          value={`${insights.branchPointCount} / ${insights.mergePointCount}`}
          caption="Multiple downstream or upstream transitions"
        />
        <MetricBlock
          label="Explicit / Inferred Links"
          value={`${insights.explicitEdgeCount} / ${insights.inferredEdgeCount}`}
          caption={
            primaryRelationship
              ? `Primary relationship: ${humanizeRelationship(primaryRelationship.relationship)} x${primaryRelationship.count}`
              : "No explicit lineage relationships returned"
          }
        />
      </div>

      <div className="rounded-sm border border-border bg-background-secondary p-3 text-sm text-muted-foreground">
        Ordered using explicit lineage edges first, then{" "}
        <span className="font-mono">previous_report_id</span> and timestamps
        when the payload is sparse. That keeps lineage progression readable
        without inventing a second surface.
      </div>

      <div className="space-y-3">
        <div>
          <SectionEyebrow label="Lineage Progression" />
          <p className="mt-1 text-xs text-muted-foreground">
            The loaded chain is shown as an operator-friendly progression from
            origin to current head.
          </p>
        </div>
        <div className="space-y-0">
          {insights.progression.map((entry, index) => (
            <LineageProgressionItem
              key={entry.report.report_id}
              entry={entry}
              isLast={index === insights.progression.length - 1}
            />
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <SectionEyebrow label="Lineage Edges" />
        {edges.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No explicit lineage edges were returned. The progression above was
            inferred from the report chain metadata.
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>From</TableHead>
                <TableHead>To</TableHead>
                <TableHead>Relationship</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {edges.map((edge) => (
                <TableRow
                  key={`${edge.from_report_id}:${edge.to_report_id}:${edge.relationship}`}
                >
                  <TableCell className="font-mono text-xs">
                    {edge.from_report_id}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {edge.to_report_id}
                  </TableCell>
                  <TableCell>
                    {humanizeRelationship(edge.relationship)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>
    </div>
  );
}

function HistoryTimelineItem({ entry }: { entry: SosHistoryTimelineEntry }) {
  return (
    <div className="rounded-sm border border-border bg-background p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={entry.report.passed ? "success" : "destructive"}>
              {entry.report.passed ? "Passed" : "Failed"}
            </Badge>
            <Badge variant={movementVariant(entry.movement)}>
              {movementLabel(entry.movement)}
            </Badge>
            {entry.isBaseline ? (
              <Badge variant="outline">Baseline</Badge>
            ) : null}
            {entry.isLatest ? <Badge variant="secondary">Latest</Badge> : null}
          </div>
          <div className="font-mono text-xs text-foreground">
            {entry.report.report_id}
          </div>
          <div className="text-xs text-muted-foreground">
            {entry.report.validation_type}
          </div>
        </div>
        <div className="text-right text-xs text-muted-foreground">
          <div>{formatTimestamp(entry.report.validated_at)}</div>
          <div>Confidence {formatPercent(entry.report.confidence)}</div>
        </div>
      </div>

      <div className="mt-3 flex flex-wrap gap-2">
        <ChangeBadge
          label="Confidence"
          value={formatSignedConfidenceDelta(entry.confidenceDelta)}
          tone={
            entry.confidenceDelta > 0
              ? "positive"
              : entry.confidenceDelta < 0
                ? "negative"
                : "neutral"
          }
        />
        <ChangeBadge
          label="New Failures"
          value={String(entry.newFailureCount)}
          tone={entry.newFailureCount > 0 ? "negative" : "neutral"}
        />
        <ChangeBadge
          label="Resolved"
          value={String(entry.resolvedCount)}
          tone={entry.resolvedCount > 0 ? "positive" : "neutral"}
        />
        {entry.versionChanged ? (
          <ChangeBadge label="Revision" value="Changed" tone="warning" />
        ) : null}
      </div>

      {(entry.report.change_summary.new_failures.length > 0 ||
        entry.report.change_summary.resolved_checks.length > 0) && (
        <div className="mt-3 grid gap-2 md:grid-cols-2">
          {entry.report.change_summary.new_failures.length > 0 ? (
            <SignalStrip
              label="New failures"
              values={entry.report.change_summary.new_failures}
              tone="negative"
            />
          ) : null}
          {entry.report.change_summary.resolved_checks.length > 0 ? (
            <SignalStrip
              label="Resolved checks"
              values={entry.report.change_summary.resolved_checks}
              tone="positive"
            />
          ) : null}
        </div>
      )}
    </div>
  );
}

function LineageProgressionItem({
  entry,
  isLast,
}: {
  entry: SosLineageProgressionEntry;
  isLast: boolean;
}) {
  return (
    <div className="flex gap-3">
      <div className="flex w-4 flex-col items-center">
        <span
          className={cn(
            "mt-3 h-2.5 w-2.5 rounded-full",
            entry.report.passed ? "bg-success" : "bg-destructive",
          )}
        />
        {!isLast ? <span className="mt-2 h-full w-px bg-border" /> : null}
      </div>

      <div className="flex-1 pb-3">
        <div className="rounded-sm border border-border bg-background p-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <Badge
                  variant={entry.report.passed ? "success" : "destructive"}
                >
                  {entry.report.passed ? "Passed" : "Failed"}
                </Badge>
                {entry.isRoot ? <Badge variant="outline">Origin</Badge> : null}
                {entry.isHead ? (
                  <Badge variant="secondary">Current Head</Badge>
                ) : null}
                {entry.isBranchPoint ? (
                  <Badge variant="warning">Branch</Badge>
                ) : null}
                {entry.isMergePoint ? (
                  <Badge variant="warning">Merge</Badge>
                ) : null}
              </div>
              <div className="font-mono text-xs text-foreground">
                {entry.report.report_id}
              </div>
              {entry.primaryUpstreamRelationship ? (
                <div className="text-xs text-muted-foreground">
                  Upstream link:{" "}
                  {humanizeRelationship(entry.primaryUpstreamRelationship)}
                  {entry.primaryUpstreamReportId
                    ? ` from ${entry.primaryUpstreamReportId}`
                    : ""}
                </div>
              ) : null}
            </div>
            <div className="text-right text-xs text-muted-foreground">
              <div>{formatTimestamp(entry.report.validated_at)}</div>
              <div>Confidence {formatPercent(entry.report.confidence)}</div>
            </div>
          </div>

          <div className="mt-3 flex flex-wrap gap-2">
            <ChangeBadge label="Step" value={String(entry.sequence)} />
            <ChangeBadge label="Upstream" value={String(entry.upstreamCount)} />
            <ChangeBadge
              label="Downstream"
              value={String(entry.downstreamCount)}
            />
            <ChangeBadge
              label="Delta"
              value={formatSignedConfidenceDelta(
                entry.report.change_summary.confidence_delta,
              )}
              tone={
                entry.report.change_summary.confidence_delta > 0
                  ? "positive"
                  : entry.report.change_summary.confidence_delta < 0
                    ? "negative"
                    : "neutral"
              }
            />
          </div>

          <p className="mt-3 text-sm text-muted-foreground">
            {buildChangeNarrative(entry.report)}
          </p>
        </div>
      </div>
    </div>
  );
}

function ValidationChecksList({ checks }: { checks: SosValidationCheck[] }) {
  if (checks.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No detailed validation checks were returned.
      </p>
    );
  }

  return (
    <div className="space-y-3">
      <SectionEyebrow label="Checks" />
      {checks.map((check) => (
        <div
          key={`${check.check_name}:${check.severity}`}
          className="rounded-sm border border-border bg-background-secondary p-3"
        >
          <div className="flex flex-wrap items-start justify-between gap-2">
            <div className="space-y-1">
              <div className="font-medium text-foreground">
                {check.check_name}
              </div>
              <div className="text-sm text-muted-foreground">
                {check.description}
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge variant={check.passed ? "success" : "destructive"}>
                {check.passed ? "Pass" : "Fail"}
              </Badge>
              <Badge variant={severityVariant(check.severity)}>
                {check.severity}
              </Badge>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function MetricBlock({
  label,
  value,
  caption,
  tone = "default",
}: {
  label: string;
  value: string;
  caption?: string;
  tone?: "default" | "positive" | "negative" | "neutral";
}) {
  return (
    <div className="rounded-sm border border-border bg-background-secondary p-3">
      <div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div
        className={cn("mt-1 text-xl font-semibold", {
          "text-foreground": tone === "default" || tone === "neutral",
          "text-success": tone === "positive",
          "text-destructive": tone === "negative",
        })}
      >
        {value}
      </div>
      {caption ? (
        <div className="mt-1 text-xs text-muted-foreground">{caption}</div>
      ) : null}
    </div>
  );
}

function ChangeBadge({
  label,
  value,
  tone = "default",
}: {
  label: string;
  value: string;
  tone?: "default" | "positive" | "negative" | "neutral" | "warning";
}) {
  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 rounded-sm border px-2 py-1 text-xs",
        tone === "positive" && "border-success/30 bg-success/10 text-success",
        tone === "negative" &&
          "border-destructive/30 bg-destructive/10 text-destructive",
        tone === "warning" && "border-warning/30 bg-warning/10 text-warning",
        (tone === "default" || tone === "neutral") &&
          "border-border bg-background text-muted-foreground",
      )}
    >
      <span className="font-semibold uppercase tracking-wide">{label}</span>
      <span>{value}</span>
    </div>
  );
}

function SignalStrip({
  label,
  values,
  tone,
}: {
  label: string;
  values: string[];
  tone: "positive" | "negative";
}) {
  return (
    <div
      className={cn(
        "rounded-sm border p-3 text-sm",
        tone === "positive"
          ? "border-success/30 bg-success/10 text-success"
          : "border-destructive/30 bg-destructive/10 text-destructive",
      )}
    >
      <div className="text-xs font-semibold uppercase tracking-wide">
        {label}
      </div>
      <div className="mt-1 text-sm">{formatSignalPreview(values)}</div>
    </div>
  );
}

function SectionEyebrow({ label }: { label: string }) {
  return (
    <div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
      {label}
    </div>
  );
}

function InfoBlock({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="rounded-sm border border-border bg-background-secondary p-3">
      <div className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div
        className={
          mono
            ? "mt-1 break-all font-mono text-sm text-foreground"
            : "mt-1 text-sm text-foreground"
        }
      >
        {value}
      </div>
    </div>
  );
}

function LoadingState({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 text-sm text-muted-foreground">
      <Loader2 className="h-4 w-4 animate-spin" />
      <span>{label}</span>
    </div>
  );
}

function InlineError({ message }: { message: string }) {
  return (
    <Alert variant="destructive">
      <AlertTriangle className="h-4 w-4" />
      <AlertTitle>Request failed</AlertTitle>
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  );
}

function EmptyState({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="rounded-sm border border-dashed border-border p-6 text-center">
      <p className="font-medium text-foreground">{title}</p>
      <p className="mt-2 text-sm text-muted-foreground">{description}</p>
    </div>
  );
}

function buildHistoryNarrative(insights: SosHistoryInsights): string {
  if (!insights.latestReport) {
    return "No loaded reports to summarize.";
  }

  const parts = [
    `${capitalize(historyTrendLabel(insights.trend))} across ${insights.totalReports} loaded reports.`,
    `Latest run ${insights.latestReport.passed ? "passed" : "failed"} at ${formatPercent(insights.latestReport.confidence)}.`,
  ];

  if (insights.totalReports > 1) {
    parts.push(
      `${formatSignedConfidenceDelta(insights.netConfidenceDelta)} versus the earliest loaded report.`,
    );
  }

  parts.push(
    `${insights.totalResolvedChecks} resolved signal${insights.totalResolvedChecks === 1 ? "" : "s"}, ${insights.totalNewFailures} new failure signal${insights.totalNewFailures === 1 ? "" : "s"}.`,
  );

  if (insights.statusFlipCount > 0 || insights.versionChangeCount > 0) {
    parts.push(
      `${insights.statusFlipCount} status flip${insights.statusFlipCount === 1 ? "" : "s"} and ${insights.versionChangeCount} schema / policy shift${insights.versionChangeCount === 1 ? "" : "s"} in the current window.`,
    );
  }

  return parts.join(" ");
}

function buildChangeNarrative(report: SosValidationReport): string {
  const changeParts: string[] = [];
  const newFailures = report.change_summary.new_failures;
  const resolvedChecks = report.change_summary.resolved_checks;

  if (newFailures.length > 0) {
    changeParts.push(
      `introduced ${newFailures.length} new failure${newFailures.length === 1 ? "" : "s"}`,
    );
  }

  if (resolvedChecks.length > 0) {
    changeParts.push(
      `resolved ${resolvedChecks.length} check${resolvedChecks.length === 1 ? "" : "s"}`,
    );
  }

  if (report.change_summary.schema_or_policy_version_changed) {
    changeParts.push("detected a schema or policy revision change");
  }

  if (changeParts.length === 0) {
    changeParts.push(
      "did not materially change any persisted validation signals",
    );
  }

  return `Confidence moved ${formatSignedConfidenceDelta(report.change_summary.confidence_delta)} and this report ${changeParts.join(", ")}.`;
}

function formatTimestamp(value: string | undefined | null): string {
  if (!value) {
    return "Unknown";
  }

  return format(new Date(value), "PPpp");
}

function formatShortTimestamp(value: string | undefined | null): string {
  if (!value) {
    return "Unknown";
  }

  return format(new Date(value), "PP p");
}

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function formatSignedConfidenceDelta(value: number): string {
  const points = Math.round(value * 100);
  if (points === 0) {
    return "0 pts";
  }

  return `${points > 0 ? "+" : ""}${points} pts`;
}

function formatList(values: string[]): string {
  return values.length > 0 ? values.join(", ") : "None";
}

function formatSignalPreview(values: string[]): string {
  if (values.length <= 3) {
    return values.join(", ");
  }

  return `${values.slice(0, 3).join(", ")} +${values.length - 3} more`;
}

function historyTrendLabel(
  trend: "improving" | "degrading" | "mixed" | "steady",
): string {
  switch (trend) {
    case "improving":
      return "Improving";
    case "degrading":
      return "Degrading";
    case "mixed":
      return "Mixed";
    default:
      return "Steady";
  }
}

function historyTrendVariant(
  trend: "improving" | "degrading" | "mixed" | "steady",
): "success" | "destructive" | "warning" | "outline" {
  switch (trend) {
    case "improving":
      return "success";
    case "degrading":
      return "destructive";
    case "mixed":
      return "warning";
    default:
      return "outline";
  }
}

function movementLabel(
  movement: "improved" | "regressed" | "mixed" | "steady",
): string {
  switch (movement) {
    case "improved":
      return "Improved";
    case "regressed":
      return "Regressed";
    case "mixed":
      return "Mixed";
    default:
      return "Steady";
  }
}

function movementVariant(
  movement: "improved" | "regressed" | "mixed" | "steady",
): "success" | "destructive" | "warning" | "outline" {
  switch (movement) {
    case "improved":
      return "success";
    case "regressed":
      return "destructive";
    case "mixed":
      return "warning";
    default:
      return "outline";
  }
}

function severityVariant(
  severity: string,
): "default" | "success" | "warning" | "destructive" | "outline" {
  if (severity === "error" || severity === "critical" || severity === "high") {
    return "destructive";
  }
  if (severity === "warning" || severity === "medium") {
    return "warning";
  }
  if (severity === "info" || severity === "low") {
    return "outline";
  }
  return "default";
}

function humanizeRelationship(value: string): string {
  if (!value) {
    return "Unknown";
  }

  return value.replace(/_/g, " ");
}

function capitalize(value: string): string {
  return value.length > 0
    ? `${value[0].toUpperCase()}${value.slice(1)}`
    : value;
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  const apiError = error as {
    message?: string;
    response?: {
      data?: {
        message?: string;
        error?: string;
      };
    };
  };

  return (
    apiError.response?.data?.message ||
    apiError.response?.data?.error ||
    apiError.message ||
    "Request failed"
  );
}
