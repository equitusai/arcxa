import React, { useEffect, useMemo, useState } from 'react';
import { format } from 'date-fns';
import {
  Activity,
  BadgeCheck,
  Database,
  FileCheck2,
  Fingerprint,
  Loader2,
  PlayCircle,
  RotateCw,
  Search,
  ServerCog,
  ShieldCheck,
} from 'lucide-react';
import { useSearchParams } from 'react-router-dom';
import { toast } from 'sonner';

import type {
  ApprovalEvent,
  ControlResult,
  EvidencePacket,
  ExceptionRecord,
  ExplainValueResponse,
  RunMigrationConnectorResponse,
  UpsertMigrationConnectorResponse,
  ValueExplanation,
} from '@/api/migrationEvidence';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import {
  useExplainMigrationValue,
  useLookupMigrationEvidencePacket,
  useLookupMigrationObjectControls,
  useLookupMigrationProgramApprovals,
  useLookupMigrationProgramExceptions,
  useMigrationRuntimeStatus,
  useRebuildMigrationReadModels,
  useRunMigrationConnector,
  useUpsertMigrationConnector,
} from '@/hooks/useMigrationEvidence';

const CONNECTOR_TEMPLATE = JSON.stringify(
  {
    connector_id: 'ibm-artifacts',
    name: 'IBM Rapid Move Artifact Ingestion',
    vendor: 'ibm_rapid_move',
    role: 'migration_artifact_source',
    transport: 'http_json',
    program_id: 'program-rise-1',
    endpoint: {
      base_url: 'https://ibm.example.test',
      path: '/artifacts',
      method: 'POST',
      headers: {},
    },
    auth: { kind: 'none' },
    enabled: true,
    metadata: {
      engagement_type: 'rise_migration',
      system_of_record: 'ibm_rapid_move',
    },
    created_at: '2026-04-30T00:00:00Z',
    updated_at: '2026-04-30T00:00:00Z',
  },
  null,
  2
);

const RUN_TEMPLATE = JSON.stringify(
  {
    run_label: 'ibm-rise-wave-1',
    manual_events: [
      {
        event_id: 'event-program',
        connector_id: 'ibm-artifacts',
        run_id: 'manual-run',
        vendor: 'ibm_rapid_move',
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        artifact_type: 'program',
        captured_at: '2026-04-30T00:00:00Z',
        payload: {
          program_id: 'program-rise-1',
          name: 'RISE Wave 1',
          customer_name: 'Contoso Manufacturing',
          source_landscape: 'SAP ECC',
          target_landscape: 'SAP S/4HANA',
          tags: ['ibm-rise', 'hana'],
          metadata: {},
          created_at: '2026-04-30T00:00:00Z',
          updated_at: '2026-04-30T00:00:00Z',
        },
      },
      {
        event_id: 'event-object',
        connector_id: 'ibm-artifacts',
        run_id: 'manual-run',
        vendor: 'ibm_rapid_move',
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        artifact_type: 'object',
        captured_at: '2026-04-30T00:00:00Z',
        payload: {
          object_id: 'object-sales-order',
          program_id: 'program-rise-1',
          object_type: 'business_object',
          name: 'SalesOrder',
          description: 'Migrated sales order total',
          source_record_id: 'SO-1',
          target_record_id: 'SO-1',
          tags: ['critical-finance'],
          metadata: {
            business_owner: 'order-to-cash',
          },
        },
      },
      {
        event_id: 'event-rule',
        connector_id: 'ibm-artifacts',
        run_id: 'manual-run',
        vendor: 'ibm_rapid_move',
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        artifact_type: 'transformation_rule',
        value_key: 'SO-1::$.amount',
        captured_at: '2026-04-30T00:00:00Z',
        payload: {
          rule_id: 'rule-net-amount',
          rule_type: 'mapping',
          name: 'Normalize net amount',
          description: 'Map ECC net value into the S/4HANA sales-order API',
          source_fields: [
            {
              system: 'SAP ECC',
              object_name: 'VBAK',
              field_name: 'NETWR',
              field_path: '$.amount',
              semantic_type: 'currency_amount',
              record_id: 'SO-1',
            },
          ],
          target_fields: [
            {
              system: 'SAP S/4HANA',
              object_name: 'A_SalesOrder',
              field_name: 'NetAmount',
              field_path: '$.amount',
              semantic_type: 'currency_amount',
              record_id: 'SO-1',
            },
          ],
          expression: 'NETWR * 1.0',
          metadata: {
            tool: 'IBM Rapid Move',
          },
        },
      },
      {
        event_id: 'event-execution',
        connector_id: 'ibm-artifacts',
        run_id: 'manual-run',
        vendor: 'ibm_rapid_move',
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        artifact_type: 'execution_event',
        value_key: 'SO-1::$.amount',
        captured_at: '2026-04-30T00:00:00Z',
        payload: {
          execution_id: 'exec-migrate-1',
          program_id: 'program-rise-1',
          object_id: 'object-sales-order',
          connector_run_id: 'ibm-run-1',
          tool_name: 'ibm_rapid_move',
          tool_run_id: 'rm-run-1',
          stage: 'load',
          status: 'succeeded',
          happened_at: '2026-04-30T00:00:00Z',
          source_snapshot_ref: 'ecc://wave1/vbak/SO-1',
          target_snapshot_ref: 's4://wave1/a_salesorder/SO-1',
          records_examined: 1,
          records_affected: 1,
          metadata: {},
        },
      },
      {
        event_id: 'event-exception',
        connector_id: 'ibm-artifacts',
        run_id: 'manual-run',
        vendor: 'ibm_rapid_move',
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        artifact_type: 'exception_record',
        value_key: 'SO-1::$.amount',
        captured_at: '2026-04-30T00:00:00Z',
        payload: {
          exception_id: 'exception-1',
          program_id: 'program-rise-1',
          object_id: 'object-sales-order',
          severity: 'warning',
          status: 'accepted',
          category: 'manual_adjustment',
          message: 'Cutover team accepted a minor rounding difference during dress rehearsal',
          source_value: 100,
          target_value: 101,
          remediation: 'Documented for sign-off packet',
          detected_at: '2026-04-30T00:00:00Z',
          resolved_at: '2026-04-30T00:05:00Z',
          metadata: {},
        },
      },
      {
        event_id: 'event-approval',
        connector_id: 'ibm-artifacts',
        run_id: 'manual-run',
        vendor: 'ibm_rapid_move',
        program_id: 'program-rise-1',
        object_id: 'object-sales-order',
        artifact_type: 'approval_event',
        captured_at: '2026-04-30T00:00:00Z',
        payload: {
          approval_id: 'approval-1',
          program_id: 'program-rise-1',
          object_id: 'object-sales-order',
          approver_role: 'data_owner',
          approver_id: 'owner-42',
          status: 'approved',
          comment: 'Approved after dress rehearsal evidence review',
          approved_at: '2026-04-30T00:06:00Z',
          evidence_refs: ['urn:ibm:rapid-move:evidence:packet-1'],
          attestation_ref: 'urn:arcxa:approval:attestation:1',
          metadata: {},
        },
      },
    ],
    verification: null,
    request_body: null,
    request_headers: {},
  },
  null,
  2
);

