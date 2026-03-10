/**
 * Inspector Pane Component
 * Configuration panel for selected workflow step
 */

import React, { useState, useMemo } from 'react';
import { motion } from 'framer-motion';
import { Node, Edge } from 'reactflow';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import { X, ChevronLeft, ChevronRight, CheckCircle, Play, Trash2, Loader2, Plus, RefreshCw } from 'lucide-react';
import { cn } from '@/lib/utils';
import { getStepTypeConfig } from '@/lib/workflow-step-config';
import { useModels } from '@/hooks/useModels';
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

interface InspectorPaneProps {
  selectedNode: Node | null;
  nodes: Node[];
  edges: Edge[];
  onUpdateNode: (nodeId: string, data: any) => void;
  onDeleteNode: (nodeId: string) => void;
  onClose: () => void;
}

export function InspectorPane({
  selectedNode,
  nodes,
  edges,
  onUpdateNode,
  onDeleteNode,
  onClose,
}: InspectorPaneProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [showTestDialog, setShowTestDialog] = useState(false);
  const [testInput, setTestInput] = useState('{\n  "data": "sample input"\n}');
  const [testContext, setTestContext] = useState('{\n  "tenant_id": "test"\n}');
  const [testResult, setTestResult] = useState<any>(null);

  const { data: models } = useModels();
  const testStep = useTestWorkflowStep();

  // Calculate upstream schema from connected nodes
  // MUST be before early return to follow Rules of Hooks
  // Helper function to extract schema from a node
  const extractSchemaFromNode = (node: Node | undefined) => {
    if (!node) return [];

    const config = node.data.config;

    // CSV Source node
    if (node.data.step_type === 'csv_source' && config?.detected_fields) {
      return config.detected_fields.map((field: any) => ({
        name: field.name,
        type: field.type,
        sample_values: field.sample_values || [],
      }));
    }

    // DB Extract node
    if (node.data.step_type === 'db_extract' && config?.detected_fields) {
      return config.detected_fields.map((field: any) => ({
        name: field.name,
        type: field.type,
        sample_values: field.sample_values || [],
      }));
    }

    // Multi-Source Input node
    if (node.data.step_type === 'multi_source_input' && config?.mergedSchema) {
      return config.mergedSchema.map((field: any) => ({
        name: field.name,
        type: field.type,
        sample_values: [],
      }));
    }

    // Field Transformer node (pass through its output schema)
    if (node.data.step_type === 'field_transformer' && config?.transformations) {
      // For now, just return the field names from transformations
      // In a full implementation, we'd calculate the transformed schema
      return config.transformations.map((t: any) => ({
        name: t.field,
        type: 'transformed',
        sample_values: [],
      }));
    }

    // Aggregator node
    if (node.data.step_type === 'aggregator' && config?.aggregations) {
      return config.aggregations.map((agg: any) => ({
        name: agg.alias || agg.field,
        type: 'number',
        sample_values: [],
      }));
    }

    // Data Joiner node
    if (node.data.step_type === 'data_joiner' && config?.output_columns) {
      return config.output_columns.map((col: string) => ({
        name: col,
        type: 'string',
        sample_values: [],
      }));
    }

    return [];
  };

  const upstreamSchema = useMemo(() => {
    if (!selectedNode) return [];

    // Find edges where the selected node is the target
    const incomingEdges = edges.filter((edge) => edge.target === selectedNode.id);

    if (incomingEdges.length === 0) return [];

    // Get the first upstream node (we can enhance this to merge multiple sources later)
    const upstreamEdge = incomingEdges[0];
    const upstreamNode = nodes.find((node) => node.id === upstreamEdge.source);

    return extractSchemaFromNode(upstreamNode);
  }, [selectedNode, nodes, edges]);

  // For Data Joiner: detect left and right upstream schemas
  const dualUpstreamSchemas = useMemo(() => {
    if (!selectedNode || selectedNode.data.step_type !== 'data_joiner') {
      return { left: [], right: [] };
    }

    // Find edges where the selected node is the target
    const incomingEdges = edges.filter((edge) => edge.target === selectedNode.id);

    if (incomingEdges.length === 0) {
      return { left: [], right: [] };
    }

    // Get left and right upstream nodes
    // Convention: first edge = left, second edge = right
    const leftNode = nodes.find((node) => node.id === incomingEdges[0]?.source);
    const rightNode = nodes.find((node) => node.id === incomingEdges[1]?.source);

    return {
      left: extractSchemaFromNode(leftNode),
      right: extractSchemaFromNode(rightNode),
      leftNodeLabel: leftNode?.data.label || 'Left Source',
      rightNodeLabel: rightNode?.data.label || 'Right Source',
    };
  }, [selectedNode, nodes, edges]);

  // Early return AFTER all hooks
  if (!selectedNode) return null;

  const stepConfig = getStepTypeConfig(selectedNode.data.step_type as StepType);
  const StepIcon = stepConfig.icon;

  // Generate sample input data from upstream schema
  const generateSampleInput = () => {
    if (upstreamSchema.length === 0) {
      return '{\n  "data": "sample input"\n}';
    }

    const sampleRow: Record<string, any> = {};
    upstreamSchema.forEach((field: any) => {
      // Use first sample value if available, otherwise use type-appropriate default
      if (field.sample_values && field.sample_values.length > 0) {
        sampleRow[field.name] = field.sample_values[0];
      } else {
        // Fallback defaults based on type
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
          step_type: selectedNode.data.step_type,  // ✅ Fixed: use step_type (snake_case)
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
    onUpdateNode(selectedNode.id, {
      ...selectedNode.data,
      config: {
        ...selectedNode.data.config,
        ...updates,
      },
    });
  };

  const handleUpdateLabel = (label: string) => {
    onUpdateNode(selectedNode.id, {
      ...selectedNode.data,
      label,
    });
  };

  // Render config form based on step type
  const renderConfigForm = () => {
    switch (selectedNode.data.step_type) {
      case 'ml_prediction':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Model</Label>
              <Select
                value={selectedNode.data.config?.model_id || ''}
                onValueChange={(value) => handleUpdateConfig({ model_id: value })}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select model..." />
                </SelectTrigger>
                <SelectContent>
                  {models?.map((model) => (
                    <SelectItem key={model.id} value={model.id}>
                      {model.name} v{model.version}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                ML model to invoke for predictions
              </p>
            </div>

            <div className="space-y-2">
              <Label>Timeout (ms)</Label>
              <Input
                type="number"
                value={selectedNode.data.config?.timeout_ms || 500}
                onChange={(e) =>
                  handleUpdateConfig({ timeout_ms: parseInt(e.target.value) || 500 })
                }
                min={100}
                max={10000}
                step={100}
              />
            </div>

            <div className="space-y-2">
              <Label>Cache TTL (seconds)</Label>
              <Input
                type="number"
                value={selectedNode.data.config?.cache_ttl_secs || 0}
                onChange={(e) =>
                  handleUpdateConfig({ cache_ttl_secs: parseInt(e.target.value) || 0 })
                }
                min={0}
                max={3600}
                step={60}
              />
              <p className="text-xs text-muted-foreground">
                0 = no caching, responses cached for specified seconds
              </p>
            </div>
          </div>
        );

      case 'confidence_gate':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Confidence Threshold</Label>
              <div className="flex items-center gap-2">
                <Input
                  type="number"
                  value={selectedNode.data.config?.threshold || 0.8}
                  onChange={(e) =>
                    handleUpdateConfig({ threshold: parseFloat(e.target.value) || 0.8 })
                  }
                  min={0}
                  max={1}
                  step={0.05}
                />
                <span className="text-sm text-muted-foreground">
                  {((selectedNode.data.config?.threshold || 0.8) * 100).toFixed(0)}%
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                Minimum confidence score to pass this gate
              </p>
            </div>

            <div className="space-y-2">
              <Label>Input Step (Optional)</Label>
              <Input
                value={selectedNode.data.config?.input_step || ''}
                onChange={(e) => handleUpdateConfig({ input_step: e.target.value })}
                placeholder="e.g., ml_step_1"
              />
              <p className="text-xs text-muted-foreground">
                Specific step to read confidence from
              </p>
            </div>
          </div>
        );

      case 'heuristic_rule':
      case 'wasm_rule':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Rule ID</Label>
              <Input
                value={selectedNode.data.config?.rule_id || ''}
                onChange={(e) => handleUpdateConfig({ rule_id: e.target.value })}
                placeholder="e.g., address_standardization_v1"
              />
              <p className="text-xs text-muted-foreground">
                Identifier for the rule to execute
              </p>
            </div>

            {selectedNode.data.step_type === 'heuristic_rule' && (
              <div className="space-y-2">
                <Label>Minimum Confidence</Label>
                <Input
                  type="number"
                  value={selectedNode.data.config?.min_confidence || 0}
                  onChange={(e) =>
                    handleUpdateConfig({
                      min_confidence: parseFloat(e.target.value) || 0,
                    })
                  }
                  min={0}
                  max={1}
                  step={0.05}
                />
              </div>
            )}
          </div>
        );

      case 'weighted_vote':
        const weights = selectedNode.data.config?.weights || {};
        const weightEntries = Object.entries(weights);
        const weightSum = weightEntries.reduce((sum, [_, weight]) => sum + (weight as number), 0);
        const isValid = Math.abs(weightSum - 1.0) < 0.01;

        const handleAddWeight = () => {
          const newStepId = `step_${Object.keys(weights).length + 1}`;
          handleUpdateConfig({
            weights: {
              ...weights,
              [newStepId]: 0.0,
            },
          });
        };

        const handleUpdateWeight = (stepId: string, value: number) => {
          handleUpdateConfig({
            weights: {
              ...weights,
              [stepId]: value,
            },
          });
        };

        const handleRemoveWeight = (stepId: string) => {
          const newWeights = { ...weights };
          delete newWeights[stepId];
          handleUpdateConfig({ weights: newWeights });
        };

        const handleUpdateStepId = (oldId: string, newId: string) => {
          const newWeights: Record<string, number> = {};
          Object.entries(weights).forEach(([id, weight]) => {
            newWeights[id === oldId ? newId : id] = weight as number;
          });
          handleUpdateConfig({ weights: newWeights });
        };

        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label>Step Weights</Label>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={handleAddWeight}
                  className="h-7 text-xs"
                >
                  <Plus className="h-3 w-3 mr-1" />
                  Add Weight
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                Assign weights to upstream steps (must sum to 1.0)
              </p>

              {/* Weight Entries */}
              <div className="space-y-2">
                {weightEntries.length === 0 ? (
                  <div className="p-3 bg-muted rounded-sm text-xs text-muted-foreground text-center">
                    No weights configured. Click "Add Weight" to start.
                  </div>
                ) : (
                  weightEntries.map(([stepId, weight]) => (
                    <div key={stepId} className="flex items-center gap-2">
                      <Input
                        value={stepId}
                        onChange={(e) => handleUpdateStepId(stepId, e.target.value)}
                        placeholder="step_id"
                        className="flex-1 text-xs font-mono"
                      />
                      <Input
                        type="number"
                        value={weight as number}
                        onChange={(e) =>
                          handleUpdateWeight(stepId, parseFloat(e.target.value) || 0)
                        }
                        min={0}
                        max={1}
                        step={0.1}
                        className="w-24 text-xs"
                      />
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => handleRemoveWeight(stepId)}
                        className="h-8 w-8 p-0"
                      >
                        <X className="h-3 w-3" />
                      </Button>
                    </div>
                  ))
                )}
              </div>

              {/* Weight Sum Validation */}
              {weightEntries.length > 0 && (
                <div
                  className={cn(
                    'p-2 rounded-sm text-xs',
                    isValid
                      ? 'bg-green-50 dark:bg-green-950/20 text-green-700 dark:text-green-300 border border-green-200 dark:border-green-800'
                      : 'bg-yellow-50 dark:bg-yellow-950/20 text-yellow-700 dark:text-yellow-300 border border-yellow-200 dark:border-yellow-800'
                  )}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-medium">Total Weight:</span>
                    <span className="font-mono">{weightSum.toFixed(2)}</span>
                  </div>
                  {!isValid && (
                    <p className="mt-1 text-xs">
                      Weights should sum to 1.0 (currently {weightSum.toFixed(2)})
                    </p>
                  )}
                </div>
              )}
            </div>
          </div>
        );

      case 'confidence_aggregate':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Aggregation Method</Label>
              <Select
                value={selectedNode.data.config?.method || 'weighted_average'}
                onValueChange={(value) => handleUpdateConfig({ method: value })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="weighted_average">Weighted Average</SelectItem>
                  <SelectItem value="bayesian">Bayesian</SelectItem>
                  <SelectItem value="voting">Voting</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                How to combine multiple confidence scores
              </p>
            </div>
          </div>
        );

      case 'conditional_router':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Condition Expression</Label>
              <Textarea
                value={selectedNode.data.config?.condition || ''}
                onChange={(e) => handleUpdateConfig({ condition: e.target.value })}
                placeholder="confidence >= 0.90"
                className="font-mono text-sm h-24"
              />
              <p className="text-xs text-muted-foreground">
                Expression to evaluate (e.g., "confidence &gt;= 0.90", "status == 'verified'")
              </p>
              <div className="bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-sm p-2 mt-2">
                <p className="text-xs text-blue-900 dark:text-blue-200 font-semibold mb-1">Available Variables:</p>
                <ul className="text-xs text-blue-800 dark:text-blue-300 space-y-0.5 ml-4 list-disc">
                  <li><code className="font-mono bg-blue-100 dark:bg-blue-900/30 px-1 rounded">confidence</code> - confidence score (0.0-1.0)</li>
                  <li><code className="font-mono bg-blue-100 dark:bg-blue-900/30 px-1 rounded">status</code> - step execution status</li>
                  <li><code className="font-mono bg-blue-100 dark:bg-blue-900/30 px-1 rounded">output.*</code> - previous step output fields</li>
                </ul>
              </div>
            </div>

            <div className="space-y-2">
              <Label>TRUE Branch Target (Optional)</Label>
              <Input
                value={selectedNode.data.config?.true_branch || ''}
                onChange={(e) => handleUpdateConfig({ true_branch: e.target.value })}
                placeholder="step_id_for_true"
              />
              <p className="text-xs text-muted-foreground">
                Step ID to route to when condition is true
              </p>
            </div>

            <div className="space-y-2">
              <Label>FALSE Branch Target (Optional)</Label>
              <Input
                value={selectedNode.data.config?.false_branch || ''}
                onChange={(e) => handleUpdateConfig({ false_branch: e.target.value })}
                placeholder="step_id_for_false"
              />
              <p className="text-xs text-muted-foreground">
                Step ID to route to when condition is false
              </p>
            </div>
          </div>
        );

      case 'field_mapper':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Target Ontology Field</Label>
              <Input
                value={selectedNode.data.config?.target_field || ''}
                onChange={(e) => handleUpdateConfig({ target_field: e.target.value })}
                placeholder="schema:streetAddress"
              />
              <p className="text-xs text-muted-foreground">
                Ontology field to map to (e.g., "schema:streetAddress")
              </p>
            </div>

            <div className="space-y-2">
              <Label>Aggregation Method</Label>
              <Select
                value={selectedNode.data.config?.aggregation_method || 'weighted_vote'}
                onValueChange={(value) => handleUpdateConfig({ aggregation_method: value })}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="weighted_vote">Weighted Vote</SelectItem>
                  <SelectItem value="highest_confidence">Highest Confidence</SelectItem>
                  <SelectItem value="most_recent">Most Recent</SelectItem>
                  <SelectItem value="manual_priority">Manual Priority</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                How to resolve conflicts between multiple sources
              </p>
            </div>

            <div className="space-y-2">
              <Label>Minimum Confidence</Label>
              <Input
                type="number"
                value={selectedNode.data.config?.min_confidence || 0}
                onChange={(e) =>
                  handleUpdateConfig({ min_confidence: parseFloat(e.target.value) || 0 })
                }
                min={0}
                max={1}
                step={0.05}
              />
              <p className="text-xs text-muted-foreground">
                Minimum confidence to consider a source
              </p>
            </div>

            <div className="space-y-2">
              <Label>Source Mappings</Label>
              <div className="p-3 bg-muted rounded-sm text-xs text-muted-foreground">
                Configure source field mappings and weights (coming soon)
              </div>
            </div>
          </div>
        );

      case 'data_transformer':
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label>Transformation Operations</Label>
              <p className="text-xs text-muted-foreground mb-2">
                Configure data normalization, validation, and cleaning operations
              </p>
              <div className="p-3 bg-muted rounded-sm text-xs text-muted-foreground">
                Configure transformation operations (coming soon)
              </div>
            </div>
          </div>
        );

      // ETL Nodes - Extract
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

      // ETL Nodes - Transform
      case 'semantic_mapper':
        return (
          <SemanticMapperConfigForm
            config={selectedNode.data.config}
            onUpdate={handleUpdateConfig}
            nodeId={selectedNode.id}
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
          />
        );

      // ETL Nodes - Quality
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

      // ETL Nodes - Load
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

      // ETL Nodes - Orchestration
      case 'scheduler':
        return (
          <div className="p-4 space-y-3">
            <div className="text-center text-sm text-muted-foreground">
              Scheduler configuration coming soon
            </div>
            <div className="p-3 bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-sm text-xs text-blue-900 dark:text-blue-200">
              <p className="font-medium mb-1">In Development</p>
              <p>
                Workflow scheduling configuration will support:
              </p>
              <ul className="list-disc ml-4 mt-1 space-y-0.5">
                <li>Cron expressions for recurring execution</li>
                <li>Interval-based scheduling</li>
                <li>One-time scheduled execution</li>
              </ul>
            </div>
          </div>
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
    <motion.aside
      initial={{ width: 800 }}
      animate={{ width: isCollapsed ? 48 : 800 }}
      className="border-l border-border bg-background flex flex-col h-full relative"
    >
      {/* Collapse Toggle */}
      <Button
        variant="ghost"
        size="sm"
        onClick={() => setIsCollapsed(!isCollapsed)}
        className="absolute top-2 -left-4 z-10 bg-white border border-border rounded-sm shadow-sm h-8 w-8 p-0"
      >
        {isCollapsed ? (
          <ChevronLeft className="h-4 w-4" />
        ) : (
          <ChevronRight className="h-4 w-4" />
        )}
      </Button>

      {!isCollapsed && (
        <>
          {/* Header */}
          <div className="border-b border-border p-4">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-semibold">Step Configuration</h3>
              <Button variant="ghost" size="sm" onClick={onClose} className="h-6 w-6 p-0">
                <X className="h-4 w-4" />
              </Button>
            </div>

            {/* Step Type Badge */}
            <div
              className={`flex items-center gap-2 p-2 rounded-sm ${
                selectedNode.data.step_type === 'field_transformer' || selectedNode.data.step_type === 'semantic_mapper' || selectedNode.data.step_type === 'data_joiner' || selectedNode.data.step_type === 'aggregator'
                  ? 'bg-green-50 dark:bg-green-950/30 text-green-700 dark:text-green-300 [&_svg]:text-green-700 dark:[&_svg]:text-green-300'
                  : selectedNode.data.step_type === 'csv_source' || selectedNode.data.step_type === 'db_extract' || selectedNode.data.step_type === 'multi_source_input'
                  ? 'bg-blue-50 dark:bg-blue-950/30 text-blue-700 dark:text-blue-300 [&_svg]:text-blue-700 dark:[&_svg]:text-blue-300'
                  : selectedNode.data.step_type === 'data_validator' || selectedNode.data.step_type === 'deduplicator'
                  ? 'bg-red-50 dark:bg-red-950/30 text-red-700 dark:text-red-300 [&_svg]:text-red-700 dark:[&_svg]:text-red-300'
                  : selectedNode.data.step_type === 'rdf_loader' || selectedNode.data.step_type === 'db_loader' || selectedNode.data.step_type === 'csv_exporter'
                  ? 'bg-purple-50 dark:bg-purple-950/30 text-purple-700 dark:text-purple-300 [&_svg]:text-purple-700 dark:[&_svg]:text-purple-300'
                  : selectedNode.data.step_type === 'scheduler'
                  ? 'bg-orange-50 dark:bg-orange-950/30 text-orange-700 dark:text-orange-300 [&_svg]:text-orange-700 dark:[&_svg]:text-orange-300'
                  : 'bg-neutral-50 dark:bg-neutral-800 text-foreground'
              }`}
            >
              <StepIcon className="h-4 w-4 flex-shrink-0" />
              <span className="text-xs font-semibold">
                {stepConfig.label}
              </span>
            </div>
          </div>

          {/* Tabs */}
          <Tabs defaultValue="config" className="flex-1 flex flex-col overflow-hidden">
            <TabsList className="mx-4 mt-2">
              <TabsTrigger value="config">Config</TabsTrigger>
              <TabsTrigger value="info">Info</TabsTrigger>
            </TabsList>

            <ScrollArea className="flex-1">
              <TabsContent value="config" className="p-4 space-y-4 mt-0">
                {/* Step Label */}
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

              <TabsContent value="info" className="p-4 mt-0">
                <div className="space-y-4">
                  <div>
                    <h4 className="text-sm font-semibold mb-2">Description</h4>
                    <p className="text-xs text-muted-foreground">{stepConfig.description}</p>
                  </div>

                  <div>
                    <h4 className="text-sm font-semibold mb-2">Category</h4>
                    <p className="text-xs text-muted-foreground capitalize">
                      {stepConfig.category}
                    </p>
                  </div>

                  {selectedNode.data.executionDuration && (
                    <div>
                      <h4 className="text-sm font-semibold mb-2">Last Execution</h4>
                      <div className="space-y-1 text-xs text-muted-foreground">
                        <div>Duration: {selectedNode.data.executionDuration}ms</div>
                        <div>
                          Confidence:{' '}
                          {((selectedNode.data.executionConfidence || 0) * 100).toFixed(1)}%
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              </TabsContent>
            </ScrollArea>
          </Tabs>

          {/* Footer Actions */}
          <div className="border-t border-border p-4 space-y-2">
            <Button className="w-full gap-2" size="sm" variant="outline">
              <CheckCircle className="h-4 w-4" />
              Validate Step
            </Button>
            <Button
              className="w-full gap-2"
              size="sm"
              variant="outline"
              onClick={() => {
                setTestResult(null);
                // Auto-populate with sample data when opening dialog
                if (upstreamSchema.length > 0) {
                  setTestInput(generateSampleInput());
                }
                setShowTestDialog(true);
              }}
            >
              <Play className="h-4 w-4" />
              Test Step
            </Button>
            <Button
              className="w-full gap-2"
              size="sm"
              variant="destructive"
              onClick={() => {
                onDeleteNode(selectedNode.id);
                onClose();
              }}
            >
              <Trash2 className="h-4 w-4" />
              Delete Step
            </Button>
          </div>
        </>
      )}

      {/* Test Step Dialog */}
      <Dialog open={showTestDialog} onOpenChange={setShowTestDialog}>
        <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Test Step: {selectedNode?.data.label}</DialogTitle>
            <DialogDescription>
              Test this step with sample input data
            </DialogDescription>
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
              {upstreamSchema.length > 0 && (
                <p className="text-xs text-muted-foreground mt-1.5">
                  💡 Click "Use Sample Data" to auto-populate from upstream source
                </p>
              )}
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
                <div className={cn(
                  "p-4 rounded-md mt-2 border-2",
                  testResult.success
                    ? "bg-green-50 dark:bg-green-950/20 border-green-200 dark:border-green-800"
                    : "bg-red-50 dark:bg-red-950/20 border-red-200 dark:border-red-800"
                )}>
                  <div className="flex items-center gap-2 mb-2">
                    <span className="font-semibold">
                      {testResult.success ? '✅ Success' : '❌ Failed'}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {testResult.execution_time_ms}ms
                    </span>
                  </div>

                  {testResult.error && (
                    <div className="mt-2 text-sm text-red-700">
                      <strong>Error:</strong> {testResult.error}
                    </div>
                  )}

                  {testResult.output && (
                    <div className="mt-2">
                      <div className="text-xs font-semibold mb-1">Output:</div>
                      <pre className="text-xs bg-white p-2 rounded border overflow-auto max-h-40">
                        {JSON.stringify(testResult.output, null, 2)}
                      </pre>
                    </div>
                  )}
                </div>
              </div>
            )}

            <div className="flex gap-2">
              <Button
                onClick={handleTestStep}
                disabled={testStep.isPending}
                className="flex-1"
              >
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
              <Button
                variant="outline"
                onClick={() => setShowTestDialog(false)}
              >
                Close
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </motion.aside>
  );
}
