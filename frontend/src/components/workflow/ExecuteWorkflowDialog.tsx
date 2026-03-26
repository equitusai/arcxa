import { useEffect, useMemo, useState } from 'react';
import { Database, Loader2, Play, TableProperties } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { useDatasets } from '@/hooks/useDatasets';
import type { Dataset, WorkflowExecutionRequest } from '@/api/types';
import { toast } from 'sonner';

const DEFAULT_JSON_INPUT = '{\n  "data": "sample input"\n}';

interface ExecuteWorkflowDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workflowName: string;
  supportsInputlessExecution?: boolean;
  isExecuting?: boolean;
  onExecute: (request: WorkflowExecutionRequest) => Promise<void>;
}

function parsePositiveInt(value: string): number | undefined {
  const trimmed = value.trim();
  if (!trimmed) {
    return undefined;
  }

  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error('Numeric execution options must be positive integers');
  }

  return parsed;
}

function formatDatasetMeta(dataset: Dataset) {
  const parts = [`${dataset.record_count.toLocaleString()} rows`];

  if (dataset.dataset_type) {
    parts.push(dataset.dataset_type.replace(/_/g, ' '));
  }

  if (dataset.source_name) {
    parts.push(dataset.source_name);
  }

  return parts.join(' · ');
}

export function ExecuteWorkflowDialog({
  open,
  onOpenChange,
  workflowName,
  supportsInputlessExecution = false,
  isExecuting = false,
  onExecute,
}: ExecuteWorkflowDialogProps) {
  const [inputMode, setInputMode] = useState<'none' | 'json' | 'dataset'>(
    supportsInputlessExecution ? 'none' : 'json'
  );
  const [jsonInput, setJsonInput] = useState(DEFAULT_JSON_INPUT);
  const [selectedDatasetId, setSelectedDatasetId] = useState('');
  const [batchSize, setBatchSize] = useState('1000');
  const [limit, setLimit] = useState('');
  const [materializeOutput, setMaterializeOutput] = useState(false);
  const [outputDatasetName, setOutputDatasetName] = useState('');

  const { data: datasetsResponse, isLoading: isLoadingDatasets } = useDatasets({
    datasetScope: 'materialized',
  });

  const datasets = useMemo(() => datasetsResponse?.datasets ?? [], [datasetsResponse?.datasets]);
  const selectedDataset = useMemo(
    () => datasets.find((dataset) => dataset.id === selectedDatasetId),
    [datasets, selectedDatasetId]
  );

  useEffect(() => {
    if (!open) {
      return;
    }

    setInputMode((currentMode) => {
      if (supportsInputlessExecution) {
        return currentMode === 'json' || currentMode === 'dataset' ? currentMode : 'none';
      }

      return currentMode === 'none' ? 'json' : currentMode;
    });
  }, [open, supportsInputlessExecution]);

  useEffect(() => {
    if (!open || inputMode !== 'dataset' || selectedDatasetId || datasets.length === 0) {
      return;
    }

    setSelectedDatasetId(datasets[0].id);
  }, [open, inputMode, selectedDatasetId, datasets]);

  const handleExecute = async () => {
    try {
      if (inputMode === 'dataset' && !selectedDatasetId) {
        throw new Error('Select a materialized dataset to execute this workflow');
      }

      const request: WorkflowExecutionRequest =
        inputMode === 'dataset'
          ? {
              input: {
                type: 'dataset',
                dataset_id: selectedDatasetId,
                batch_size: parsePositiveInt(batchSize),
                limit: parsePositiveInt(limit),
              },
            }
          : inputMode === 'none'
          ? {
              input: {
                type: 'json',
                data: null,
              },
            }
          : {
              input: {
                type: 'json',
                data: JSON.parse(jsonInput),
              },
            };

      if (materializeOutput) {
        request.output_dataset = {
          name: outputDatasetName.trim() || undefined,
        };
      }

      await onExecute(request);
      onOpenChange(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Failed to execute workflow');
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Execute Workflow</DialogTitle>
          <DialogDescription>
            Run {workflowName || 'this workflow'} from source-driven execution, a JSON payload,
            or a materialized dataset, and optionally write the final rows back into the
            catalogue.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          <Card>
            <CardHeader className="pb-4">
              <CardTitle className="text-base">Input Source</CardTitle>
              <CardDescription>
                JSON is best for ad hoc testing. Dataset mode reuses a materialized catalogue
                dataset as the workflow input stream.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="workflow-input-mode">Input mode</Label>
                <Select
                  value={inputMode}
                  onValueChange={(value: 'none' | 'json' | 'dataset') => setInputMode(value)}
                >
                  <SelectTrigger id="workflow-input-mode">
                    <SelectValue placeholder="Select execution input" />
                  </SelectTrigger>
                  <SelectContent>
                    {supportsInputlessExecution && (
                      <SelectItem value="none">No external input</SelectItem>
                    )}
                    <SelectItem value="json">JSON payload</SelectItem>
                    <SelectItem value="dataset">Materialized dataset</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {inputMode === 'none' ? (
                <div className="rounded-sm border border-border bg-muted/30 p-4 space-y-2">
                  <div className="flex items-center gap-2">
                    <Database className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">Source-driven execution</span>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    This workflow includes source steps that fetch their own data, so ARCXA will
                    execute it without requiring an external JSON payload.
                  </p>
                </div>
              ) : inputMode === 'json' ? (
                <div className="space-y-2">
                  <Label htmlFor="workflow-json-input">Input JSON</Label>
                  <Textarea
                    id="workflow-json-input"
                    value={jsonInput}
                    onChange={(event) => setJsonInput(event.target.value)}
                    className="font-mono text-sm h-44"
                    placeholder={DEFAULT_JSON_INPUT}
                  />
                  <p className="text-xs text-muted-foreground">
                    This is sent as workflow input without persisting it to the catalogue.
                  </p>
                </div>
              ) : (
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="workflow-dataset-select">Dataset</Label>
                    <Select
                      value={selectedDatasetId}
                      onValueChange={setSelectedDatasetId}
                      disabled={isLoadingDatasets || datasets.length === 0}
                    >
                      <SelectTrigger id="workflow-dataset-select">
                        <SelectValue
                          placeholder={
                            isLoadingDatasets
                              ? 'Loading datasets...'
                              : datasets.length === 0
                              ? 'No materialized datasets available'
                              : 'Select a dataset'
                          }
                        />
                      </SelectTrigger>
                      <SelectContent>
                        {datasets.map((dataset) => (
                          <SelectItem key={dataset.id} value={dataset.id}>
                            {dataset.name}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>

                  {selectedDataset && (
                    <div className="rounded-sm border border-border bg-muted/30 p-3 space-y-2">
                      <div className="flex items-center gap-2">
                        <TableProperties className="h-4 w-4 text-muted-foreground" />
                        <span className="text-sm font-medium">{selectedDataset.name}</span>
                        {selectedDataset.dataset_type && (
                          <Badge variant="outline" className="capitalize">
                            {selectedDataset.dataset_type.replace(/_/g, ' ')}
                          </Badge>
                        )}
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {formatDatasetMeta(selectedDataset)}
                      </p>
                    </div>
                  )}

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div className="space-y-2">
                      <Label htmlFor="workflow-dataset-batch-size">Batch size</Label>
                      <Input
                        id="workflow-dataset-batch-size"
                        value={batchSize}
                        onChange={(event) => setBatchSize(event.target.value)}
                        placeholder="1000"
                        inputMode="numeric"
                      />
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="workflow-dataset-limit">Row limit</Label>
                      <Input
                        id="workflow-dataset-limit"
                        value={limit}
                        onChange={(event) => setLimit(event.target.value)}
                        placeholder="Unlimited"
                        inputMode="numeric"
                      />
                    </div>
                  </div>

                  <p className="text-xs text-muted-foreground">
                    Dataset execution reads from previously imported or workflow-produced datasets
                    only. Source-only catalogue assets do not appear here.
                  </p>
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-4">
              <CardTitle className="text-base">Output Materialization</CardTitle>
              <CardDescription>
                Persist the final workflow rows as a new catalogue dataset when you want the
                result to feed later workflows or downstream review.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between gap-4 rounded-sm border border-border px-3 py-3">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <Database className="h-4 w-4 text-muted-foreground" />
                    <span className="text-sm font-medium">Materialize final output</span>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    The coordinator will write a Parquet-backed dataset and register workflow
                    lineage for it.
                  </p>
                </div>
                <Switch checked={materializeOutput} onCheckedChange={setMaterializeOutput} />
              </div>

              {materializeOutput && (
                <div className="space-y-2">
                  <Label htmlFor="workflow-output-dataset-name">Dataset name</Label>
                  <Input
                    id="workflow-output-dataset-name"
                    value={outputDatasetName}
                    onChange={(event) => setOutputDatasetName(event.target.value)}
                    placeholder="Optional. Leave blank to let the coordinator name it."
                  />
                </div>
              )}
            </CardContent>
          </Card>

          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isExecuting}>
              Cancel
            </Button>
            <Button
              onClick={handleExecute}
              disabled={isExecuting || (inputMode === 'dataset' && datasets.length === 0)}
              className="gap-2"
            >
              {isExecuting ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Executing...
                </>
              ) : (
                <>
                  <Play className="h-4 w-4" />
                  Execute Workflow
                </>
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
