import React, { useEffect, useMemo, useState } from 'react';
import { format } from 'date-fns';
import {
  AlertTriangle,
  CheckCircle2,
  Database,
  Loader2,
  Network,
  Search,
  Workflow,
} from 'lucide-react';
import { useSearchParams } from 'react-router-dom';
import { toast } from 'sonner';

import type {
  SosCompatibilityScore,
  SosDataContract,
  SosInterfaceRecord,
  SosValidationCheck,
  SosValidationResponse,
} from '@/api/sosValidation';
import { buildInterfacePairSubjectKey } from '@/api/sosValidation';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  SosCatalogPanel,
  type SosCatalogSelectionState,
  type SosCatalogTab,
} from '@/components/sos/SosCatalogPanel';
import {
  createDefaultSosAnalyticsInvestigationState,
  type SosAnalyticsInvestigationState,
} from '@/components/sos/sosAnalyticsState';
import { SosAnalyticsPanel } from '@/components/sos/SosAnalyticsPanel';
import { SosOperationsPanel } from '@/components/sos/SosOperationsPanel';
import { SosPoliciesPanel } from '@/components/sos/SosPoliciesPanel';
import { SosReportsPanel } from '@/components/sos/SosReportsPanel';
import {
  DEFAULT_VISIBLE_SOS_GRAPH_KINDS,
  parseVisibleKinds,
} from '@/components/sos/sosDependencyGraphUtils';
import { cn } from '@/lib/utils';
import {
  useLookupSosContract,
  useSosCompatibilityMatrix,
  useSosContracts,
  useSosInterfaces,
  useValidateInterfacePair,
} from '@/hooks/useSosValidation';

type SosTab =
  | 'workbench'
  | 'reports'
  | 'catalog'
  | 'policies'
  | 'analytics'
  | 'matrix'
  | 'operations';

interface ReportNavigationTarget {
  reportId?: string | null;
  subjectType?: string;
  subjectKey?: string;
}

interface CatalogNavigationTarget {
  tab: SosCatalogTab;
  systemId?: string | null;
  interfaceId?: string | null;
  contractId?: string | null;
}