type MigrationEvidenceTab = 'explain' | 'audit' | 'connectors';

export function MigrationEvidence() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [activeTab, setActiveTab] = useState<MigrationEvidenceTab>(() =>
    parseTab(searchParams.get('tab'))
  );
  const [programId, setProgramId] = useState(() => searchParams.get('program') ?? 'program-rise-1');
  const [objectId, setObjectId] = useState(() => searchParams.get('object') ?? 'object-sales-order');
  const [targetFieldPath, setTargetFieldPath] = useState(
    () => searchParams.get('field') ?? '$.amount'
  );
  const [targetRecordId, setTargetRecordId] = useState(
    () => searchParams.get('targetRecord') ?? 'SO-1'
  );
  const [sourceRecordId, setSourceRecordId] = useState(
    () => searchParams.get('sourceRecord') ?? 'SO-1'
  );
  const [connectorId, setConnectorId] = useState('ibm-artifacts');
  const [connectorJson, setConnectorJson] = useState(CONNECTOR_TEMPLATE);
  const [runJson, setRunJson] = useState(RUN_TEMPLATE);
  const [explanationResult, setExplanationResult] = useState<ExplainValueResponse | null>(null);
  const [packetResult, setPacketResult] = useState<EvidencePacket | null>(null);
  const [controlsResult, setControlsResult] = useState<ControlResult[]>([]);
  const [exceptionsResult, setExceptionsResult] = useState<ExceptionRecord[]>([]);
  const [approvalsResult, setApprovalsResult] = useState<ApprovalEvent[]>([]);
  const [connectorResult, setConnectorResult] = useState<UpsertMigrationConnectorResponse | null>(null);
  const [runResult, setRunResult] = useState<RunMigrationConnectorResponse | null>(null);
  const [pageError, setPageError] = useState<string | null>(null);

  const upsertConnector = useUpsertMigrationConnector();
  const runConnector = useRunMigrationConnector();
  const runtimeStatus = useMigrationRuntimeStatus();
  const rebuildReadModels = useRebuildMigrationReadModels();
  const explainValue = useExplainMigrationValue();
  const lookupPacket = useLookupMigrationEvidencePacket();
  const lookupControls = useLookupMigrationObjectControls();
  const lookupExceptions = useLookupMigrationProgramExceptions();
  const lookupApprovals = useLookupMigrationProgramApprovals();

  useEffect(() => {
    const nextParams = new URLSearchParams(searchParams);
    writeSearchParam(nextParams, 'tab', activeTab !== 'explain' ? activeTab : null);
    writeSearchParam(nextParams, 'program', programId || null);
    writeSearchParam(nextParams, 'object', objectId || null);
    writeSearchParam(nextParams, 'field', targetFieldPath || null);
    writeSearchParam(nextParams, 'targetRecord', targetRecordId || null);
    writeSearchParam(nextParams, 'sourceRecord', sourceRecordId || null);

    if (nextParams.toString() !== searchParams.toString()) {
      setSearchParams(nextParams, { replace: true });
    }
  }, [activeTab, objectId, programId, searchParams, setSearchParams, sourceRecordId, targetFieldPath, targetRecordId]);

  const explanation = explanationResult?.explanation ?? null;
  const runtime = runtimeStatus.data?.status;
  const ingestionStatus = runtimeStatus.data?.ingestion_status ?? null;
  const valueKey = useMemo(
    () => buildValueKey(targetFieldPath, targetRecordId),
    [targetFieldPath, targetRecordId]
  );
  const isLoadingAuditBundle =
    lookupPacket.isPending ||
    lookupControls.isPending ||
    lookupExceptions.isPending ||
    lookupApprovals.isPending;

  const handleExplain = async () => {
    setPageError(null);
    try {
      const response = await explainValue.mutateAsync({
        programId: programId.trim(),
        objectId: objectId.trim(),
        targetFieldPath: targetFieldPath.trim(),
        targetRecordId: trimToUndefined(targetRecordId),
        sourceRecordId: trimToUndefined(sourceRecordId),
      });
      setExplanationResult(response);
      toast.success('Value explanation loaded', {
        description: `${response.explanation.target_field.field_name} is now traceable end to end.`,
      });
    } catch (error) {
      setPageError(getErrorMessage(error));
    }
  };

  const handleLoadAuditBundle = async () => {
    setPageError(null);
    try {
      const currentExplanation = explanationResult ?? (await explainValue.mutateAsync({
        programId: programId.trim(),
        objectId: objectId.trim(),
        targetFieldPath: targetFieldPath.trim(),
        targetRecordId: trimToUndefined(targetRecordId),
        sourceRecordId: trimToUndefined(sourceRecordId),
      }));
      setExplanationResult(currentExplanation);

      const [packet, controls, exceptions, approvals] = await Promise.all([
        lookupPacket.mutateAsync({ objectId: objectId.trim(), valueKey }),
        lookupControls.mutateAsync(objectId.trim()),
        lookupExceptions.mutateAsync(programId.trim()),
        lookupApprovals.mutateAsync(programId.trim()),
      ]);

      setPacketResult(packet.packet);
      setControlsResult(controls.controls);
      setExceptionsResult(exceptions.exceptions);
      setApprovalsResult(approvals.approvals);
      setActiveTab('audit');
      toast.success('Audit bundle loaded', {
        description: 'Evidence packet, controls, exceptions, and approvals are ready for review.',
      });
    } catch (error) {
      setPageError(getErrorMessage(error));
    }
  };

  const handleSaveConnector = async () => {
    setPageError(null);
    try {
      const response = await upsertConnector.mutateAsync(parseJsonObject(connectorJson, 'connector'));
      setConnectorResult(response);
      setConnectorId(response.connector.connector_id);
    } catch (error) {
      setPageError(getErrorMessage(error));
    }
  };

  const handleRunConnector = async () => {
    setPageError(null);
    try {
      const response = await runConnector.mutateAsync({
        connectorId: connectorId.trim(),
        request: parseJsonObject(runJson, 'connector run request'),
      });
      setRunResult(response);
    } catch (error) {
      setPageError(getErrorMessage(error));
    }
  };

  const handleRebuildReadModels = async () => {
    setPageError(null);
    try {
      await rebuildReadModels.mutateAsync();
      await runtimeStatus.refetch();
    } catch (error) {
      setPageError(getErrorMessage(error));
    }
  };

  return (
    <div className="space-y-6 p-6">
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1.4fr)_minmax(320px,0.8fr)]">
        <Card className="border-primary/20 bg-gradient-to-br from-primary/10 via-background to-background">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="rounded-sm border border-primary/30 bg-primary/10 p-2 text-primary">
                <Fingerprint className="h-5 w-5" />
              </div>
              <div>
                <CardTitle>Migration Evidence Graph</CardTitle>
                <CardDescription>
                  ARCXA sits beside IBM, SNP, smartShift, and SAP migration tooling as the persistent evidence and transformation traceability layer.
                </CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4 text-sm text-muted-foreground">
            <p>
              This workspace is optimized for the first proof point: explain one migrated value with source, target, rule, execution, exceptions, controls, approvals, and signed evidence.
            </p>
            <div className="flex flex-wrap gap-2">
              <Badge variant="default">Explain This Value</Badge>
              <Badge variant="secondary">Audit-Ready Evidence</Badge>
              <Badge variant="outline">IBM RISE / SAP HANA</Badge>
            </div>
          </CardContent>
        </Card>

        <div className="grid gap-4 sm:grid-cols-3 lg:grid-cols-1 xl:grid-cols-3">
          <SummaryStatCard
            icon={Search}
            label="Evidence Packet"
            value={packetResult?.packet_id ?? explanation?.evidence_packet_id ?? 'Not loaded'}
            tone={packetResult ? 'success' : 'muted'}
          />
          <SummaryStatCard
            icon={ShieldCheck}
            label="Approvals"
            value={String(approvalsResult.length || explanation?.approvals.length || 0)}
            tone={approvalsResult.length > 0 || (explanation?.approvals.length ?? 0) > 0 ? 'success' : 'muted'}
          />
          <SummaryStatCard
            icon={Activity}
            label="Controls"
            value={String(controlsResult.length || explanation?.controls.length || 0)}
            tone={controlsResult.length > 0 || (explanation?.controls.length ?? 0) > 0 ? 'secondary' : 'muted'}
          />
          <SummaryStatCard
            icon={ServerCog}
            label="Read Model Backend"
            value={runtime ? formatBackend(runtime.backend) : 'Loading'}
            tone={runtime?.event_log_available ? 'success' : 'muted'}
          />
        </div>
      </div>

      {pageError ? (
        <Alert variant="destructive">
          <AlertTitle>Migration evidence request failed</AlertTitle>
          <AlertDescription>{pageError}</AlertDescription>
        </Alert>
      ) : null}

      <Tabs value={activeTab} onValueChange={(value) => setActiveTab(parseTab(value))}>
        <TabsList>
          <TabsTrigger value="explain">Explain Value</TabsTrigger>
          <TabsTrigger value="audit">Audit Bundle</TabsTrigger>
          <TabsTrigger value="connectors">Connectors</TabsTrigger>
        </TabsList>

        <TabsContent value="explain" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle>Explain One Migrated Value</CardTitle>
              <CardDescription>
                Ask for the exact source-to-target chain behind a migrated field or record value.
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 md:grid-cols-2 xl:grid-cols-5">
              <div className="space-y-2 xl:col-span-2">
                <Label htmlFor="program-id">Program ID</Label>
                <Input id="program-id" value={programId} onChange={(event) => setProgramId(event.target.value)} />
              </div>
              <div className="space-y-2 xl:col-span-2">
                <Label htmlFor="object-id">Object ID</Label>
                <Input id="object-id" value={objectId} onChange={(event) => setObjectId(event.target.value)} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="target-field-path">Target Field Path</Label>
                <Input
                  id="target-field-path"
                  value={targetFieldPath}
                  onChange={(event) => setTargetFieldPath(event.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="target-record-id">Target Record ID</Label>
                <Input
                  id="target-record-id"
                  value={targetRecordId}
                  onChange={(event) => setTargetRecordId(event.target.value)}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="source-record-id">Source Record ID</Label>
                <Input
                  id="source-record-id"
                  value={sourceRecordId}
                  onChange={(event) => setSourceRecordId(event.target.value)}
                />
              </div>
              <div className="flex items-end gap-3 xl:col-span-3">
                <Button onClick={handleExplain} disabled={explainValue.isPending}>
                  {explainValue.isPending ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Search className="mr-2 h-4 w-4" />}
                  Explain Value
                </Button>
                <Button variant="secondary" onClick={handleLoadAuditBundle} disabled={explainValue.isPending || isLoadingAuditBundle}>
                  {isLoadingAuditBundle ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <FileCheck2 className="mr-2 h-4 w-4" />}
                  Load Audit Bundle
                </Button>
              </div>
            </CardContent>
          </Card>

          {explanation ? (
            <div className="grid gap-6 xl:grid-cols-[minmax(0,1.2fr)_minmax(320px,0.8fr)]">
              <div className="space-y-6">
                <Card>
                  <CardHeader>
                    <CardTitle>Explanation Summary</CardTitle>
                    <CardDescription>
                      {explanation.confidence_summary ?? 'Traceability evidence assembled from the migration evidence graph.'}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="grid gap-4 md:grid-cols-2">
                    <FieldRefCard title="Source Field" field={explanation.source_field} />
                    <FieldRefCard title="Target Field" field={explanation.target_field} />
                    <ExecutionSummaryCard explanation={explanation} />
                    <EvidenceSummaryCard explanation={explanation} />
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader>
                    <CardTitle>Value Walkthrough</CardTitle>
                    <CardDescription>
                      The exact values and rule expression behind this migrated outcome.
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="grid gap-4 md:grid-cols-2">
                    <JsonBlock title="Source Value" value={explanation.source_value} />
                    <JsonBlock title="Target Value" value={explanation.target_value} />
                    <JsonBlock
                      title="Transformation Rule"
                      value={explanation.transformation_rule}
                      className="md:col-span-2"
                    />
                  </CardContent>
                </Card>
              </div>

              <Card>
                <CardHeader>
                  <CardTitle>Immediate Evidence Signals</CardTitle>
                  <CardDescription>
                    Fast answers for sign-off and post-go-live triage.
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <SignalRow label="Exceptions" value={explanation.exceptions.length} />
                  <SignalRow label="Controls" value={explanation.controls.length} />
                  <SignalRow label="Approvals" value={explanation.approvals.length} />
                  <SignalRow label="Evidence Packet" value={explanation.evidence_packet_id ?? 'pending'} />
                  <SignalRow label="Lookup Key" value={valueKey ?? 'not available'} />
                  <JsonBlock title="Graph References" value={explanation.graph_refs} />
                </CardContent>
              </Card>
            </div>
          ) : (
            <EmptyStateCard
              icon={Search}
              title="No explanation loaded yet"
              description="Run an explain-value request to inspect one migrated field end to end."
            />
          )}
        </TabsContent>

        <TabsContent value="audit" className="space-y-6">
          {packetResult || controlsResult.length || exceptionsResult.length || approvalsResult.length ? (
            <>
              <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(320px,0.8fr)]">
                <Card>
                  <CardHeader>
                    <CardTitle>Signed Evidence Packet</CardTitle>
                    <CardDescription>
                      Canonical packet for auditors, business owners, and IBM delivery leadership.
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-4">
                    {packetResult ? (
                      <>
                        <div className="grid gap-4 md:grid-cols-2">
                          <SignalRow label="Packet ID" value={packetResult.packet_id} />
                          <SignalRow label="Generated" value={formatTimestamp(packetResult.generated_at)} />
                          <SignalRow label="Signature" value={packetResult.signature?.algorithm ?? 'unsigned'} />
                          <SignalRow label="Fingerprint" value={packetResult.signature?.key_fingerprint ?? 'not available'} />
                        </div>
                        <JsonBlock title="Narrative" value={packetResult.narrative ?? null} />
                        <JsonBlock title="Packet Metadata" value={packetResult.metadata} />
                      </>
                    ) : (
                      <p className="text-sm text-muted-foreground">Load the audit bundle to retrieve a signed packet.</p>
                    )}
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader>
                    <CardTitle>Audit Readiness</CardTitle>
                    <CardDescription>
                      A compact view of what is ready to defend and what still needs attention.
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-4">
                    <SignalRow label="Program" value={programId} />
                    <SignalRow label="Object" value={objectId} />
                    <SignalRow label="Exceptions" value={exceptionsResult.length} />
                    <SignalRow label="Approvals" value={approvalsResult.length} />
                    <SignalRow label="Controls" value={controlsResult.length} />
                    <SignalRow label="Value Key" value={valueKey ?? 'not available'} />
                  </CardContent>
                </Card>
              </div>

              <div className="grid gap-6 xl:grid-cols-2">
                <Card>
                  <CardHeader>
                    <CardTitle>Controls</CardTitle>
                    <CardDescription>
                      Verification and reconciliation outcomes captured for this migration object.
                    </CardDescription>
                  </CardHeader>
                  <CardContent>
                    <ControlsTable controls={controlsResult} />
                  </CardContent>
                </Card>

                <Card>
                  <CardHeader>
                    <CardTitle>Approvals</CardTitle>
                    <CardDescription>
                      Business-owner and governance decisions tied to this program object.
                    </CardDescription>
                  </CardHeader>
                  <CardContent>
                    <ApprovalsTable approvals={approvalsResult} />
                  </CardContent>
                </Card>
              </div>

              <Card>
                <CardHeader>
                  <CardTitle>Exceptions</CardTitle>
                  <CardDescription>
                    Accepted, overridden, and open deltas that still shape sign-off conversations.
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <ExceptionsTable exceptions={exceptionsResult} />
                </CardContent>
              </Card>
            </>
          ) : (
            <EmptyStateCard
              icon={BadgeCheck}
              title="No audit bundle loaded yet"
              description="Use Load Audit Bundle from the explain tab to assemble packet, controls, approvals, and exceptions together."
            />
          )}
        </TabsContent>

        <TabsContent value="connectors" className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle>Traceability Runtime</CardTitle>
              <CardDescription>
                High-assurance read-model status for the microservice-owned evidence graph projection layer.
              </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_auto] xl:items-end">
              <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                <SignalRow label="Backend" value={runtime ? formatBackend(runtime.backend) : 'loading'} />
                <SignalRow label="Event Log" value={runtime?.event_log_available ? 'available' : 'not available'} />
                <SignalRow label="Replay" value={runtime?.replay_supported ? 'supported' : 'not supported'} />
                <SignalRow label="Delivery Mode" value={runtime ? formatEventBusMode(runtime.event_bus.mode) : 'loading'} />
                <SignalRow label="Consumer State" value={runtime ? formatConsumerState(runtime.event_bus.consumer_state) : 'loading'} />
                <SignalRow label="Broker Reachability" value={runtime ? formatBrokerReachability(runtime.event_bus.broker_reachability) : 'loading'} />
                <SignalRow label="Discovered Brokers" value={runtime?.event_bus.discovered_broker_count ?? 'unknown'} />
                <SignalRow label="Topic" value={runtime?.event_bus.topic ?? 'not configured'} />
                <SignalRow label="Topic Partitions" value={runtime?.event_bus.topic_partition_count ?? 'unknown'} />
                <SignalRow label="Consumer Group" value={runtime?.event_bus.consumer_group ?? 'not configured'} />
                <SignalRow
                  label="Assigned Partitions"
                  value={runtime?.event_bus.assigned_partitions?.length ? runtime.event_bus.assigned_partitions.join(', ') : 'none'}
                />
                <SignalRow label="Lag Posture" value={runtime ? formatLagState(runtime.event_bus.lag_state) : 'loading'} />
                <SignalRow
                  label="Estimated Lag"
                  value={runtime?.event_bus.estimated_lag_message_count ?? 'unknown'}
                />
                <SignalRow
                  label="Lag Diagnostics"
                  value={runtime?.event_bus.lag_diagnostics ?? 'not observed'}
                />
                <SignalRow label="Processed Messages" value={runtime?.event_bus.processed_message_count ?? 0} />
                <SignalRow label="Malformed Messages" value={runtime?.event_bus.malformed_message_count ?? 0} />
                <SignalRow label="Retry Attempts" value={runtime?.event_bus.retry_attempt_count ?? 0} />
                <SignalRow
                  label="Startup"
                  value={
                    runtime?.event_bus.startup_failure_reason
                      ? 'degraded'
                      : runtime?.event_bus.startup_completed_at
                        ? 'ready'
                        : runtime
                          ? 'pending'
                          : 'loading'
                  }
                />
                <SignalRow
                  label="Last Consumed"
                  value={runtime?.event_bus.last_consumed_at ? formatTimestamp(runtime.event_bus.last_consumed_at) : 'not observed'}
                />
                <SignalRow
                  label="Last Successful Ingest"
                  value={
                    runtime?.event_bus.last_successful_ingest_at
                      ? formatTimestamp(runtime.event_bus.last_successful_ingest_at)
                      : 'not observed'
                  }
                />
                <SignalRow
                  label="Last Retry"
                  value={runtime?.event_bus.last_retry_at ? formatTimestamp(runtime.event_bus.last_retry_at) : 'not observed'}
                />
                <SignalRow
                  label="Last Assignment"
                  value={runtime?.event_bus.last_assignment_at ? formatTimestamp(runtime.event_bus.last_assignment_at) : 'not observed'}
                />
                <SignalRow
                  label="Last Broker Probe"
                  value={runtime?.event_bus.last_broker_probe_at ? formatTimestamp(runtime.event_bus.last_broker_probe_at) : 'not observed'}
                />
                <SignalRow label="Last Sequence" value={runtime?.last_event_sequence ?? 0} />
                <SignalRow label="Programs" value={runtime?.read_models.programs ?? 0} />
                <SignalRow label="Objects" value={runtime?.read_models.objects ?? 0} />
                <SignalRow label="Packets" value={runtime?.read_models.packets ?? 0} />
                <SignalRow label="Events" value={runtime?.read_models.event_log_entries ?? 0} />
                <SignalRow
                  label="Connector Store"
                  value={ingestionStatus ? formatConnectorBackend(ingestionStatus.connector_store.backend) : 'loading'}
                />
                <SignalRow
                  label="Connector Store Health"
                  value={ingestionStatus ? formatConnectorHealth(ingestionStatus.connector_store.health) : 'loading'}
                />
                <SignalRow
                  label="Connector Count"
                  value={ingestionStatus?.connector_store.connector_count ?? 'unknown'}
                />
                <SignalRow
                  label="Connector Writable"
                  value={ingestionStatus ? (ingestionStatus.connector_store.writable ? 'yes' : 'no') : 'loading'}
                />
              </div>
              <div className="flex flex-wrap gap-3">
                <Button
                  variant="secondary"
                  onClick={() => runtimeStatus.refetch()}
                  disabled={runtimeStatus.isFetching}
                >
                  {runtimeStatus.isFetching ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Activity className="mr-2 h-4 w-4" />
                  )}
                  Refresh Status
                </Button>
                <Button
                  onClick={handleRebuildReadModels}
                  disabled={rebuildReadModels.isPending || !runtime?.replay_supported}
                >
                  {rebuildReadModels.isPending ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <RotateCw className="mr-2 h-4 w-4" />
                  )}
                  Rebuild Read Models
                </Button>
              </div>
            </CardContent>
            {runtime?.event_bus.last_error ? (
              <CardContent className="pt-0">
                <Alert>
                  <ServerCog className="h-4 w-4" />
                  <AlertTitle>Event bus attention needed</AlertTitle>
                  <AlertDescription>
                    {runtime.event_bus.last_error}
                  </AlertDescription>
                </Alert>
              </CardContent>
            ) : null}
            {runtime?.event_bus.startup_failure_reason ? (
              <CardContent className="pt-0">
                <Alert>
                  <ServerCog className="h-4 w-4" />
                  <AlertTitle>Consumer startup failed</AlertTitle>
                  <AlertDescription>
                    {runtime.event_bus.startup_failure_reason}
                  </AlertDescription>
                </Alert>
              </CardContent>
            ) : null}
            {ingestionStatus?.connector_store.last_error ? (
              <CardContent className="pt-0">
                <Alert>
                  <ServerCog className="h-4 w-4" />
                  <AlertTitle>Connector store attention needed</AlertTitle>
                  <AlertDescription>
                    {ingestionStatus.connector_store.last_error}
                  </AlertDescription>
                </Alert>
              </CardContent>
            ) : null}
          </Card>

          <div className="grid gap-6 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Connector Template</CardTitle>
                <CardDescription>
                  Register IBM, SNP, smartShift, or SAP evidence sources without duplicating runtime logic in the UI.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <Textarea
                  value={connectorJson}
                  onChange={(event) => setConnectorJson(event.target.value)}
                  className="min-h-[360px] font-mono text-xs"
                />
                <Button onClick={handleSaveConnector} disabled={upsertConnector.isPending}>
                  {upsertConnector.isPending ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Database className="mr-2 h-4 w-4" />}
                  Save Connector
                </Button>
                {connectorResult ? <JsonBlock title="Saved Connector" value={connectorResult} /> : null}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Run Template</CardTitle>
                <CardDescription>
                  Kick off manual artifact ingestion or verification-backed evidence capture against the coordinator gateway.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="connector-run-id">Connector ID</Label>
                  <Input
                    id="connector-run-id"
                    value={connectorId}
                    onChange={(event) => setConnectorId(event.target.value)}
                  />
                </div>
                <Textarea
                  value={runJson}
                  onChange={(event) => setRunJson(event.target.value)}
                  className="min-h-[360px] font-mono text-xs"
                />
                <Button onClick={handleRunConnector} disabled={runConnector.isPending}>
                  {runConnector.isPending ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <PlayCircle className="mr-2 h-4 w-4" />}
                  Start Connector Run
                </Button>
                {runResult ? <JsonBlock title="Run Summary" value={runResult} /> : null}
              </CardContent>
            </Card>
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function SummaryStatCard({
  icon: Icon,
  label,
  value,
  tone,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: string;
  tone: 'success' | 'secondary' | 'muted';
}) {
  const badgeVariant = tone === 'success' ? 'success' : tone === 'secondary' ? 'secondary' : 'outline';

  return (
    <Card>
      <CardContent className="flex items-center justify-between gap-3 p-4">
        <div>
          <div className="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
          <div className="mt-1 text-sm font-semibold text-foreground break-all">{value}</div>
        </div>
        <Badge variant={badgeVariant}>
          <Icon className="h-3.5 w-3.5" />
        </Badge>
      </CardContent>
    </Card>
  );
}

