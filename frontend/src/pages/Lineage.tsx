/**
 * Data Lineage Page
 * Workflow-aware lineage explorer for execution activity and row journeys.
 */

import React, { useEffect, useMemo, useState } from 'react';
import { format } from 'date-fns';
import {
  AlertCircle,
  Clock,
  Database,
  GitBranch,
  Loader2,
  Search,
} from 'lucide-react';

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
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { LineageGraph } from '@/components/lineage/LineageGraph';
import { LineageNodeDetails } from '@/components/lineage/LineageNodeDetails';
import { RowKeyTypeahead } from '@/components/lineage/RowKeyTypeahead';
import { LineageTimeSlider } from '@/components/lineage/LineageTimeSlider';
import { useRowJourney, useRowLineage, useRunLineage } from '@/hooks/useLineage';
import type { LineageGraph as LineageGraphModel, LineageNode } from '@/hooks/useLineageGraph';
import { useExecutions } from '@/hooks/useWorkflows';
import {
  buildRowJourneyGraph,
  formatRowEventSummary,
  formatRowIdentity,
  getLatestRunLineageEvents,
} from '@/lib/workflow-lineage-adapter';
import type { RowKeySearchMatch, WorkflowExecutionSummary } from '@/api/types';

const EMPTY_GRAPH: LineageGraphModel = {
  nodes: [],
  edges: [],
  metadata: {
    totalEvents: 0,
    dateRange: {
      start: new Date().toISOString(),
      end: new Date().toISOString(),
    },
    datasets: new Set<string>(),
    models: new Set<string>(),
  },
};

const ORACLE_DEMO_ROW_KEY = 'oracle:CUSTOMER_FEED:STAGE_ROW_ID=FEED001';

function formatTimestamp(value: string | undefined | null) {
  if (!value) return 'Unknown';
  return format(new Date(value), 'PPpp');
}

function formatErrorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  return 'Unable to load lineage data';
}

function getExecutionStatusVariant(execution: WorkflowExecutionSummary) {
  return execution.success ? 'success' : 'destructive';
}

