/**
 * Step Configuration Dialog
 * Full-window popup for configuring workflow steps
 *
 * Design Philosophy:
 * - Gradio-inspired full-window configuration
 * - Maximum screen space for complex forms
 * - Modal overlay with backdrop
 * - Tabbed interface for organization
 * - Quick actions and keyboard shortcuts
 */

import React, { useState, useMemo } from 'react';
import { Node, Edge } from 'reactflow';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import {
  X,
  CheckCircle,
  Play,
  Trash2,
  Loader2,
  RefreshCw,
  Save,
  Info,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { getStepTypeConfig } from '@/lib/workflow-step-config';
import { getETLStepTypeConfig, isETLStepType } from '@/lib/workflow-etl-config';
import { useTestWorkflowStep } from '@/hooks/useWorkflows';
import { toast } from 'sonner';
import type { StepType } from '@/api/types';
import {
  CSVSourceConfigForm,
  CSVExporterConfigForm,
  DBExtractConfigForm,
  DBLoaderConfigForm,
  SemanticMapperConfigForm,
  FieldTransformerConfigForm,
  DataJoinerConfigForm,
  AggregatorConfigForm,
  DataValidatorConfigForm,
  DeduplicatorConfigForm,
  RDFLoaderConfigForm,
  MultiSourceInputConfigForm,
} from './config-forms';

interface StepConfigDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedNode: Node | null;
  nodes: Node[];
  edges: Edge[];
  onUpdateNode: (nodeId: string, data: any) => void;
  onDeleteNode: (nodeId: string) => void;
}