function FieldRefCard({
  title,
  field,
}: {
  title: string;
  field: { system: string; object_name: string; field_name: string; field_path: string; record_id?: string | null };
}) {
  return (
    <Card className="border-border/60">
      <CardHeader className="pb-3">
        <CardTitle className="text-sm">{title}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <SignalRow label="System" value={field.system} />
        <SignalRow label="Object" value={field.object_name} />
        <SignalRow label="Field" value={field.field_name} />
        <SignalRow label="Path" value={field.field_path} />
        <SignalRow label="Record" value={field.record_id ?? 'n/a'} />
      </CardContent>
    </Card>
  );
}

function ExecutionSummaryCard({ explanation }: { explanation: ValueExplanation }) {
  return (
    <Card className="border-border/60">
      <CardHeader className="pb-3">
        <CardTitle className="text-sm">Execution Context</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <SignalRow label="Tool" value={explanation.execution_event?.tool_name ?? 'not captured'} />
        <SignalRow label="Stage" value={explanation.execution_event?.stage ?? 'not captured'} />
        <SignalRow
          label="Occurred"
          value={explanation.execution_event?.happened_at ? formatTimestamp(explanation.execution_event.happened_at) : 'not captured'}
        />
        <SignalRow label="Run ID" value={explanation.execution_event?.tool_run_id ?? 'not captured'} />
      </CardContent>
    </Card>
  );
}