export function Lineage() {
  const [activeTab, setActiveTab] = useState<'row' | 'run'>('row');
  const [draftRowKey, setDraftRowKey] = useState('');
  const [selectedRowKey, setSelectedRowKey] = useState<string | undefined>(undefined);
  const [draftRunId, setDraftRunId] = useState('');
  const [selectedRunId, setSelectedRunId] = useState<string | undefined>(undefined);
  const [selectedNode, setSelectedNode] = useState<LineageNode | null>(null);
  const [selectedTime, setSelectedTime] = useState<string | undefined>(undefined);

  const { data: executionsResponse, isLoading: isLoadingExecutions } = useExecutions({
    limit: 12,
  });
  const recentExecutions = executionsResponse?.executions || [];

  const {
    data: rowJourney,
    isLoading: isLoadingJourney,
    error: rowJourneyError,
  } = useRowJourney(selectedRowKey, { format: 'graph' }, !!selectedRowKey);

  const {
    data: rowLineage,
    isLoading: isLoadingRowLineage,
    error: rowLineageError,
  } = useRowLineage(selectedRowKey, !!selectedRowKey);

  const {
    data: runLineage,
    isLoading: isLoadingRunLineage,
    error: runLineageError,
  } = useRunLineage(selectedRunId, undefined, !!selectedRunId);

  useEffect(() => {
    setSelectedNode(null);
    setSelectedTime(undefined);
  }, [selectedRowKey]);

  const rowGraph = useMemo(() => {
    if (!rowJourney) return EMPTY_GRAPH;
    return buildRowJourneyGraph(rowJourney);
  }, [rowJourney]);

  const filteredRowGraph = useMemo(() => {
    if (!selectedTime || rowGraph.nodes.length === 0) return rowGraph;

    const filteredEdges = rowGraph.edges.filter((edge) => edge.timestamp <= selectedTime);
    const nodeIds = new Set<string>();
    filteredEdges.forEach((edge) => {
      nodeIds.add(edge.source);
      nodeIds.add(edge.target);
    });

    if (rowGraph.nodes[0]) {
      nodeIds.add(rowGraph.nodes[0].id);
    }

    return {
      ...rowGraph,
      nodes: rowGraph.nodes.filter((node) => nodeIds.has(node.id)),
      edges: filteredEdges,
    };
  }, [rowGraph, selectedTime]);

  const relatedEdges = useMemo(() => {
    if (!selectedNode) return [];
    return filteredRowGraph.edges.filter(
      (edge) => edge.source === selectedNode.id || edge.target === selectedNode.id
    );
  }, [filteredRowGraph.edges, selectedNode]);

  const runEvents = useMemo(() => getLatestRunLineageEvents(runLineage), [runLineage]);

  const handleRowSearch = () => {
    const nextRowKey = draftRowKey.trim();
    if (!nextRowKey) return;
    setSelectedRowKey(nextRowKey);
    setActiveTab('row');
  };

  const handleRunSearch = () => {
    const nextRunId = draftRunId.trim();
    if (!nextRunId) return;
    setSelectedRunId(nextRunId);
    setActiveTab('run');
  };

  return (
    <div className="space-y-4 pb-8">
      <div className="space-y-2">
        <h1 className="text-2xl font-semibold text-foreground">Data Lineage</h1>
        <p className="text-sm text-muted-foreground">
          Explore workflow execution activity and trace individual rows across transformation,
          deduplication, and load steps.
        </p>
      </div>

      <Tabs
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as 'row' | 'run')}
        className="space-y-4"
      >
        <TabsList>
          <TabsTrigger value="row">Row Journey</TabsTrigger>
          <TabsTrigger value="run">Run Activity</TabsTrigger>
        </TabsList>

        <TabsContent value="row" className="space-y-4">
          <div className="grid gap-4 xl:grid-cols-[360px,minmax(0,1fr)]">
            <div className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <GitBranch className="h-4 w-4" />
                    Trace A Workflow Row
                  </CardTitle>
                  <CardDescription>
                    Enter a row key from workflow lineage. For the Oracle demo, try
                    {' '}
                    <span className="font-mono">{ORACLE_DEMO_ROW_KEY}</span>.
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  <RowKeyTypeahead
                    value={draftRowKey}
                    onChange={setDraftRowKey}
                    onSelect={(match: RowKeySearchMatch) => {
                      setDraftRowKey(match.row_key);
                      setSelectedRowKey(match.row_key);
                      setActiveTab('row');
                    }}
                    onSubmit={(rowKey) => {
                      setDraftRowKey(rowKey);
                      setSelectedRowKey(rowKey);
                      setActiveTab('row');
                    }}
                    placeholder="oracle:CUSTOMER_FEED:STAGE_ROW_ID=FEED001"
                  />
                  <div className="flex gap-2">
                    <Button onClick={handleRowSearch} className="gap-2">
                      <Search className="h-4 w-4" />
                      View Journey
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() => {
                        setDraftRowKey(ORACLE_DEMO_ROW_KEY);
                        setSelectedRowKey(ORACLE_DEMO_ROW_KEY);
                      }}
                    >
                      Load Demo Row
                    </Button>
                  </div>
                </CardContent>
              </Card>

              {selectedRowKey && (
                <Card>
                  <CardHeader>
                    <CardTitle>Row Summary</CardTitle>
                    <CardDescription className="font-mono break-all">
                      {selectedRowKey}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3 text-sm">
                    <div className="flex items-center justify-between">
                      <span className="text-muted-foreground">Journey steps</span>
                      <Badge variant="outline">
                        {rowJourney?.steps?.length ?? rowLineage?.total_count ?? 0}
                      </Badge>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-muted-foreground">Total duration</span>
                      <span>{rowJourney?.total_duration_ms ?? 0} ms</span>
                    </div>
                    {rowJourney?.destination && (
                      <div className="space-y-1">
                        <div className="text-muted-foreground">Destination</div>
                        <div className="font-mono break-all text-xs">
                          {formatRowIdentity(rowJourney.destination)}
                        </div>
                      </div>
                    )}
                  </CardContent>
                </Card>
              )}

              {selectedRowKey && (
                <Card>
                  <CardHeader>
                    <CardTitle>Row Events</CardTitle>
                    <CardDescription>
                      Raw event history for the selected row.
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3 max-h-[420px] overflow-auto">
                    {isLoadingRowLineage && (
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Loader2 className="h-4 w-4 animate-spin" />
                        Loading row events...
                      </div>
                    )}

                    {rowLineageError && (
                      <div className="rounded-sm border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
                        {formatErrorMessage(rowLineageError)}
                      </div>
                    )}

                    {!isLoadingRowLineage && !rowLineageError && (rowLineage?.events?.length || 0) === 0 && (
                      <div className="text-sm text-muted-foreground">
                        No row-level events were returned for this key.
                      </div>
                    )}

                    {(rowLineage?.events || []).map((event, index) => (
                      <div key={`${event.batch_id}-${event.timestamp}-${index}`} className="rounded-sm border border-border p-3 space-y-2">
                        <div className="flex items-start justify-between gap-3">
                          <div className="text-sm font-medium">{formatRowEventSummary(event)}</div>
                          <Badge variant="outline">{event.job_id}</Badge>
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {formatTimestamp(event.timestamp)}
                        </div>
                        <div className="text-xs text-muted-foreground">
                          Batch {event.batch_id}
                        </div>
                      </div>
                    ))}
                  </CardContent>
                </Card>
              )}
            </div>

            <div className="space-y-4">
              <Card className="min-h-[620px]">
                <CardContent className="p-0 h-[620px] relative">
                  {!selectedRowKey && (
                    <div className="absolute inset-0 flex items-center justify-center bg-background">
                      <div className="text-center max-w-md px-6">
                        <Database className="h-12 w-12 mx-auto mb-3 text-muted-foreground opacity-60" />
                        <p className="text-lg font-semibold text-foreground">Trace a real workflow row</p>
                        <p className="text-sm text-muted-foreground mt-2">
                          Enter a row key from workflow processing to see its journey across source,
                          transformation, deduplication, and destination steps.
                        </p>
                      </div>
                    </div>
                  )}

                  {selectedRowKey && isLoadingJourney && (
                    <div className="absolute inset-0 flex items-center justify-center bg-background">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Loader2 className="h-5 w-5 animate-spin" />
                        Loading row journey...
                      </div>
                    </div>
                  )}

                  {selectedRowKey && rowJourneyError && (
                    <div className="absolute inset-0 flex items-center justify-center bg-background">
                      <div className="max-w-md text-center px-6">
                        <AlertCircle className="h-12 w-12 mx-auto mb-3 text-destructive" />
                        <p className="text-lg font-semibold text-foreground">Unable to load row journey</p>
                        <p className="text-sm text-muted-foreground mt-2">
                          {formatErrorMessage(rowJourneyError)}
                        </p>
                      </div>
                    </div>
                  )}

                  {selectedRowKey && !isLoadingJourney && !rowJourneyError && (
                    <LineageGraph
                      graph={filteredRowGraph}
                      onNodeClick={setSelectedNode}
                      selectedNodeId={selectedNode?.id}
                      className="h-full"
                    />
                  )}
                </CardContent>
              </Card>

              {selectedRowKey && !isLoadingJourney && !rowJourneyError && rowGraph.nodes.length > 0 && (
                <LineageTimeSlider
                  dateRange={rowGraph.metadata.dateRange}
                  selectedTime={selectedTime || rowGraph.metadata.dateRange.end}
                  onTimeChange={setSelectedTime}
                  totalEvents={rowGraph.metadata.totalEvents}
                />
              )}

              <LineageNodeDetails
                node={selectedNode}
                relatedEdges={relatedEdges}
              />
            </div>
          </div>
        </TabsContent>

        <TabsContent value="run" className="space-y-4">
          <div className="grid gap-4 xl:grid-cols-[360px,minmax(0,1fr)]">
            <div className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <Clock className="h-4 w-4" />
                    Explore Workflow Runs
                  </CardTitle>
                  <CardDescription>
                    Pick a recent execution or paste an execution id to inspect run-level lineage.
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  <Input
                    value={draftRunId}
                    onChange={(event) => setDraftRunId(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault();
                        handleRunSearch();
                      }
                    }}
                    placeholder="exec_97351606-ed1a-41aa-a173-cb58f1d56d3b"
                  />
                  <Button onClick={handleRunSearch} className="gap-2">
                    <Search className="h-4 w-4" />
                    View Run Activity
                  </Button>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle>Recent Executions</CardTitle>
                  <CardDescription>
                    The latest workflow runs visible to this environment.
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 max-h-[520px] overflow-auto">
                  {isLoadingExecutions && (
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Loader2 className="h-4 w-4 animate-spin" />
                      Loading executions...
                    </div>
                  )}

                  {!isLoadingExecutions && recentExecutions.length === 0 && (
                    <div className="text-sm text-muted-foreground">
                      No recent executions were returned.
                    </div>
                  )}

                  {recentExecutions.map((execution) => (
                    <button
                      key={execution.execution_id}
                      type="button"
                      onClick={() => {
                        setDraftRunId(execution.execution_id);
                        setSelectedRunId(execution.execution_id);
                      }}
                      className="w-full rounded-sm border border-border p-3 text-left hover:bg-muted/40 transition-colors"
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div>
                          <div className="text-sm font-medium break-all">
                            {execution.execution_id}
                          </div>
                          <div className="text-xs text-muted-foreground mt-1">
                            {execution.workflow_id}
                          </div>
                        </div>
                        <Badge variant={getExecutionStatusVariant(execution)}>
                          {execution.success ? 'completed' : 'failed'}
                        </Badge>
                      </div>
                      <div className="text-xs text-muted-foreground mt-2">
                        {formatTimestamp(execution.started_at)}
                      </div>
                    </button>
                  ))}
                </CardContent>
              </Card>
            </div>

            <div className="space-y-4">
              {!selectedRunId && (
                <Card>
                  <CardContent className="py-16 text-center">
                    <GitBranch className="h-12 w-12 mx-auto mb-3 text-muted-foreground opacity-60" />
                    <p className="text-lg font-semibold text-foreground">Select a workflow run</p>
                    <p className="text-sm text-muted-foreground mt-2">
                      Run activity shows the execution-level lineage records emitted while a workflow runs.
                    </p>
                  </CardContent>
                </Card>
              )}

              {selectedRunId && (
                <>
                  <Card>
                    <CardHeader>
                      <CardTitle>Run Summary</CardTitle>
                      <CardDescription className="font-mono break-all">
                        {selectedRunId}
                      </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3">
                      {isLoadingRunLineage && (
                        <div className="flex items-center gap-2 text-sm text-muted-foreground">
                          <Loader2 className="h-4 w-4 animate-spin" />
                          Loading run lineage...
                        </div>
                      )}

                      {runLineageError && (
                        <div className="rounded-sm border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
                          {formatErrorMessage(runLineageError)}
                        </div>
                      )}

                      {!isLoadingRunLineage && !runLineageError && runLineage && (
                        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                          <div className="rounded-sm border border-border p-3">
                            <div className="text-xs text-muted-foreground">Records</div>
                            <div className="text-lg font-semibold">{runLineage.total_records || 0}</div>
                          </div>
                          <div className="rounded-sm border border-border p-3">
                            <div className="text-xs text-muted-foreground">Datasets</div>
                            <div className="text-lg font-semibold">
                              {runLineage.datasets?.length || 0}
                            </div>
                          </div>
                          <div className="rounded-sm border border-border p-3">
                            <div className="text-xs text-muted-foreground">Started</div>
                            <div className="text-sm font-medium">
                              {formatTimestamp(runLineage.start_time)}
                            </div>
                          </div>
                          <div className="rounded-sm border border-border p-3">
                            <div className="text-xs text-muted-foreground">Ended</div>
                            <div className="text-sm font-medium">
                              {formatTimestamp(runLineage.end_time)}
                            </div>
                          </div>
                        </div>
                      )}
                    </CardContent>
                  </Card>

                  <Card>
                    <CardHeader>
                      <CardTitle>Execution Events</CardTitle>
                      <CardDescription>
                        Workflow and step-level lineage records for this run.
                      </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3 max-h-[620px] overflow-auto">
                      {!isLoadingRunLineage && !runLineageError && runEvents.length === 0 && (
                        <div className="text-sm text-muted-foreground">
                          No execution events were returned for this run.
                        </div>
                      )}

                      {runEvents.map((event) => (
                        <div key={`${event.record_id}-${event.timestamp}`} className="rounded-sm border border-border p-3 space-y-2">
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <div className="text-sm font-medium break-all">{event.record_id}</div>
                              <div className="text-xs text-muted-foreground mt-1">
                                {event.dataset}
                              </div>
                            </div>
                            <Badge variant="outline">{event.metadata.event_kind || 'event'}</Badge>
                          </div>
                          <div className="text-xs text-muted-foreground">
                            {formatTimestamp(event.timestamp)}
                          </div>
                          {event.sources[0] && (
                            <div className="text-xs text-muted-foreground break-all">
                              Source: {event.sources[0].system} • {event.sources[0].path}
                            </div>
                          )}
                          <div className="text-xs text-muted-foreground break-all">
                            Output: {event.output.system} • {event.output.path}
                          </div>
                          {event.transforms.length > 0 && (
                            <div className="flex flex-wrap gap-2 pt-1">
                              {event.transforms.map((transform) => (
                                <Badge key={transform.id} variant="secondary">
                                  {transform.transform_type}
                                </Badge>
                              ))}
                            </div>
                          )}
                        </div>
                      ))}
                    </CardContent>
                  </Card>
                </>
              )}
            </div>
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}