export function StepConfigDialog({
  open,
  onOpenChange,
  selectedNode,
  nodes,
  edges,
  onUpdateNode,
  onDeleteNode,
}: StepConfigDialogProps) {
  const [activeTab, setActiveTab] = useState('config');
  const [showTestDialog, setShowTestDialog] = useState(false);
  const [testInput, setTestInput] = useState('{\n  "data": "sample input"\n}');
  const [testContext, setTestContext] = useState('{\n  "tenant_id": "test"\n}');
  const [testResult, setTestResult] = useState<any>(null);

  const testStep = useTestWorkflowStep();

  // Calculate upstream schema from connected nodes
  const extractSchemaFromNode = (node: Node | undefined) => {
    if (!node) return [];

    const config = node.data.config;

    if (node.data.step_type === 'csv_source' && config?.detected_fields) {
      return config.detected_fields.map((field: any) => ({
        name: field.name,
        type: field.type,
        sample_values: field.sample_values || [],
      }));
    }

    if (node.data.step_type === 'db_extract' && config?.detected_fields) {
      return config.detected_fields.map((field: any) => ({
        name: field.name,
        type: field.type,
        sample_values: field.sample_values || [],
      }));
    }

    if (node.data.step_type === 'multi_source_input' && config?.mergedSchema) {
      return config.mergedSchema.map((field: any) => ({
        name: field.name,
        type: field.type,
        sample_values: [],
      }));
    }

    if (node.data.step_type === 'field_transformer' && config?.transformations) {
      return config.transformations.map((t: any) => ({
        name: t.field,
        type: 'transformed',
        sample_values: [],
      }));
    }

    if (node.data.step_type === 'aggregator' && config?.aggregations) {
      return config.aggregations.map((agg: any) => ({
        name: agg.alias || agg.field,
        type: 'number',
        sample_values: [],
      }));
    }

    if (node.data.step_type === 'data_joiner' && config?.output_columns) {
      return config.output_columns.map((col: string) => ({
        name: col,
        type: 'string',
        sample_values: [],
      }));
    }

    return [];
  };

  const upstreamNode = useMemo(() => {
    if (!selectedNode) return undefined;

    const incomingEdges = edges.filter((edge) => edge.target === selectedNode.id);
    if (incomingEdges.length === 0) return undefined;

    const upstreamEdge = incomingEdges[0];
    return nodes.find((node) => node.id === upstreamEdge.source);
  }, [selectedNode, nodes, edges]);

  const upstreamSchema = useMemo(() => {
    if (!upstreamNode) return [];
    return extractSchemaFromNode(upstreamNode);
  }, [upstreamNode]);

  const upstreamDatasourceId =
    upstreamNode?.data.step_type === 'db_extract'
      ? upstreamNode.data.config?.datasource_id
      : undefined;

  const upstreamSourceTable =
    upstreamNode?.data.step_type === 'db_extract'
      ? upstreamNode.data.config?.schema_table || upstreamNode.data.config?.table_name
      : undefined;

  const dualUpstreamSchemas = useMemo(() => {
    if (!selectedNode || selectedNode.data.step_type !== 'data_joiner') {
      return { left: [], right: [] };
    }

    const incomingEdges = edges.filter((edge) => edge.target === selectedNode.id);

    if (incomingEdges.length === 0) {
      return { left: [], right: [] };
    }

    const leftNode = nodes.find((node) => node.id === incomingEdges[0]?.source);
    const rightNode = nodes.find((node) => node.id === incomingEdges[1]?.source);

    return {
      left: extractSchemaFromNode(leftNode),
      right: extractSchemaFromNode(rightNode),
      leftNodeLabel: leftNode?.data.label || 'Left Source',
      rightNodeLabel: rightNode?.data.label || 'Right Source',
    };
  }, [selectedNode, nodes, edges]);

  if (!selectedNode) return null;

  const stepConfig = isETLStepType(selectedNode.data.step_type as StepType)
    ? getETLStepTypeConfig(selectedNode.data.step_type as any)
    : getStepTypeConfig(selectedNode.data.step_type as StepType);
  const StepIcon = stepConfig.icon;

  const generateSampleInput = () => {
    if (upstreamSchema.length === 0) {
      return '{\n  "data": "sample input"\n}';
    }

    const sampleRow: Record<string, any> = {};
    upstreamSchema.forEach((field: any) => {
      if (field.sample_values && field.sample_values.length > 0) {
        sampleRow[field.name] = field.sample_values[0];
      } else {
        switch (field.type.toUpperCase()) {
          case 'STRING':
            sampleRow[field.name] = 'sample_value';
            break;
          case 'INTEGER':
          case 'NUMBER':
            sampleRow[field.name] = 123;
            break;
          case 'BOOLEAN':
            sampleRow[field.name] = true;
            break;
          case 'TIMESTAMP':
          case 'DATE':
            sampleRow[field.name] = new Date().toISOString();
            break;
          default:
            sampleRow[field.name] = null;
        }
      }
    });

    return JSON.stringify(sampleRow, null, 2);
  };

  const handleTestStep = async () => {
    try {
      const input = JSON.parse(testInput);
      const context = JSON.parse(testContext);

      const result = await testStep.mutateAsync({
        step: {
          id: selectedNode.id,
          step_type: selectedNode.data.step_type,
          config: selectedNode.data.config || {},
        },
        input,
        context,
      });

      setTestResult(result);
    } catch (error: any) {
      if (error.message?.includes('JSON')) {
        toast.error('Invalid JSON in input or context');
      }
    }
  };

  const handleUpdateConfig = (updates: any) => {
    // Find current node from nodes array to avoid stale data
    const currentNode = nodes.find(n => n.id === selectedNode.id);
    const currentData = currentNode?.data || selectedNode.data;

    const updatedData = {
      ...currentData,
      config: {
        ...currentData.config,
        ...updates,
      },
    };

    onUpdateNode(selectedNode.id, updatedData);
  };

  const handleUpdateLabel = (label: string) => {
    // Find current node from nodes array to avoid stale data
    const currentNode = nodes.find(n => n.id === selectedNode.id);
    const currentData = currentNode?.data || selectedNode.data;

    onUpdateNode(selectedNode.id, {
      ...currentData,
      label,
    });
  };

  const handleClose = () => {
    onOpenChange(false);
    setActiveTab('config');
  };

  const handleDelete = () => {
    onDeleteNode(selectedNode.id);
    handleClose();
  };

  // Render config form based on step type
  const renderConfigForm = () => {
    switch (selectedNode.data.step_type) {
      case 'csv_source':
        return (
          <CSVSourceConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
          />
        );

      case 'db_extract':
        return (
          <DBExtractConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
          />
        );

      case 'multi_source_input':
        return (
          <MultiSourceInputConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
          />
        );

      case 'semantic_mapper':
        return (
          <SemanticMapperConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
            datasourceId={upstreamDatasourceId}
            sourceTable={upstreamSourceTable}
            upstreamSchema={upstreamSchema}
          />
        );

      case 'field_transformer':
        return (
          <FieldTransformerConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
            upstreamSchema={upstreamSchema}
          />
        );

      case 'data_joiner':
        return (
          <DataJoinerConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
            leftSchema={dualUpstreamSchemas.left}
            rightSchema={dualUpstreamSchemas.right}
            leftNodeLabel={dualUpstreamSchemas.leftNodeLabel}
            rightNodeLabel={dualUpstreamSchemas.rightNodeLabel}
          />
        );

      case 'aggregator':
        return (
          <AggregatorConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
            upstreamSchema={upstreamSchema}
          />
        );

      case 'data_validator':
        return (
          <DataValidatorConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
            upstreamSchema={upstreamSchema}
          />
        );

      case 'deduplicator':
        return (
          <DeduplicatorConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
          />
        );

      case 'rdf_loader':
        return (
          <RDFLoaderConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
          />
        );

      case 'db_loader':
        return (
          <DBLoaderConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
            upstreamSchema={upstreamSchema}
          />
        );

      case 'csv_exporter':
        return (
          <CSVExporterConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
          />
        );

      default:
        return (
          <div className="p-4 text-center text-sm text-muted-foreground">
            No configuration available for this step type
          </div>
        );
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-6xl max-h-[90vh] h-[90vh] p-0 gap-0 flex flex-col">
        {/* Header */}
        <DialogHeader className="p-6 pb-4 border-b">
          <div className="flex items-start justify-between">
            <div className="flex-1">
              <div className="flex items-center gap-3 mb-2">
                <div
                  className="p-2 rounded-lg border"
                  style={{
                    background: `linear-gradient(135deg, ${stepConfig.color.surface} 0%, ${stepConfig.color.subtle} 100%)`,
                    borderColor: stepConfig.color.border,
                  }}
                >
                  <StepIcon className="h-5 w-5" style={{ color: stepConfig.color.text }} />
                </div>
                <div>
                  <DialogTitle className="text-xl font-semibold">{selectedNode.data.label}</DialogTitle>
                  <DialogDescription className="text-sm">{stepConfig.label}</DialogDescription>
                </div>
              </div>
            </div>

            {/* Quick Actions */}
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setTestResult(null);
                  if (upstreamSchema.length > 0) {
                    setTestInput(generateSampleInput());
                  }
                  setShowTestDialog(true);
                }}
              >
                <Play className="h-4 w-4 mr-1.5" />
                Test
              </Button>
              <Button variant="outline" size="sm" onClick={handleDelete}>
                <Trash2 className="h-4 w-4 mr-1.5" />
                Delete
              </Button>
            </div>
          </div>
        </DialogHeader>

        {/* Tabs */}
        <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col min-h-0">
          <div className="px-6 pt-4">
            <TabsList>
              <TabsTrigger value="config">Configuration</TabsTrigger>
              <TabsTrigger value="info">Information</TabsTrigger>
            </TabsList>
          </div>

          <ScrollArea className="flex-1">
            <div className="px-6 py-4">
              <TabsContent value="config" className="space-y-6 mt-0">
                {/* Step Name */}
                <div className="space-y-2">
                  <Label>Step Name</Label>
                  <Input
                    value={selectedNode.data.label}
                    onChange={(e) => handleUpdateLabel(e.target.value)}
                    placeholder="e.g., Fraud Detection Model"
                  />
                </div>

                {/* Step ID (read-only) */}
                <div className="space-y-2">
                  <Label>Step ID</Label>
                  <Input value={selectedNode.id} disabled className="font-mono text-xs" />
                </div>

                {/* Type-specific configuration */}
                {renderConfigForm()}
              </TabsContent>

              <TabsContent value="info" className="space-y-6 mt-0">
                <div>
                  <h4 className="text-sm font-semibold mb-2">Description</h4>
                  <p className="text-sm text-muted-foreground">{stepConfig.description}</p>
                </div>

                <div>
                  <h4 className="text-sm font-semibold mb-2">Category</h4>
                  <p className="text-sm text-muted-foreground capitalize">{stepConfig.category}</p>
                </div>

                {selectedNode.data.executionDuration && (
                  <div>
                    <h4 className="text-sm font-semibold mb-2">Last Execution</h4>
                    <div className="space-y-1 text-sm text-muted-foreground">
                      <div>Duration: {selectedNode.data.executionDuration}ms</div>
                      <div>
                        Confidence: {((selectedNode.data.executionConfidence || 0) * 100).toFixed(1)}%
                      </div>
                    </div>
                  </div>
                )}
              </TabsContent>
            </div>
          </ScrollArea>
        </Tabs>

        {/* Footer */}
        <DialogFooter className="p-6 pt-4 border-t">
          <Button variant="outline" onClick={handleClose}>
            Close
          </Button>
          <Button onClick={handleClose}>
            <Save className="h-4 w-4 mr-1.5" />
            Save Changes
          </Button>
        </DialogFooter>

        {/* Test Dialog */}
        <Dialog open={showTestDialog} onOpenChange={setShowTestDialog}>
          <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>Test Step: {selectedNode?.data.label}</DialogTitle>
              <DialogDescription>Test this step with sample input data</DialogDescription>
            </DialogHeader>

            <div className="space-y-4 mt-4">
              <div>
                <div className="flex items-center justify-between mb-2">
                  <Label>Input Data (JSON)</Label>
                  {upstreamSchema.length > 0 && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setTestInput(generateSampleInput())}
                      className="h-7 text-xs"
                    >
                      <RefreshCw className="w-3 h-3 mr-1.5" />
                      Use Sample Data
                    </Button>
                  )}
                </div>
                <Textarea
                  value={testInput}
                  onChange={(e) => setTestInput(e.target.value)}
                  className="font-mono text-sm h-32"
                  placeholder='{"data": "sample input"}'
                />
              </div>

              <div>
                <Label>Context (JSON)</Label>
                <Textarea
                  value={testContext}
                  onChange={(e) => setTestContext(e.target.value)}
                  className="font-mono text-sm h-24 mt-2"
                  placeholder='{"tenant_id": "test"}'
                />
              </div>

              {testResult && (
                <div>
                  <Label>Test Result</Label>
                  <div
                    className={cn(
                      'p-4 rounded-md mt-2 border-2',
                      testResult.success
                        ? 'bg-green-50 dark:bg-green-950/15 border-green-200 dark:border-green-800'
                        : 'bg-red-50 dark:bg-red-950/15 border-red-200 dark:border-red-800'
                    )}
                  >
                    <div className="flex items-center gap-2 mb-2">
                      <span className="font-semibold">
                        {testResult.success ? '✅ Success' : '❌ Failed'}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {testResult.execution_time_ms}ms
                      </span>
                    </div>

                    {testResult.error && (
                      <div className="mt-2 text-sm text-error">
                        <strong>Error:</strong> {testResult.error}
                      </div>
                    )}

                    {testResult.output && (
                      <div className="mt-2">
                        <div className="text-xs font-semibold mb-1">Output:</div>
                        <pre className="text-xs bg-card p-2 rounded border overflow-auto max-h-40">
                          {JSON.stringify(testResult.output, null, 2)}
                        </pre>
                      </div>
                    )}
                  </div>
                </div>
              )}

              <div className="flex gap-2">
                <Button onClick={handleTestStep} disabled={testStep.isPending} className="flex-1">
                  {testStep.isPending ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Testing...
                    </>
                  ) : (
                    <>
                      <Play className="h-4 w-4 mr-2" />
                      Run Test
                    </>
                  )}
                </Button>
                <Button variant="outline" onClick={() => setShowTestDialog(false)}>
                  Close
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      </DialogContent>
    </Dialog>
  );
}