function EvidenceSummaryCard({ explanation }: { explanation: ValueExplanation }) {
  return (
    <Card className="border-border/60">
      <CardHeader className="pb-3">
        <CardTitle className="text-sm">Evidence Summary</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <SignalRow label="Rule" value={explanation.transformation_rule?.name ?? 'not captured'} />
        <SignalRow label="Exceptions" value={String(explanation.exceptions.length)} />
        <SignalRow label="Controls" value={String(explanation.controls.length)} />
        <SignalRow label="Approvals" value={String(explanation.approvals.length)} />
      </CardContent>
    </Card>
  );
}

function SignalRow({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-border/40 py-1 last:border-b-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="max-w-[70%] break-all text-right text-foreground">{String(value)}</span>
    </div>
  );
}

function JsonBlock({
  title,
  value,
  className,
}: {
  title: string;
  value: unknown;
  className?: string;
}) {
  return (
    <div className={className}>
      <div className="mb-2 text-sm font-medium text-foreground">{title}</div>
      <div className="rounded-sm border border-border/60 bg-muted/30 p-3">
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-all text-xs text-foreground">
          {formatJson(value)}
        </pre>
      </div>
    </div>
  );
}

function ControlsTable({ controls }: { controls: ControlResult[] }) {
  if (!controls.length) {
    return <p className="text-sm text-muted-foreground">No control results loaded.</p>;
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Control</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Executed</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {controls.map((control) => (
          <TableRow key={control.control_id}>
            <TableCell>
              <div className="font-medium">{control.control_name}</div>
              <div className="text-xs text-muted-foreground">{control.summary}</div>
            </TableCell>
            <TableCell>
              <Badge variant={control.status === 'passed' ? 'success' : control.status === 'warning' ? 'warning' : 'destructive'}>
                {control.status}
              </Badge>
            </TableCell>
            <TableCell>{formatTimestamp(control.executed_at)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function ApprovalsTable({ approvals }: { approvals: ApprovalEvent[] }) {
  if (!approvals.length) {
    return <p className="text-sm text-muted-foreground">No approvals loaded.</p>;
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Approver</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Approved</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {approvals.map((approval) => (
          <TableRow key={approval.approval_id}>
            <TableCell>
              <div className="font-medium">{approval.approver_role}</div>
              <div className="text-xs text-muted-foreground">{approval.approver_id}</div>
            </TableCell>
            <TableCell>
              <Badge variant={approval.status === 'approved' ? 'success' : approval.status === 'pending' ? 'warning' : 'destructive'}>
                {approval.status}
              </Badge>
            </TableCell>
            <TableCell>{formatTimestamp(approval.approved_at)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function ExceptionsTable({ exceptions }: { exceptions: ExceptionRecord[] }) {
  if (!exceptions.length) {
    return <p className="text-sm text-muted-foreground">No exceptions loaded.</p>;
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Severity</TableHead>
          <TableHead>Category</TableHead>
          <TableHead>Message</TableHead>
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {exceptions.map((exception) => (
          <TableRow key={exception.exception_id}>
            <TableCell>
              <Badge variant={exception.severity === 'warning' ? 'warning' : exception.severity === 'info' ? 'secondary' : 'destructive'}>
                {exception.severity}
              </Badge>
            </TableCell>
            <TableCell>{exception.category}</TableCell>
            <TableCell>
              <div className="font-medium">{exception.message}</div>
              {exception.remediation ? (
                <div className="text-xs text-muted-foreground">{exception.remediation}</div>
              ) : null}
            </TableCell>
            <TableCell>{exception.status}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function EmptyStateCard({
  icon: Icon,
  title,
  description,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description: string;
}) {
  return (
    <Card>
      <CardContent className="flex min-h-[220px] flex-col items-center justify-center gap-3 p-8 text-center">
        <div className="rounded-sm border border-border/60 bg-muted/30 p-3 text-muted-foreground">
          <Icon className="h-5 w-5" />
        </div>
        <div>
          <div className="font-medium text-foreground">{title}</div>
          <div className="mt-1 text-sm text-muted-foreground">{description}</div>
        </div>
      </CardContent>
    </Card>
  );
}

function parseTab(value: string | null): MigrationEvidenceTab {
  switch (value) {
    case 'audit':
    case 'connectors':
      return value;
    default:
      return 'explain';
  }
}

function writeSearchParam(params: URLSearchParams, key: string, value: string | null) {
  if (value && value.trim()) {
    params.set(key, value.trim());
  } else {
    params.delete(key);
  }
}

function parseJsonObject(value: string, label: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(value);
    if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
      throw new Error(`${label} must be a JSON object`);
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    throw new Error(`${label} JSON is invalid: ${getErrorMessage(error)}`);
  }
}

function buildValueKey(targetFieldPath: string, targetRecordId: string): string | undefined {
  const trimmedFieldPath = targetFieldPath.trim();
  if (!trimmedFieldPath) {
    return undefined;
  }

  const trimmedRecordId = targetRecordId.trim();
  if (!trimmedRecordId) {
    return trimmedFieldPath;
  }

  return `${trimmedRecordId}::${trimmedFieldPath}`;
}

function formatJson(value: unknown): string {
  if (value === undefined) {
    return 'Not available';
  }

  if (typeof value === 'string') {
    return value;
  }

  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function formatTimestamp(value: string): string {
  try {
    return format(new Date(value), 'PPpp');
  } catch {
    return value;
  }
}

function formatBackend(value: 'file' | 'rocks_db'): string {
  return value === 'rocks_db' ? 'RocksDB' : 'File';
}

function formatConnectorBackend(value: 'unknown' | 'file' | 'rocks_db'): string {
  switch (value) {
    case 'rocks_db':
      return 'RocksDB';
    case 'file':
      return 'File';
    default:
      return 'Unknown';
  }
}

function formatConnectorHealth(value: 'unknown' | 'healthy' | 'degraded' | 'unavailable'): string {
  switch (value) {
    case 'healthy':
      return 'healthy';
    case 'degraded':
      return 'degraded';
    case 'unavailable':
      return 'unavailable';
    default:
      return 'unknown';
  }
}

function formatEventBusMode(value: 'direct' | 'kafka'): string {
  return value === 'kafka' ? 'Kafka' : 'Direct';
}

function formatConsumerState(value: 'disabled' | 'running' | 'recovering' | 'stopped'): string {
  switch (value) {
    case 'running':
      return 'running';
    case 'recovering':
      return 'recovering';
    case 'stopped':
      return 'stopped';
    default:
      return 'disabled';
  }
}

function formatBrokerReachability(value: 'unknown' | 'reachable' | 'degraded' | 'unreachable'): string {
  switch (value) {
    case 'reachable':
      return 'reachable';
    case 'degraded':
      return 'degraded';
    case 'unreachable':
      return 'unreachable';
    default:
      return 'unknown';
  }
}

function formatLagState(value: 'unknown' | 'caught_up' | 'backlog'): string {
  switch (value) {
    case 'caught_up':
      return 'caught up';
    case 'backlog':
      return 'backlog observed';
    default:
      return 'unknown';
  }
}

function trimToUndefined(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function getErrorMessage(error: unknown): string {
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
    'Request failed'
  );
}