export function SosValidation() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [activeTab, setActiveTab] = useState<SosTab>(() => parseSosTab(searchParams.get('tab')));
  const [providerInterfaceId, setProviderInterfaceId] = useState(
    () => searchParams.get('provider') ?? ''
  );
  const [consumerInterfaceId, setConsumerInterfaceId] = useState(
    () => searchParams.get('consumer') ?? ''
  );
  const [contractResult, setContractResult] = useState<SosDataContract | null>(null);
  const [validationResult, setValidationResult] = useState<SosValidationResponse | null>(null);
  const [lookupErrorMessage, setLookupErrorMessage] = useState<string | null>(null);
  const [validationErrorMessage, setValidationErrorMessage] = useState<string | null>(null);
  const [reportSeedId, setReportSeedId] = useState<string | null>(
    () => searchParams.get('reportId')
  );
  const [reportSeedSubjectType, setReportSeedSubjectType] = useState<string | null>(
    () => searchParams.get('reportSubjectType')
  );
  const [reportSeedSubjectKey, setReportSeedSubjectKey] = useState<string | null>(
    () => searchParams.get('reportSubjectKey')
  );
  const [catalogSeedToken, setCatalogSeedToken] = useState(0);
  const [catalogSeedTab, setCatalogSeedTab] = useState<SosCatalogTab>(() =>
    parseCatalogTab(searchParams.get('catalogTab'))
  );
  const [catalogSeedSystemId, setCatalogSeedSystemId] = useState<string | null>(
    () => searchParams.get('catalogSystem')
  );
  const [catalogSeedInterfaceId, setCatalogSeedInterfaceId] = useState<string | null>(
    () => searchParams.get('catalogInterface')
  );
  const [catalogSeedContractId, setCatalogSeedContractId] = useState<string | null>(
    () => searchParams.get('catalogContract')
  );
  const [analyticsInvestigationState, setAnalyticsInvestigationState] =
    useState<SosAnalyticsInvestigationState>(() =>
      parseAnalyticsInvestigationState(searchParams)
    );

  const {
    data: interfaceRecords,
    isLoading: isLoadingInterfaces,
    error: interfacesError,
  } = useSosInterfaces();
  const {
    data: contracts,
    isLoading: isLoadingContracts,
    error: contractsError,
  } = useSosContracts();
  const {
    data: compatibilityMatrix,
    isLoading: isLoadingMatrix,
    error: matrixError,
  } = useSosCompatibilityMatrix();

  const lookupContract = useLookupSosContract();
  const validatePair = useValidateInterfacePair();

  const interfaces = useMemo(() => interfaceRecords ?? [], [interfaceRecords]);
  const contractList = useMemo(() => contracts ?? [], [contracts]);
  const matrixRows = useMemo(() => compatibilityMatrix?.matrix ?? [], [compatibilityMatrix]);

  const trimmedProviderId = providerInterfaceId.trim();
  const trimmedConsumerId = consumerInterfaceId.trim();
  const latestPersistedReportId = reportSeedId ?? validationResult?.report_id ?? null;
  const shouldPersistCatalogSeed =
    catalogSeedTab !== 'systems' ||
    Boolean(catalogSeedSystemId || catalogSeedInterfaceId || catalogSeedContractId);

  useEffect(() => {
    setContractResult(null);
    setValidationResult(null);
    setLookupErrorMessage(null);
    setValidationErrorMessage(null);
  }, [trimmedProviderId, trimmedConsumerId]);

  useEffect(() => {
    const nextParams = new URLSearchParams(searchParams);

    writeSearchParam(nextParams, 'tab', activeTab !== 'workbench' ? activeTab : null);
    writeSearchParam(nextParams, 'provider', trimmedProviderId || null);
    writeSearchParam(nextParams, 'consumer', trimmedConsumerId || null);
    writeSearchParam(nextParams, 'reportId', latestPersistedReportId);
    writeSearchParam(nextParams, 'reportSubjectType', reportSeedSubjectType);
    writeSearchParam(nextParams, 'reportSubjectKey', reportSeedSubjectKey);
    writeSearchParam(nextParams, 'catalogTab', shouldPersistCatalogSeed ? catalogSeedTab : null);
    writeSearchParam(nextParams, 'catalogSystem', catalogSeedSystemId);
    writeSearchParam(nextParams, 'catalogInterface', catalogSeedInterfaceId);
    writeSearchParam(nextParams, 'catalogContract', catalogSeedContractId);
    writeSearchParam(
      nextParams,
      'analyticsGraph',
      analyticsInvestigationState.graphLoaded ? '1' : null
    );
    writeSearchParam(
      nextParams,
      'analyticsNode',
      analyticsInvestigationState.selectedNodeId ?? null
    );
    writeSearchParam(
      nextParams,
      'analyticsEdge',
      analyticsInvestigationState.selectedEdgeKey ?? null
    );

    const visibleKindsAreDefault =
      analyticsInvestigationState.visibleKinds.join(',') ===
      DEFAULT_VISIBLE_SOS_GRAPH_KINDS.join(',');
    writeSearchParam(
      nextParams,
      'analyticsLanes',
      visibleKindsAreDefault ? null : analyticsInvestigationState.visibleKinds.join(',')
    );

    if (nextParams.toString() !== searchParams.toString()) {
      setSearchParams(nextParams, { replace: true });
    }
  }, [
    activeTab,
    analyticsInvestigationState,
    catalogSeedContractId,
    catalogSeedInterfaceId,
    catalogSeedSystemId,
    catalogSeedTab,
    latestPersistedReportId,
    reportSeedSubjectKey,
    reportSeedSubjectType,
    searchParams,
    setSearchParams,
    shouldPersistCatalogSeed,
    trimmedConsumerId,
    trimmedProviderId,
  ]);

  const interfaceMap = useMemo(
    () => new Map<string, SosInterfaceRecord>(interfaces.map((record) => [record.interface_id, record])),
    [interfaces]
  );

  const sortedInterfaces = useMemo(
    () =>
      [...interfaces].sort(
        (left, right) =>
          left.interface_name.localeCompare(right.interface_name) ||
          left.interface_id.localeCompare(right.interface_id)
      ),
    [interfaces]
  );

  const sortedContracts = useMemo(
    () =>
      [...contractList].sort((left, right) => right.updated_at.localeCompare(left.updated_at)),
    [contractList]
  );

  const sortedMatrixRows = useMemo(
    () => [...matrixRows].sort((left, right) => left.score - right.score),
    [matrixRows]
  );

  const providerInterface = trimmedProviderId ? interfaceMap.get(trimmedProviderId) : undefined;
  const consumerInterface = trimmedConsumerId ? interfaceMap.get(trimmedConsumerId) : undefined;

  const approvedContracts = contractList.filter((contract) => contract.approved).length;
  const signedContracts = contractList.filter((contract) => contract.signed).length;
  const lowestScorePair = sortedMatrixRows[0];

  const handleLookup = async () => {
    if (!trimmedProviderId || !trimmedConsumerId) {
      toast.warning('Select an interface pair first', {
        description: 'Both provider and consumer interface ids are required.',
      });
      return;
    }

    setLookupErrorMessage(null);

    try {
      const result = await lookupContract.mutateAsync({
        providerInterfaceId: trimmedProviderId,
        consumerInterfaceId: trimmedConsumerId,
      });
      setContractResult(result);
    } catch (error) {
      setLookupErrorMessage(getErrorMessage(error));
    }
  };

  const handleValidate = async () => {
    if (!trimmedProviderId || !trimmedConsumerId) {
      toast.warning('Select an interface pair first', {
        description: 'Both provider and consumer interface ids are required.',
      });
      return;
    }

    setValidationErrorMessage(null);

    try {
      const result = await validatePair.mutateAsync({
        providerInterfaceId: trimmedProviderId,
        consumerInterfaceId: trimmedConsumerId,
      });
      setValidationResult(result);
    } catch (error) {
      setValidationErrorMessage(getErrorMessage(error));
    }
  };

  const handleUsePair = (providerId: string, consumerId: string) => {
    setProviderInterfaceId(providerId);
    setConsumerInterfaceId(consumerId);
    setActiveTab('workbench');
  };

  const handleOpenReports = (target?: ReportNavigationTarget) => {
    if (target?.reportId !== undefined) {
      setReportSeedId(target.reportId ?? null);
    }
    if (target?.subjectType !== undefined) {
      setReportSeedSubjectType(target.subjectType ?? null);
    }
    if (target?.subjectKey !== undefined) {
      setReportSeedSubjectKey(target.subjectKey ?? null);
    }
    setActiveTab('reports');
  };

  const handleOpenCatalog = (target: CatalogNavigationTarget) => {
    setCatalogSeedTab(target.tab);
    setCatalogSeedSystemId(target.systemId ?? null);
    setCatalogSeedInterfaceId(target.interfaceId ?? null);
    setCatalogSeedContractId(target.contractId ?? null);
    setCatalogSeedToken((current) => current + 1);
    setActiveTab('catalog');
  };

  const handleCatalogSelectionChange = (state: SosCatalogSelectionState) => {
    setCatalogSeedTab(state.tab);
    setCatalogSeedSystemId(state.systemId ?? null);
    setCatalogSeedInterfaceId(state.interfaceId ?? null);
    setCatalogSeedContractId(state.contractId ?? null);
  };

  const isPairBusy = lookupContract.isPending || validatePair.isPending;
  const pairReady = Boolean(trimmedProviderId && trimmedConsumerId);
  const currentPair = pairReady
    ? {
        providerInterfaceId: trimmedProviderId,
        consumerInterfaceId: trimmedConsumerId,
      }
    : null;

  return (
    <div className="space-y-4 pb-8">
      <div className="space-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="secondary">Dedicated Area</Badge>
          <Badge variant="outline">Systems-of-Systems</Badge>
        </div>
        <h1 className="text-2xl font-semibold text-foreground">Systems-of-Systems Validation</h1>
        <p className="max-w-4xl text-sm text-muted-foreground">
          Keep interface-pair validation in its own workspace. This area is wired directly to the
          coordinator's SoS endpoints for pair validation, persisted reports, catalog governance,
          policy evaluation, and scenario analysis without burying the flow inside workflows or
          ontology mapping.
        </p>
      </div>

      {(interfacesError || contractsError) && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>SoS catalogue data could not be loaded</AlertTitle>
          <AlertDescription>
            {interfacesError && <p>Interfaces: {getErrorMessage(interfacesError)}</p>}
            {contractsError && <p>Contracts: {getErrorMessage(contractsError)}</p>}
          </AlertDescription>
        </Alert>
      )}

      <div className="grid gap-4 lg:grid-cols-2 xl:grid-cols-4">
        <SummaryCard
          title="Known Interfaces"
          value={isLoadingInterfaces ? '...' : String(interfaces.length)}
          description="Registered provider and consumer surfaces available for pairing."
          icon={Network}
        />
        <SummaryCard
          title="Registered Contracts"
          value={isLoadingContracts ? '...' : String(contractList.length)}
          description="Persisted interface-pair contracts currently discoverable."
          icon={Database}
        />
        <SummaryCard
          title="Approved / Signed"
          value={isLoadingContracts ? '...' : `${approvedContracts} / ${signedContracts}`}
          description="A quick read on whether contract governance is complete."
          icon={CheckCircle2}
        />
        <SummaryCard
          title="Lowest Matrix Score"
          value={
            isLoadingMatrix
              ? '...'
              : lowestScorePair
                ? `${formatPercent(lowestScorePair.score)}`
                : 'n/a'
          }
          description={
            lowestScorePair
              ? `${lowestScorePair.provider_interface_id} -> ${lowestScorePair.consumer_interface_id}`
              : 'Load compatibility analytics to spot risky pairings.'
          }
          icon={Workflow}
        />
      </div>

      <Tabs
        value={activeTab}
        onValueChange={(nextValue) => setActiveTab(nextValue as SosTab)}
        className="space-y-4"
      >
        <TabsList className="flex h-auto w-full flex-wrap gap-1">
          <TabsTrigger value="workbench">Pair Workbench</TabsTrigger>
          <TabsTrigger value="reports">Reports</TabsTrigger>
          <TabsTrigger value="catalog">Catalog</TabsTrigger>
          <TabsTrigger value="policies">Policies</TabsTrigger>
          <TabsTrigger value="analytics">Analytics</TabsTrigger>
          <TabsTrigger value="matrix">Compatibility Matrix</TabsTrigger>
          <TabsTrigger value="operations">Operations</TabsTrigger>
        </TabsList>

        <TabsContent value="workbench" className="space-y-4">
          <div className="grid gap-4 xl:grid-cols-[420px,minmax(0,1fr)]">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Search className="h-4 w-4" />
                  Contract And Validation Workbench
                </CardTitle>
                <CardDescription>
                  Enter or select the exact provider and consumer interface ids, then drive the
                  canonical SoS lookup and validation endpoints from one place.
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="provider-interface-id">Provider Interface</Label>
                  <Input
                    id="provider-interface-id"
                    list="sos-interface-options"
                    value={providerInterfaceId}
                    onChange={(event) => setProviderInterfaceId(event.target.value)}
                    placeholder="provider-interface-id"
                    autoComplete="off"
                  />
                  <InterfaceSelectionSummary
                    label="Provider"
                    interfaceRecord={providerInterface}
                    emptyMessage="Use the exact provider interface id or choose a known interface below."
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="consumer-interface-id">Consumer Interface</Label>
                  <Input
                    id="consumer-interface-id"
                    list="sos-interface-options"
                    value={consumerInterfaceId}
                    onChange={(event) => setConsumerInterfaceId(event.target.value)}
                    placeholder="consumer-interface-id"
                    autoComplete="off"
                  />
                  <InterfaceSelectionSummary
                    label="Consumer"
                    interfaceRecord={consumerInterface}
                    emptyMessage="Use the exact consumer interface id or choose a known interface below."
                  />
                </div>

                <datalist id="sos-interface-options">
                  {sortedInterfaces.map((record) => (
                    <option
                      key={record.interface_id}
                      value={record.interface_id}
                      label={`${record.interface_name} (${record.system_id})`}
                    />
                  ))}
                </datalist>

                <div className="flex flex-wrap gap-2">
                  <Button onClick={handleLookup} disabled={!pairReady || isPairBusy}>
                    {lookupContract.isPending ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : (
                      <Search className="mr-2 h-4 w-4" />
                    )}
                    Lookup Contract
                  </Button>
                  <Button
                    variant="outline"
                    onClick={handleValidate}
                    disabled={!pairReady || isPairBusy}
                  >
                    {validatePair.isPending ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : (
                      <Workflow className="mr-2 h-4 w-4" />
                    )}
                    Validate Pair
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => {
                      setProviderInterfaceId(consumerInterfaceId);
                      setConsumerInterfaceId(providerInterfaceId);
                    }}
                    disabled={!pairReady || isPairBusy}
                  >
                    Swap Pair
                  </Button>
                </div>

                <div className="rounded-sm border border-border bg-background-secondary p-3 text-xs text-muted-foreground">
                  The contract lookup call uses the coordinator's direct interface-pair index, so
                  this workbench is aligned with the backend's canonical lookup path rather than a
                  client-side scan of all contracts.
                </div>
              </CardContent>
            </Card>

            <div className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle>Contract Coverage</CardTitle>
                  <CardDescription>
                    Confirm whether the selected interface pair has a contract and whether that
                    contract is approved and signed.
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  {lookupContract.isPending ? (
                    <LoadingState label="Looking up the interface pair contract..." />
                  ) : lookupErrorMessage ? (
                    <InlineError message={lookupErrorMessage} />
                  ) : contractResult ? (
                    <ContractResult contract={contractResult} />
                  ) : (
                    <EmptyState
                      title="No lookup result yet"
                      description="Run Lookup Contract to confirm whether this interface pair is already covered by a registered contract."
                    />
                  )}
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle>Compatibility Validation</CardTitle>
                  <CardDescription>
                    Execute the SoS compatibility validator for the selected provider and consumer
                    interfaces.
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  {validatePair.isPending ? (
                    <LoadingState label="Running interface compatibility validation..." />
                  ) : validationErrorMessage ? (
                    <InlineError message={validationErrorMessage} />
                  ) : validationResult ? (
                    <ValidationResultPanel
                      validation={validationResult}
                      onOpenReports={() =>
                        handleOpenReports({
                          reportId: validationResult.report_id,
                          subjectType: currentPair ? 'interface_pair' : undefined,
                          subjectKey: currentPair
                            ? buildInterfacePairSubjectKey(
                                currentPair.providerInterfaceId,
                                currentPair.consumerInterfaceId
                              )
                            : undefined,
                        })
                      }
                    />
                  ) : (
                    <EmptyState
                      title="No validation report yet"
                      description="Run Validate Pair to see the coordinator's compatibility checks, confidence score, and persisted report id if one was produced."
                    />
                  )}
                </CardContent>
              </Card>
            </div>
          </div>

          <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr),420px]">
            <Card>
              <CardHeader>
                <CardTitle>Known Interfaces</CardTitle>
                <CardDescription>
                  Use the catalogue below to populate the workbench with exact interface ids.
                </CardDescription>
              </CardHeader>
              <CardContent>
                {isLoadingInterfaces ? (
                  <LoadingState label="Loading interface catalogue..." />
                ) : sortedInterfaces.length === 0 ? (
                  <EmptyState
                    title="No interfaces registered"
                    description="Register SoS interfaces in the coordinator before using the workbench."
                  />
                ) : (
                  <div className="max-h-[520px] space-y-3 overflow-auto pr-1">
                    {sortedInterfaces.map((record) => (
                      <div
                        key={record.interface_id}
                        className="rounded-sm border border-border bg-background p-3"
                      >
                        <div className="flex flex-wrap items-start justify-between gap-2">
                          <div className="space-y-1">
                            <div className="font-medium text-foreground">{record.interface_name}</div>
                            <div className="text-xs text-muted-foreground">{record.interface_id}</div>
                            <div className="text-xs text-muted-foreground">
                              System: {record.system_id}
                            </div>
                          </div>
                          <Badge variant={directionVariant(record.direction)}>{record.direction}</Badge>
                        </div>
                        <div className="mt-3 flex flex-wrap gap-2">
                          <Badge variant="outline">{record.protocol}</Badge>
                          <Badge variant="outline">{record.data_format}</Badge>
                          {record.unit_system && <Badge variant="outline">{record.unit_system}</Badge>}
                          {record.coordinate_system && (
                            <Badge variant="outline">{record.coordinate_system}</Badge>
                          )}
                        </div>
                        <div className="mt-3 flex flex-wrap gap-2">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => setProviderInterfaceId(record.interface_id)}
                          >
                            Use As Provider
                          </Button>
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => setConsumerInterfaceId(record.interface_id)}
                          >
                            Use As Consumer
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Registered Contracts</CardTitle>
                <CardDescription>
                  Review recent contracts and jump straight into the workbench using their stored
                  interface pair.
                </CardDescription>
              </CardHeader>
              <CardContent>
                {isLoadingContracts ? (
                  <LoadingState label="Loading contract catalogue..." />
                ) : sortedContracts.length === 0 ? (
                  <EmptyState
                    title="No contracts registered"
                    description="Once contracts exist, this panel becomes the fast path back into validation."
                  />
                ) : (
                  <div className="space-y-3">
                    {sortedContracts.slice(0, 8).map((contract) => (
                      <div
                        key={contract.contract_id}
                        className="rounded-sm border border-border bg-background p-3"
                      >
                        <div className="flex flex-wrap items-start justify-between gap-2">
                          <div className="space-y-1">
                            <div className="font-medium text-foreground">{contract.contract_name}</div>
                            <div className="text-xs text-muted-foreground">
                              {contract.contract_id}
                            </div>
                          </div>
                          <div className="flex flex-wrap gap-2">
                            <Badge variant={contract.approved ? 'success' : 'warning'}>
                              {contract.approved ? 'Approved' : 'Pending Approval'}
                            </Badge>
                            <Badge variant={contract.signed ? 'success' : 'outline'}>
                              {contract.signed ? 'Signed' : 'Unsigned'}
                            </Badge>
                          </div>
                        </div>
                        <div className="mt-3 space-y-1 text-xs text-muted-foreground">
                          <p>Provider: {contract.provider_interface_id}</p>
                          <p>Consumer: {contract.consumer_interface_id}</p>
                          <p>Updated: {formatTimestamp(contract.updated_at)}</p>
                        </div>
                        <div className="mt-3 flex flex-wrap gap-2">
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() =>
                              handleUsePair(
                                contract.provider_interface_id,
                                contract.consumer_interface_id
                              )
                            }
                          >
                            Use Pair
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="reports" className="space-y-4">
          <SosReportsPanel
            currentPair={currentPair}
            latestReportId={reportSeedId ?? validationResult?.report_id ?? null}
            seedSubjectType={reportSeedSubjectType ?? undefined}
            seedSubjectKey={reportSeedSubjectKey ?? undefined}
          />
        </TabsContent>

        <TabsContent value="catalog" className="space-y-4">
          <SosCatalogPanel
            seedTab={catalogSeedTab}
            seedSystemId={catalogSeedSystemId}
            seedInterfaceId={catalogSeedInterfaceId}
            seedContractId={catalogSeedContractId}
            seedToken={catalogSeedToken}
            onSelectionChange={handleCatalogSelectionChange}
          />
        </TabsContent>

        <TabsContent value="policies" className="space-y-4">
          <SosPoliciesPanel currentPair={currentPair} onOpenReports={handleOpenReports} />
        </TabsContent>

        <TabsContent value="analytics" className="space-y-4">
          <SosAnalyticsPanel
            currentPair={currentPair}
            onOpenReports={handleOpenReports}
            onOpenCatalog={handleOpenCatalog}
            onUsePair={handleUsePair}
            investigationState={analyticsInvestigationState}
            onInvestigationStateChange={setAnalyticsInvestigationState}
          />
        </TabsContent>

        <TabsContent value="matrix" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Compatibility Matrix</CardTitle>
              <CardDescription>
                Coordinator-generated compatibility scores, sorted from weakest to strongest pairing
                so the riskiest interface combinations surface first.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {matrixError ? (
                <InlineError message={getErrorMessage(matrixError)} />
              ) : isLoadingMatrix ? (
                <LoadingState label="Generating compatibility matrix..." />
              ) : sortedMatrixRows.length === 0 ? (
                <EmptyState
                  title="No compatibility rows available"
                  description="Register SoS interfaces first, then reload the matrix to inspect pairwise scores."
                />
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Provider</TableHead>
                      <TableHead>Consumer</TableHead>
                      <TableHead>Score</TableHead>
                      <TableHead>Signals</TableHead>
                      <TableHead className="w-[120px]">Action</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {sortedMatrixRows.map((row) => (
                      <CompatibilityRow
                        key={`${row.provider_interface_id}::${row.consumer_interface_id}`}
                        row={row}
                        onUsePair={handleUsePair}
                      />
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="operations" className="space-y-4">
          <SosOperationsPanel />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function SummaryCard({
  title,
  value,
  description,
  icon: Icon,
}: {
  title: string;
  value: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
}) {
  return (
    <Card>
      <CardContent className="flex items-start justify-between gap-3 p-4">
        <div className="space-y-1">
          <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            {title}
          </p>
          <p className="text-2xl font-semibold text-foreground">{value}</p>
          <p className="text-xs text-muted-foreground">{description}</p>
        </div>
        <div className="rounded-sm border border-border bg-background p-2">
          <Icon className="h-5 w-5 text-foreground-secondary" />
        </div>
      </CardContent>
    </Card>
  );
}

function InterfaceSelectionSummary({
  label,
  interfaceRecord,
  emptyMessage,
}: {
  label: string;
  interfaceRecord?: SosInterfaceRecord;
  emptyMessage: string;
}) {
  if (!interfaceRecord) {
    return <p className="text-xs text-muted-foreground">{emptyMessage}</p>;
  }

  return (
    <div className="rounded-sm border border-border bg-background-secondary p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            {label}
          </p>
          <p className="font-medium text-foreground">{interfaceRecord.interface_name}</p>
          <p className="text-xs text-muted-foreground">{interfaceRecord.system_id}</p>
        </div>
        <Badge variant={directionVariant(interfaceRecord.direction)}>
          {interfaceRecord.direction}
        </Badge>
      </div>
      <div className="mt-2 flex flex-wrap gap-2">
        <Badge variant="outline">{interfaceRecord.protocol}</Badge>
        <Badge variant="outline">{interfaceRecord.data_format}</Badge>
        {interfaceRecord.unit_system && <Badge variant="outline">{interfaceRecord.unit_system}</Badge>}
        {interfaceRecord.coordinate_system && (
          <Badge variant="outline">{interfaceRecord.coordinate_system}</Badge>
        )}
      </div>
    </div>
  );
}

function ContractResult({ contract }: { contract: SosDataContract }) {
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="space-y-1">
          <div className="text-base font-semibold text-foreground">{contract.contract_name}</div>
          <div className="text-xs text-muted-foreground">{contract.contract_id}</div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge variant={contract.approved ? 'success' : 'warning'}>
            {contract.approved ? 'Approved' : 'Pending Approval'}
          </Badge>
          <Badge variant={contract.signed ? 'success' : 'outline'}>
            {contract.signed ? 'Signed' : 'Unsigned'}
          </Badge>
        </div>
      </div>

      {contract.description && (
        <p className="text-sm text-muted-foreground">{contract.description}</p>
      )}

      <div className="grid gap-3 md:grid-cols-2">
        <div className="rounded-sm border border-border bg-background-secondary p-3">
          <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            Provider
          </p>
          <p className="mt-1 text-sm text-foreground">{contract.provider_interface_id}</p>
        </div>
        <div className="rounded-sm border border-border bg-background-secondary p-3">
          <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            Consumer
          </p>
          <p className="mt-1 text-sm text-foreground">{contract.consumer_interface_id}</p>
        </div>
      </div>

      <div className="space-y-2">
        <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          SLA Metrics
        </p>
        {contract.sla_metrics.length === 0 ? (
          <p className="text-sm text-muted-foreground">No SLA metrics attached to this contract.</p>
        ) : (
          <div className="grid gap-2">
            {contract.sla_metrics.map((metric) => (
              <div
                key={`${metric.name}:${metric.operator}:${metric.value}`}
                className="flex flex-wrap items-center justify-between gap-2 rounded-sm border border-border bg-background p-3"
              >
                <div className="font-medium text-foreground">{metric.name}</div>
                <div className="text-sm text-muted-foreground">
                  {metric.operator} {metric.value}
                  {metric.unit ? ` ${metric.unit}` : ''}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="space-y-2">
        <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          Transformation Rules
        </p>
        {Object.keys(contract.transformation_rules).length === 0 ? (
          <p className="text-sm text-muted-foreground">No transformation rules defined.</p>
        ) : (
          <pre className="max-h-72 overflow-auto rounded-sm border border-border bg-background p-3 text-xs text-foreground">
            {formatJson(contract.transformation_rules)}
          </pre>
        )}
      </div>

      {contract.tags.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {contract.tags.map((tag) => (
            <Badge key={tag} variant="outline">
              {tag}
            </Badge>
          ))}
        </div>
      )}
    </div>
  );
}

function ValidationResultPanel({
  validation,
  onOpenReports,
}: {
  validation: SosValidationResponse;
  onOpenReports: () => void;
}) {
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <Badge variant={validation.passed ? 'success' : 'destructive'}>
              {validation.passed ? 'Passed' : 'Failed'}
            </Badge>
            <span className="text-sm text-muted-foreground">
              Confidence {formatPercent(validation.confidence)}
            </span>
          </div>
          <div className="text-xs text-muted-foreground">
            Validation id: {validation.validation_id}
          </div>
        </div>
        <div className="text-xs text-muted-foreground">
          Validated {formatTimestamp(validation.validated_at)}
        </div>
      </div>

      {validation.report_id && (
        <div className="rounded-sm border border-border bg-background-secondary p-3 text-sm text-foreground">
          <div>
            Persisted report id: <span className="font-mono">{validation.report_id}</span>
          </div>
          <Button variant="link" className="mt-1 h-auto px-0 py-0 text-sm" onClick={onOpenReports}>
            Inspect persisted report, history, and lineage
          </Button>
        </div>
      )}

      <div className="space-y-3">
        {validation.checks.length === 0 ? (
          <p className="text-sm text-muted-foreground">No detailed checks were returned.</p>
        ) : (
          validation.checks.map((check) => <ValidationCheckRow key={check.check_name} check={check} />)
        )}
      </div>
    </div>
  );
}

function ValidationCheckRow({ check }: { check: SosValidationCheck }) {
  return (
    <div className="rounded-sm border border-border bg-background p-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="space-y-1">
          <div className="font-medium text-foreground">{check.check_name}</div>
          <div className="text-sm text-muted-foreground">{check.description}</div>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge variant={check.passed ? 'success' : 'destructive'}>
            {check.passed ? 'Pass' : 'Fail'}
          </Badge>
          <Badge variant={severityVariant(check.severity)}>{check.severity}</Badge>
        </div>
      </div>
      {check.details !== undefined && check.details !== null && (
        <pre className="mt-3 max-h-64 overflow-auto rounded-sm border border-border bg-background-secondary p-3 text-xs text-foreground">
          {formatJson(check.details)}
        </pre>
      )}
    </div>
  );
}

function CompatibilityRow({
  row,
  onUsePair,
}: {
  row: SosCompatibilityScore;
  onUsePair: (providerId: string, consumerId: string) => void;
}) {
  const incompatibleSignals = row.details.filter((detail) => !detail.compatible);
  const matrixScoreClass =
    row.score < 0.5
      ? 'text-error'
      : row.score < 0.8
        ? 'text-warning'
        : 'text-success';

  return (
    <TableRow>
      <TableCell className="align-top">
        <div className="space-y-1">
          <div className="font-medium text-foreground">{row.provider_interface_id}</div>
        </div>
      </TableCell>
      <TableCell className="align-top">
        <div className="space-y-1">
          <div className="font-medium text-foreground">{row.consumer_interface_id}</div>
        </div>
      </TableCell>
      <TableCell className="align-top">
        <div className={cn('font-semibold', matrixScoreClass)}>{formatPercent(row.score)}</div>
      </TableCell>
      <TableCell className="align-top">
        <div className="space-y-2">
          {incompatibleSignals.length === 0 ? (
            <Badge variant="success">No incompatibilities flagged</Badge>
          ) : (
            incompatibleSignals.map((detail) => (
              <div key={`${row.provider_interface_id}:${detail.aspect}`} className="text-xs">
                <span className="font-semibold text-foreground">{detail.aspect}</span>
                <span className="text-muted-foreground"> - {detail.explanation}</span>
              </div>
            ))
          )}
        </div>
      </TableCell>
      <TableCell className="align-top">
        <Button
          size="sm"
          variant="outline"
          onClick={() => onUsePair(row.provider_interface_id, row.consumer_interface_id)}
        >
          Use Pair
        </Button>
      </TableCell>
    </TableRow>
  );
}

function parseSosTab(rawValue: string | null): SosTab {
  switch (rawValue) {
    case 'reports':
    case 'catalog':
    case 'policies':
    case 'analytics':
    case 'matrix':
    case 'operations':
      return rawValue;
    default:
      return 'workbench';
  }
}

function parseCatalogTab(rawValue: string | null): SosCatalogTab {
  switch (rawValue) {
    case 'interfaces':
    case 'contracts':
      return rawValue;
    default:
      return 'systems';
  }
}

function parseAnalyticsInvestigationState(
  searchParams: URLSearchParams
): SosAnalyticsInvestigationState {
  return {
    ...createDefaultSosAnalyticsInvestigationState(),
    graphLoaded: searchParams.get('analyticsGraph') === '1',
    selectedNodeId: searchParams.get('analyticsNode'),
    selectedEdgeKey: searchParams.get('analyticsEdge'),
    visibleKinds: parseVisibleKinds(searchParams.get('analyticsLanes')),
  };
}

function writeSearchParam(
  searchParams: URLSearchParams,
  key: string,
  value: string | null
): void {
  if (value) {
    searchParams.set(key, value);
    return;
  }

  searchParams.delete(key);
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

function directionVariant(direction: string): 'default' | 'secondary' | 'outline' {
  if (direction === 'Provider') {
    return 'default';
  }
  if (direction === 'Consumer') {
    return 'secondary';
  }
  return 'outline';
}

function severityVariant(
  severity: string
): 'default' | 'success' | 'warning' | 'destructive' | 'outline' {
  if (severity === 'error' || severity === 'critical' || severity === 'high') {
    return 'destructive';
  }
  if (severity === 'warning' || severity === 'medium') {
    return 'warning';
  }
  if (severity === 'info' || severity === 'low') {
    return 'outline';
  }
  return 'default';
}

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function formatTimestamp(value: string | undefined | null): string {
  if (!value) {
    return 'Unknown';
  }

  return format(new Date(value), 'PPpp');
}

function formatJson(value: unknown): string {
  return JSON.stringify(value, null, 2);
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
    'Request failed'
  );
}
