import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  Binary,
  GitBranch,
  Loader2,
  PlayCircle,
  ScanSearch,
  Sparkles,
} from 'lucide-react';

import type {
  SosDependencyGraphEdge,
  SosDependencyGraphNode,
  SosDependencyGraphResponse,
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Textarea } from '@/components/ui/textarea';
import {
  useLookupSosDependencyGraph,
  useRunSosWhatIfAnalysis,
  useSosInterfaces,
  useValidateSosInterfaceSchema,
} from '@/hooks/useSosValidation';
import {
  createDefaultSosAnalyticsInvestigationState,
  normalizeSosAnalyticsInvestigationState,
  type SosAnalyticsInvestigationState,
} from '@/components/sos/sosAnalyticsState';
import type { SosCatalogTab } from '@/components/sos/SosCatalogPanel';
import { SosDependencyGraphView } from '@/components/sos/SosDependencyGraphView';
import {
  buildVisibleKindState,
  extractVisibleKinds,
  getDependencyGraphEdgeKey,
} from '@/components/sos/sosDependencyGraphUtils';

interface ReportsTarget {
  reportId?: string | null;
  subjectType?: string;
  subjectKey?: string;
}

interface CatalogTarget {
  tab: SosCatalogTab;
  systemId?: string | null;
  interfaceId?: string | null;
  contractId?: string | null;
}

interface WhatIfTemplate {
  id: string;
  title: string;
  description: string;
  scenario: string;
  changes: unknown[];
  accent: string;
}

interface SosAnalyticsPanelProps {
  currentPair?: {
    providerInterfaceId: string;
    consumerInterfaceId: string;
  } | null;
  onOpenReports?: (target?: ReportsTarget) => void;
  onOpenCatalog?: (target: CatalogTarget) => void;
  onUsePair?: (providerInterfaceId: string, consumerInterfaceId: string) => void;
  investigationState?: SosAnalyticsInvestigationState;
  onInvestigationStateChange?: (state: SosAnalyticsInvestigationState) => void;
}

export function SosAnalyticsPanel({
  currentPair,
  onOpenReports,
  onOpenCatalog,
  onUsePair,
  investigationState,
  onInvestigationStateChange,
}: SosAnalyticsPanelProps) {
  const {
    data: interfacesData,
    isLoading: isLoadingInterfaces,
    error: interfacesError,
  } = useSosInterfaces();

  const loadDependencyGraph = useLookupSosDependencyGraph();
  const runWhatIfAnalysis = useRunSosWhatIfAnalysis();
  const validateSchema = useValidateSosInterfaceSchema();

  const sortedInterfaces = useMemo(
    () =>
      [...(interfacesData ?? [])].sort((left, right) =>
        left.interface_id.localeCompare(right.interface_id)
      ),
    [interfacesData]
  );
  const interfaceSystemMap = useMemo(
    () =>
      new Map(
        (interfacesData ?? []).map((record) => [record.interface_id, record.system_id] as const)
      ),
    [interfacesData]
  );

  const [graph, setGraph] = useState<SosDependencyGraphResponse | null>(null);
  const [graphError, setGraphError] = useState<string | null>(null);
  const [whatIfError, setWhatIfError] = useState<string | null>(null);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [schemaResult, setSchemaResult] = useState<SosValidationResponse | null>(null);
  const [internalInvestigationState, setInternalInvestigationState] = useState(
    createDefaultSosAnalyticsInvestigationState
  );
  const [whatIfResult, setWhatIfResult] = useState<{
    scenarioId: string;
    impact: string[];
    affectedEntities: string[];
    recommendations: string[];
  } | null>(null);
  const [scenario, setScenario] = useState('Assess a hypothetical SoS catalog change');
  const [changesText, setChangesText] = useState('[\n  {\n    "entity_type": "system",\n    "operation": "delete",\n    "system_id": "sys.example"\n  }\n]');
  const [schemaInterfaceId, setSchemaInterfaceId] = useState(currentPair?.providerInterfaceId ?? '');
  const [schemaPayloadText, setSchemaPayloadText] = useState('{\n  "sample": true\n}');
  const didAutoLoadGraphRef = useRef(false);

  const resolvedInvestigationState = useMemo(
    () =>
      normalizeSosAnalyticsInvestigationState(
        investigationState ?? internalInvestigationState
      ),
    [investigationState, internalInvestigationState]
  );
  const selectedGraphNodeId = resolvedInvestigationState.selectedNodeId;
  const selectedGraphEdgeKey = resolvedInvestigationState.selectedEdgeKey;
  const visibleGraphKinds = useMemo(
    () => buildVisibleKindState(resolvedInvestigationState.visibleKinds),
    [resolvedInvestigationState.visibleKinds]
  );

  const nodeCounts = useMemo(() => countBy(graph?.nodes ?? [], (item) => item.kind), [graph]);
  const edgeCounts = useMemo(() => countBy(graph?.edges ?? [], (item) => item.kind), [graph]);
  const graphNodes = useMemo(() => graph?.nodes ?? [], [graph]);
  const graphEdges = useMemo(() => graph?.edges ?? [], [graph]);
  const selectedGraphNode = useMemo(
    () => graphNodes.find((node) => node.id === selectedGraphNodeId) ?? null,
    [graphNodes, selectedGraphNodeId]
  );
  const selectedGraphEdge = useMemo(
    () =>
      graphEdges.find((edge) => getDependencyGraphEdgeKey(edge) === selectedGraphEdgeKey) ?? null,
    [graphEdges, selectedGraphEdgeKey]
  );
  const selectedGraphEdges = useMemo(() => {
    if (!selectedGraphNodeId) {
      return [];
    }

    return graphEdges.filter(
      (edge) => edge.from === selectedGraphNodeId || edge.to === selectedGraphNodeId
    );
  }, [graphEdges, selectedGraphNodeId]);
  const selectedGraphNodeReportsTarget = useMemo(
    () => (selectedGraphNode ? buildReportsTargetForGraphNode(selectedGraphNode) : null),
    [selectedGraphNode]
  );
  const selectedGraphNodeCatalogTarget = useMemo(
    () => (selectedGraphNode ? buildCatalogTargetForGraphNode(selectedGraphNode) : null),
    [selectedGraphNode]
  );
  const selectedGraphNodeContractContext = useMemo(
    () =>
      selectedGraphNode?.kind === 'contract'
        ? buildContractContextForContractId(selectedGraphNode.id, graphEdges)
        : null,
    [graphEdges, selectedGraphNode]
  );
  const selectedGraphEdgeContractContext = useMemo(
    () =>
      selectedGraphEdge ? buildContractContextForEdge(selectedGraphEdge, graphEdges) : null,
    [graphEdges, selectedGraphEdge]
  );
  const whatIfTemplates = useMemo(
    () =>
      buildWhatIfTemplates({
        currentPair,
        providerSystemId: currentPair
          ? interfaceSystemMap.get(currentPair.providerInterfaceId) ?? null
          : null,
      }),
    [currentPair, interfaceSystemMap]
  );
  const schemaSelectValue = sortedInterfaces.some(
    (item) => item.interface_id === schemaInterfaceId
  )
    ? schemaInterfaceId
    : 'none';

  const updateInvestigationState = useCallback(
    (
      updater:
        | Partial<SosAnalyticsInvestigationState>
        | ((current: SosAnalyticsInvestigationState) => Partial<SosAnalyticsInvestigationState>)
    ) => {
      const currentState = normalizeSosAnalyticsInvestigationState(
        investigationState ?? internalInvestigationState
      );
      const patch = typeof updater === 'function' ? updater(currentState) : updater;
      const nextState = normalizeSosAnalyticsInvestigationState({
        ...currentState,
        ...patch,
      });

      if (onInvestigationStateChange) {
        onInvestigationStateChange(nextState);
        return;
      }

      setInternalInvestigationState(nextState);
    },
    [investigationState, internalInvestigationState, onInvestigationStateChange]
  );

  const handleLoadGraph = useCallback(async () => {
    setGraphError(null);

    try {
      const result = await loadDependencyGraph.mutateAsync();
      setGraph(result);

      updateInvestigationState((current) => {
        const hasSelectedNode =
          Boolean(current.selectedNodeId) &&
          result.nodes.some((node) => node.id === current.selectedNodeId);
        const hasSelectedEdge =
          Boolean(current.selectedEdgeKey) &&
          result.edges.some((edge) => getDependencyGraphEdgeKey(edge) === current.selectedEdgeKey);

        return {
          graphLoaded: true,
          selectedNodeId: hasSelectedNode
            ? current.selectedNodeId
            : hasSelectedEdge
              ? null
              : result.nodes[0]?.id ?? null,
          selectedEdgeKey: hasSelectedEdge ? current.selectedEdgeKey : null,
        };
      });
    } catch (error) {
      setGraphError(getErrorMessage(error));
    }
  }, [loadDependencyGraph, updateInvestigationState]);

  useEffect(() => {
    if (!resolvedInvestigationState.graphLoaded || graph || loadDependencyGraph.isPending) {
      return;
    }

    if (didAutoLoadGraphRef.current) {
      return;
    }

    didAutoLoadGraphRef.current = true;
    void handleLoadGraph();
  }, [
    graph,
    handleLoadGraph,
    loadDependencyGraph.isPending,
    resolvedInvestigationState.graphLoaded,
  ]);

  const handleSeedCurrentPairScenario = () => {
    if (!currentPair) {
      return;
    }

    setScenario('Assess the current validation pair after contract and interface changes');
    setChangesText(
      JSON.stringify(
        [
          {
            entity_type: 'interface',
            operation: 'upsert',
            interface_id: currentPair.providerInterfaceId,
            direction: 'Provider',
            protocol: 'REST',
            data_format: 'JSON',
            schema: {
              type: 'object',
              properties: {
                sample_id: { type: 'string' },
              },
            },
          },
          {
            entity_type: 'contract',
            operation: 'upsert',
            contract_id: `what-if:${currentPair.providerInterfaceId}:${currentPair.consumerInterfaceId}`,
            provider_interface_id: currentPair.providerInterfaceId,
            consumer_interface_id: currentPair.consumerInterfaceId,
            contract_name: 'Hypothetical pair contract',
            approved: true,
          },
        ],
        null,
        2
      )
    );
  };

  const handleApplyWhatIfTemplate = (template: WhatIfTemplate) => {
    setScenario(template.scenario);
    setChangesText(JSON.stringify(template.changes, null, 2));
  };

  const handleRunWhatIf = async () => {
    setWhatIfError(null);

    const parsedChanges = parseJsonArray(changesText, 'what-if changes');
    if (!parsedChanges.ok) {
      setWhatIfError(parsedChanges.error);
      return;
    }

    try {
      const result = await runWhatIfAnalysis.mutateAsync({
        scenario: scenario.trim() || 'Untitled what-if scenario',
        changes: parsedChanges.value,
      });
      setWhatIfResult({
        scenarioId: result.scenario_id,
        impact: result.impact,
        affectedEntities: result.affected_entities,
        recommendations: result.recommendations,
      });
    } catch (error) {
      setWhatIfError(getErrorMessage(error));
    }
  };

  const handleValidateSchema = async () => {
    setSchemaError(null);

    if (!schemaInterfaceId.trim()) {
      setSchemaError('Choose an interface before validating a sample payload.');
      return;
    }

    const payload = parseJsonValue(schemaPayloadText, 'sample payload');
    if (!payload.ok) {
      setSchemaError(payload.error);
      return;
    }

    try {
      const result = await validateSchema.mutateAsync({
        interfaceId: schemaInterfaceId.trim(),
        data: payload.value,
      });
      setSchemaResult(result);
    } catch (error) {
      setSchemaError(getErrorMessage(error));
    }
  };

  return (
    <div className="space-y-4">
      {(interfacesError || graphError || whatIfError || schemaError) && (
        <div className="space-y-3">
          {interfacesError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Interface catalog unavailable</AlertTitle>
              <AlertDescription>{getErrorMessage(interfacesError)}</AlertDescription>
            </Alert>
          )}
          {graphError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Dependency graph load failed</AlertTitle>
              <AlertDescription>{graphError}</AlertDescription>
            </Alert>
          )}
          {whatIfError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>What-if analysis failed</AlertTitle>
              <AlertDescription>{whatIfError}</AlertDescription>
            </Alert>
          )}
          {schemaError && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Schema validation failed</AlertTitle>
              <AlertDescription>{schemaError}</AlertDescription>
            </Alert>
          )}
        </div>
      )}

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr),420px]">
        <Card>
          <CardHeader>
            <div className="flex items-start justify-between gap-3">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <GitBranch className="h-4 w-4" />
                  Dependency Graph
                </CardTitle>
                <CardDescription>
                  Inspect the coordinator-generated SoS graph instead of inferring system relationships from scattered screens.
                </CardDescription>
              </div>
              <Button onClick={handleLoadGraph} disabled={loadDependencyGraph.isPending}>
                {loadDependencyGraph.isPending ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Sparkles className="mr-2 h-4 w-4" />
                )}
                Load Graph
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {graph ? (
              <>
                <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
                  <SummaryCell label="Nodes" value={String(graphNodes.length)} />
                  <SummaryCell label="Edges" value={String(graphEdges.length)} />
                  <SummaryCell label="Systems" value={String(nodeCounts.system ?? 0)} />
                  <SummaryCell label="Contracts" value={String(nodeCounts.contract ?? 0)} />
                </div>

                <div className="grid gap-4 lg:grid-cols-2">
                  <div className="rounded-sm border border-border p-3">
                    <div className="mb-2 text-sm font-medium text-foreground">Node Kinds</div>
                    <div className="flex flex-wrap gap-2">
                      {Object.entries(nodeCounts).map(([key, value]) => (
                        <Badge key={key} variant="outline">
                          {key}: {value}
                        </Badge>
                      ))}
                    </div>
                  </div>
                  <div className="rounded-sm border border-border p-3">
                    <div className="mb-2 text-sm font-medium text-foreground">Edge Kinds</div>
                    <div className="flex flex-wrap gap-2">
                      {Object.entries(edgeCounts).map(([key, value]) => (
                        <Badge key={key} variant="outline">
                          {key}: {value}
                        </Badge>
                      ))}
                    </div>
                  </div>
                </div>

                <SosDependencyGraphView
                  graph={graph}
                  selectedNodeId={selectedGraphNodeId}
                  onSelectNode={(node) => {
                    updateInvestigationState({
                      graphLoaded: true,
                      selectedNodeId: node.id,
                      selectedEdgeKey: null,
                    });
                  }}
                  selectedEdgeKey={selectedGraphEdgeKey}
                  onSelectEdge={(edge) => {
                    updateInvestigationState({
                      graphLoaded: true,
                      selectedNodeId: null,
                      selectedEdgeKey: getDependencyGraphEdgeKey(edge),
                    });
                  }}
                  visibleKinds={visibleGraphKinds}
                  onVisibleKindsChange={(nextVisibleKinds) =>
                    updateInvestigationState({
                      graphLoaded: true,
                      visibleKinds: extractVisibleKinds(nextVisibleKinds),
                    })
                  }
                />

                {(selectedGraphNode || selectedGraphEdge) && (
                  <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr),320px]">
                    <div className="rounded-sm border border-border p-4">
                      {selectedGraphNode ? (
                        <>
                          <div className="flex flex-wrap items-start justify-between gap-3">
                            <div>
                              <div className="text-sm font-medium text-foreground">
                                {selectedGraphNode.label}
                              </div>
                              <div className="font-mono text-xs text-muted-foreground">
                                {selectedGraphNode.id}
                              </div>
                            </div>
                            <Badge variant="outline">{selectedGraphNode.kind}</Badge>
                          </div>

                          <div className="mt-4 grid gap-3 md:grid-cols-2">
                            <MetadataCell label="Node Kind" value={selectedGraphNode.kind} />
                            <MetadataCell
                              label="Connected Edges"
                              value={String(selectedGraphEdges.length)}
                            />
                            {selectedGraphNode.system_id && (
                              <MetadataCell
                                label="System Id"
                                value={selectedGraphNode.system_id}
                                monospace
                              />
                            )}
                            {selectedGraphNode.system_type && (
                              <MetadataCell
                                label="System Type"
                                value={selectedGraphNode.system_type}
                              />
                            )}
                          </div>

                          {selectedGraphEdges.length > 0 && (
                            <div className="mt-4 space-y-2">
                              <div className="text-sm font-medium text-foreground">
                                Connected Edges
                              </div>
                              {selectedGraphEdges.slice(0, 4).map((edge) => (
                                <button
                                  key={getDependencyGraphEdgeKey(edge)}
                                  type="button"
                                  onClick={() => {
                                    updateInvestigationState({
                                      graphLoaded: true,
                                      selectedNodeId: null,
                                      selectedEdgeKey: getDependencyGraphEdgeKey(edge),
                                    });
                                  }}
                                  className="w-full rounded-sm border border-border bg-background p-3 text-left text-xs transition-colors hover:bg-background-secondary"
                                >
                                  <div className="font-medium text-foreground">{edge.kind}</div>
                                  <div className="mt-1 font-mono text-muted-foreground">
                                    {edge.from} -&gt; {edge.to}
                                  </div>
                                  {edge.contract_id && (
                                    <div className="mt-1 font-mono text-muted-foreground">
                                      contract: {edge.contract_id}
                                    </div>
                                  )}
                                </button>
                              ))}
                            </div>
                          )}
                        </>
                      ) : selectedGraphEdge ? (
                        <>
                          <div className="flex flex-wrap items-start justify-between gap-3">
                            <div>
                              <div className="text-sm font-medium text-foreground">
                                Edge Focus
                              </div>
                              <div className="font-mono text-xs text-muted-foreground">
                                {selectedGraphEdge.from} -&gt; {selectedGraphEdge.to}
                              </div>
                            </div>
                            <Badge variant="outline">{selectedGraphEdge.kind}</Badge>
                          </div>

                          <div className="mt-4 grid gap-3 md:grid-cols-2">
                            <MetadataCell label="From" value={selectedGraphEdge.from} monospace />
                            <MetadataCell label="To" value={selectedGraphEdge.to} monospace />
                            <MetadataCell label="Edge Kind" value={selectedGraphEdge.kind} />
                            <MetadataCell
                              label="Contract Id"
                              value={selectedGraphEdge.contract_id ?? 'Derived from graph'}
                              monospace
                            />
                          </div>

                          {selectedGraphEdgeContractContext && (
                            <div className="mt-4 grid gap-3 md:grid-cols-3">
                              <MetadataCell
                                label="Resolved Contract"
                                value={selectedGraphEdgeContractContext.contractId}
                                monospace
                              />
                              <MetadataCell
                                label="Provider Interface"
                                value={
                                  selectedGraphEdgeContractContext.providerInterfaceId ?? 'Unknown'
                                }
                                monospace
                              />
                              <MetadataCell
                                label="Consumer Interface"
                                value={
                                  selectedGraphEdgeContractContext.consumerInterfaceId ?? 'Unknown'
                                }
                                monospace
                              />
                            </div>
                          )}
                        </>
                      ) : null}
                    </div>

                    <div className="rounded-sm border border-border bg-background-secondary p-4">
                      <div className="text-sm font-medium text-foreground">Graph Actions</div>
                      <p className="mt-2 text-sm text-muted-foreground">
                        Move directly from topology inspection into the SoS workbench, reports, or
                        catalog without re-entering ids by hand.
                      </p>

                      {selectedGraphNode && (
                        <div className="mt-4 space-y-3">
                          <div className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                            Node Actions
                          </div>
                          <div className="flex flex-wrap gap-2">
                            {selectedGraphNodeCatalogTarget && onOpenCatalog ? (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() => onOpenCatalog(selectedGraphNodeCatalogTarget)}
                              >
                                Open In Catalog
                              </Button>
                            ) : null}
                            {selectedGraphNodeReportsTarget && onOpenReports ? (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() => onOpenReports(selectedGraphNodeReportsTarget)}
                              >
                                Open Node History
                              </Button>
                            ) : null}
                            {selectedGraphNodeContractContext &&
                            selectedGraphNodeContractContext.providerInterfaceId &&
                            selectedGraphNodeContractContext.consumerInterfaceId &&
                            onUsePair ? (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                  onUsePair(
                                    selectedGraphNodeContractContext.providerInterfaceId!,
                                    selectedGraphNodeContractContext.consumerInterfaceId!
                                  )
                                }
                              >
                                Open Pair In Workbench
                              </Button>
                            ) : null}
                          </div>
                          {!selectedGraphNodeCatalogTarget &&
                            !selectedGraphNodeReportsTarget &&
                            !selectedGraphNodeContractContext && (
                              <Badge variant="secondary">No direct action available for this node</Badge>
                            )}
                        </div>
                      )}

                      {selectedGraphEdge && (
                        <div className="mt-4 space-y-3">
                          <div className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                            Edge Actions
                          </div>
                          <div className="flex flex-wrap gap-2">
                            {selectedGraphEdgeContractContext && onOpenCatalog ? (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                  onOpenCatalog({
                                    tab: 'contracts',
                                    contractId: selectedGraphEdgeContractContext.contractId,
                                  })
                                }
                              >
                                Open Contract In Catalog
                              </Button>
                            ) : null}
                            {selectedGraphEdgeContractContext?.providerInterfaceId &&
                            selectedGraphEdgeContractContext.consumerInterfaceId &&
                            onUsePair ? (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                  onUsePair(
                                    selectedGraphEdgeContractContext.providerInterfaceId!,
                                    selectedGraphEdgeContractContext.consumerInterfaceId!
                                  )
                                }
                              >
                                Open Pair In Workbench
                              </Button>
                            ) : null}
                            {selectedGraphEdgeContractContext?.providerInterfaceId &&
                            selectedGraphEdgeContractContext.consumerInterfaceId &&
                            onOpenReports ? (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                  onOpenReports({
                                    subjectType: 'interface_pair',
                                    subjectKey: buildInterfacePairSubjectKey(
                                      selectedGraphEdgeContractContext.providerInterfaceId!,
                                      selectedGraphEdgeContractContext.consumerInterfaceId!
                                    ),
                                  })
                                }
                              >
                                Open Pair History
                              </Button>
                            ) : null}
                          </div>
                          {!selectedGraphEdgeContractContext && (
                            <Badge variant="secondary">
                              This edge does not resolve to a contract-backed interface pair.
                            </Badge>
                          )}
                        </div>
                      )}
                    </div>
                  </div>
                )}

                <div className="rounded-sm border border-border">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>From</TableHead>
                        <TableHead>To</TableHead>
                        <TableHead>Kind</TableHead>
                        <TableHead>Contract</TableHead>
                        <TableHead className="w-[120px]">Action</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {graphEdges.slice(0, 16).map((edge) => (
                        <TableRow key={getDependencyGraphEdgeKey(edge)}>
                          <TableCell className="font-mono text-xs">{edge.from}</TableCell>
                          <TableCell className="font-mono text-xs">{edge.to}</TableCell>
                          <TableCell>{edge.kind}</TableCell>
                          <TableCell className="font-mono text-xs">{edge.contract_id ?? '—'}</TableCell>
                          <TableCell>
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => {
                                updateInvestigationState({
                                  graphLoaded: true,
                                  selectedNodeId: null,
                                  selectedEdgeKey: getDependencyGraphEdgeKey(edge),
                                });
                              }}
                            >
                              Inspect Edge
                            </Button>
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>
              </>
            ) : (
              <div className="rounded-sm border border-dashed border-border p-4 text-sm text-muted-foreground">
                Load the graph to inspect system, interface, and contract topology.
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Binary className="h-4 w-4" />
              What-If Analysis
            </CardTitle>
            <CardDescription>
              Project catalog changes in-memory and see how the validation surface would shift before touching production state.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <Label>Curated Starters</Label>
                <Badge variant="secondary">
                  {currentPair ? 'Current pair aware' : 'Generic fallbacks'}
                </Badge>
              </div>
              <div className="grid gap-2 md:grid-cols-2">
                {whatIfTemplates.map((template) => (
                  <button
                    key={template.id}
                    type="button"
                    onClick={() => handleApplyWhatIfTemplate(template)}
                    className="rounded-sm border border-border bg-background p-3 text-left transition-colors hover:bg-background-secondary"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <div className="text-sm font-medium text-foreground">{template.title}</div>
                      <span
                        className="h-2.5 w-2.5 rounded-full"
                        style={{ backgroundColor: template.accent }}
                      />
                    </div>
                    <p className="mt-2 text-xs text-muted-foreground">
                      {template.description}
                    </p>
                  </button>
                ))}
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="sos-what-if-scenario">Scenario</Label>
              <Input
                id="sos-what-if-scenario"
                value={scenario}
                onChange={(event) => setScenario(event.target.value)}
                placeholder="Assess a hypothetical topology change"
                autoComplete="off"
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="sos-what-if-changes">Changes JSON Array</Label>
              <Textarea
                id="sos-what-if-changes"
                value={changesText}
                onChange={(event) => setChangesText(event.target.value)}
                rows={14}
                spellCheck={false}
                placeholder='[{"entity_type":"system","operation":"delete","system_id":"sys.example"}]'
              />
              <p className="text-xs text-muted-foreground">
                Supported change kinds are inferred from `entity_type`/`kind` or from keys like `system_id`, `interface_id`, and `contract_id`.
              </p>
            </div>

            <div className="flex flex-wrap gap-2">
              <Button onClick={handleRunWhatIf} disabled={runWhatIfAnalysis.isPending}>
                {runWhatIfAnalysis.isPending ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <PlayCircle className="mr-2 h-4 w-4" />
                )}
                Run Scenario
              </Button>
              {currentPair && (
                <Button variant="outline" onClick={handleSeedCurrentPairScenario}>
                  Seed Current Pair Example
                </Button>
              )}
            </div>

            {whatIfResult && (
              <div className="space-y-4 rounded-sm border border-border p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="secondary">Scenario</Badge>
                  <span className="font-mono text-xs text-muted-foreground">{whatIfResult.scenarioId}</span>
                </div>

                <div>
                  <div className="mb-2 text-sm font-medium text-foreground">Affected Entities</div>
                  <div className="flex flex-wrap gap-2">
                    {whatIfResult.affectedEntities.length > 0 ? (
                      whatIfResult.affectedEntities.map((entity) => (
                        <Badge key={entity} variant="outline">
                          {entity}
                        </Badge>
                      ))
                    ) : (
                      <span className="text-sm text-muted-foreground">No entities were marked as affected.</span>
                    )}
                  </div>
                </div>

                <ResultList title="Impact" items={whatIfResult.impact} />
                <ResultList title="Recommendations" items={whatIfResult.recommendations} />
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <ScanSearch className="h-4 w-4" />
            Interface Schema Probe
          </CardTitle>
          <CardDescription>
            Send a sample payload to the coordinator for interface-specific schema validation and persist the report if the endpoint produces one.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-4 lg:grid-cols-[280px,minmax(0,1fr)]">
            <div className="space-y-2">
              <Label htmlFor="sos-schema-interface">Interface</Label>
              <Select
                value={schemaSelectValue}
                onValueChange={(value) => setSchemaInterfaceId(value === 'none' ? '' : value)}
              >
                <SelectTrigger id="sos-schema-interface">
                  <SelectValue placeholder="Choose an interface" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">Choose an interface</SelectItem>
                  {sortedInterfaces.map((item) => (
                    <SelectItem key={item.interface_id} value={item.interface_id}>
                      {item.interface_id}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                {isLoadingInterfaces
                  ? 'Loading known interfaces...'
                  : 'The request body is passed straight through to the coordinator endpoint.'}
              </p>
            </div>
            <div className="space-y-2">
              <Label htmlFor="sos-schema-payload">Sample Payload JSON</Label>
              <Textarea
                id="sos-schema-payload"
                value={schemaPayloadText}
                onChange={(event) => setSchemaPayloadText(event.target.value)}
                rows={8}
                spellCheck={false}
                placeholder='{"track_id":"abc-123"}'
              />
            </div>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button onClick={handleValidateSchema} disabled={validateSchema.isPending}>
              {validateSchema.isPending ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <PlayCircle className="mr-2 h-4 w-4" />
              )}
              Validate Payload
            </Button>
            {currentPair && (
              <Button variant="outline" onClick={() => setSchemaInterfaceId(currentPair.providerInterfaceId)}>
                Use Current Provider
              </Button>
            )}
          </div>

          {schemaResult && (
            <SchemaValidationResult
              result={schemaResult}
              interfaceId={schemaInterfaceId}
              onOpenReports={onOpenReports}
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function SummaryCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-sm border border-border bg-background-secondary p-3">
      <div className="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
      <div className="mt-1 text-xl font-semibold text-foreground">{value}</div>
    </div>
  );
}

function ResultList({ title, items }: { title: string; items: string[] }) {
  return (
    <div>
      <div className="mb-2 text-sm font-medium text-foreground">{title}</div>
      <div className="space-y-2">
        {items.map((item, index) => (
          <div key={`${title}-${index}`} className="rounded-sm border border-border bg-background-secondary p-3 text-sm text-foreground-secondary">
            {item}
          </div>
        ))}
      </div>
    </div>
  );
}

function MetadataCell({
  label,
  value,
  monospace = false,
}: {
  label: string;
  value: string;
  monospace?: boolean;
}) {
  return (
    <div className="rounded-sm border border-border bg-background p-3">
      <div className="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
      <div
        className={
          monospace
            ? 'mt-1 font-mono text-sm text-foreground'
            : 'mt-1 text-sm text-foreground'
        }
      >
        {value}
      </div>
    </div>
  );
}

function buildReportsTargetForGraphNode(node: SosDependencyGraphNode): ReportsTarget | null {
  if (node.kind === 'interface') {
    return {
      subjectType: 'interface',
      subjectKey: `interface:${node.id}`,
    };
  }

  if (node.kind === 'contract') {
    return {
      subjectType: 'contract',
      subjectKey: `contract:${node.id}`,
    };
  }

  return null;
}

function buildCatalogTargetForGraphNode(node: SosDependencyGraphNode): CatalogTarget | null {
  if (node.kind === 'system') {
    return {
      tab: 'systems',
      systemId: node.system_id ?? node.id,
    };
  }

  if (node.kind === 'interface') {
    return {
      tab: 'interfaces',
      interfaceId: node.id,
    };
  }

  if (node.kind === 'contract') {
    return {
      tab: 'contracts',
      contractId: node.id,
    };
  }

  return null;
}

interface ContractGraphContext {
  contractId: string;
  providerInterfaceId: string | null;
  consumerInterfaceId: string | null;
}

function buildContractContextForEdge(
  edge: SosDependencyGraphEdge,
  edges: SosDependencyGraphEdge[]
): ContractGraphContext | null {
  const contractId = resolveContractIdForEdge(edge);
  if (!contractId) {
    return null;
  }

  return buildContractContextForContractId(contractId, edges);
}

function buildContractContextForContractId(
  contractId: string,
  edges: SosDependencyGraphEdge[]
): ContractGraphContext | null {
  if (!contractId) {
    return null;
  }

  const providerEdge =
    edges.find((entry) => entry.kind === 'governs_provider' && entry.to === contractId) ?? null;
  const consumerEdge =
    edges.find((entry) => entry.kind === 'governs_consumer' && entry.from === contractId) ?? null;

  return {
    contractId,
    providerInterfaceId: providerEdge?.from ?? null,
    consumerInterfaceId: consumerEdge?.to ?? null,
  };
}

function resolveContractIdForEdge(edge: SosDependencyGraphEdge): string | null {
  if (edge.contract_id) {
    return edge.contract_id;
  }

  if (edge.kind === 'governs_provider') {
    return edge.to;
  }

  if (edge.kind === 'governs_consumer') {
    return edge.from;
  }

  return null;
}

function buildWhatIfTemplates(
  params: {
    currentPair?: {
      providerInterfaceId: string;
      consumerInterfaceId: string;
    } | null;
    providerSystemId?: string | null;
  }
): WhatIfTemplate[] {
  const providerInterfaceId =
    params.currentPair?.providerInterfaceId ?? 'iface.provider.example';
  const consumerInterfaceId =
    params.currentPair?.consumerInterfaceId ?? 'iface.consumer.example';
  const providerSystemId = params.providerSystemId ?? 'sys.provider.example';

  return [
    {
      id: 'provider-schema-tightening',
      title: 'Tighten Provider Schema',
      description:
        'Simulate a provider-side schema tightening and contract update before the change lands.',
      scenario: 'Assess provider schema tightening against downstream consumers',
      accent: '#0f766e',
      changes: [
        {
          entity_type: 'interface',
          operation: 'upsert',
          interface_id: providerInterfaceId,
          direction: 'Provider',
          protocol: 'REST',
          data_format: 'JSON',
          schema: {
            type: 'object',
            required: ['sample_id', 'event_timestamp'],
            properties: {
              sample_id: { type: 'string' },
              event_timestamp: { type: 'string', format: 'date-time' },
              status: { type: 'string', enum: ['ready', 'processing', 'complete'] },
            },
          },
        },
        {
          entity_type: 'contract',
          operation: 'upsert',
          contract_id: `what-if:${providerInterfaceId}:${consumerInterfaceId}`,
          provider_interface_id: providerInterfaceId,
          consumer_interface_id: consumerInterfaceId,
          contract_name: 'Provider schema tightening review',
          approved: true,
        },
      ],
    },
    {
      id: 'consumer-format-drift',
      title: 'Introduce Consumer Drift',
      description:
        'Model a consumer-side protocol and format drift to see whether the pair still validates cleanly.',
      scenario: 'Assess consumer drift away from the current provider contract',
      accent: '#d97706',
      changes: [
        {
          entity_type: 'interface',
          operation: 'upsert',
          interface_id: consumerInterfaceId,
          direction: 'Consumer',
          protocol: 'gRPC',
          data_format: 'Protobuf',
          schema: {
            type: 'object',
            properties: {
              sample_id: { type: 'string' },
              event_timestamp: { type: 'integer' },
            },
          },
        },
      ],
    },
    {
      id: 'retire-provider-system',
      title: 'Retire Provider System',
      description:
        'Trace the impact of removing the provider system and the contract edges it anchors.',
      scenario: 'Assess provider system retirement and downstream contract fallout',
      accent: '#2563eb',
      changes: [
        {
          entity_type: 'system',
          operation: 'delete',
          system_id: providerSystemId,
        },
        {
          entity_type: 'contract',
          operation: 'delete',
          contract_id: `what-if:${providerInterfaceId}:${consumerInterfaceId}`,
        },
      ],
    },
    {
      id: 'parallel-consumer',
      title: 'Add Parallel Consumer',
      description:
        'Preview the blast radius of onboarding another consumer against the same provider surface.',
      scenario: 'Assess introducing a parallel consumer for the current provider',
      accent: '#7c3aed',
      changes: [
        {
          entity_type: 'interface',
          operation: 'upsert',
          interface_id: `${consumerInterfaceId}.candidate`,
          direction: 'Consumer',
          protocol: 'REST',
          data_format: 'JSON',
          schema: {
            type: 'object',
            properties: {
              sample_id: { type: 'string' },
              requested_view: { type: 'string' },
            },
          },
        },
        {
          entity_type: 'contract',
          operation: 'upsert',
          contract_id: `what-if:${providerInterfaceId}:${consumerInterfaceId}.candidate`,
          provider_interface_id: providerInterfaceId,
          consumer_interface_id: `${consumerInterfaceId}.candidate`,
          contract_name: 'Parallel consumer onboarding review',
          approved: false,
        },
      ],
    },
  ];
}

function SchemaValidationResult({
  result,
  interfaceId,
  onOpenReports,
}: {
  result: SosValidationResponse;
  interfaceId: string;
  onOpenReports?: (target?: ReportsTarget) => void;
}) {
  return (
    <div className="space-y-3 rounded-sm border border-border p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-sm font-medium text-foreground">Schema Validation Result</div>
          <p className="text-xs text-muted-foreground">Validation id {result.validation_id}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge variant={result.passed ? 'default' : 'destructive'}>
            {result.passed ? 'Passed' : 'Failed'}
          </Badge>
          <Badge variant="outline">Confidence {formatPercent(result.confidence)}</Badge>
        </div>
      </div>

      <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
        {result.checks.map((check) => (
          <div key={check.check_name} className="rounded-sm border border-border bg-background-secondary p-3">
            <div className="flex items-center justify-between gap-2">
              <div className="text-sm font-medium text-foreground">{check.check_name}</div>
              <Badge variant={check.passed ? 'outline' : 'secondary'}>{check.severity}</Badge>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">{check.description}</p>
          </div>
        ))}
      </div>

      {result.report_id && onOpenReports && (
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            onOpenReports({
              reportId: result.report_id,
              subjectType: 'interface',
              subjectKey: `interface:${interfaceId}`,
            })
          }
        >
          Open Persisted Report
        </Button>
      )}
    </div>
  );
}

function parseJsonArray(
  value: string,
  label: string
): { ok: true; value: unknown[] } | { ok: false; error: string } {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!Array.isArray(parsed)) {
      return { ok: false, error: `${label} must be a JSON array.` };
    }
    return { ok: true, value: parsed };
  } catch {
    return { ok: false, error: `${label} must be valid JSON.` };
  }
}

function parseJsonValue(
  value: string,
  label: string
): { ok: true; value: unknown } | { ok: false; error: string } {
  try {
    return { ok: true, value: JSON.parse(value) as unknown };
  } catch {
    return { ok: false, error: `${label} must be valid JSON.` };
  }
}

function countBy<T>(items: T[], getKey: (item: T) => string): Record<string, number> {
  return items.reduce<Record<string, number>>((counts, item) => {
    const key = getKey(item);
    counts[key] = (counts[key] ?? 0) + 1;
    return counts;
  }, {});
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

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}
