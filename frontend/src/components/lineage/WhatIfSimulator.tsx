/**
 * WhatIfSimulator Component
 * Phase 3 feature: Test hypothetical rule changes before production deployment
 * Innovation: Shows impact preview with before/after comparison
 */

import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Slider } from '@/components/ui/slider';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import {
  Beaker,
  Play,
  RotateCcw,
  TrendingUp,
  TrendingDown,
  AlertTriangle,
  CheckCircle2,
  Info,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { LineageGraph, LineageNode, LineageEdge } from '@/hooks/useLineageGraph';

// ============================================================================
// Simulation Types
// ============================================================================

export type SimulationRuleType =
  | 'confidence_threshold' // Change min confidence threshold
  | 'model_version' // Swap model version
  | 'add_validation' // Add validation step
  | 'remove_step' // Remove transformation step
  | 'change_priority'; // Change datasource priority

export interface SimulationRule {
  id: string;
  type: SimulationRuleType;
  description: string;
  parameters: Record<string, any>;
}

export interface SimulationResult {
  affectedNodes: number;
  affectedEdges: number;
  confidenceChange: {
    increased: number;
    decreased: number;
    unchanged: number;
  };
  qualityImpact: {
    improved: number;
    degraded: number;
  };
  simulatedGraph: LineageGraph;
}

interface WhatIfSimulatorProps {
  originalGraph: LineageGraph;
  onSimulationChange: (result: SimulationResult | null) => void;
  className?: string;
}

// ============================================================================
// Simulation Engine
// ============================================================================

/**
 * Apply confidence threshold change to graph
 */
function applyConfidenceThresholdChange(
  graph: LineageGraph,
  newThreshold: number
): SimulationResult {
  let increased = 0;
  let decreased = 0;
  let unchanged = 0;
  let improved = 0;
  let degraded = 0;

  // Simulate filtering nodes below threshold
  const simulatedNodes = graph.nodes.map((node) => {
    const oldConf = node.confidence || 0;
    const passesThreshold = oldConf >= newThreshold;

    if (!passesThreshold && oldConf > 0) {
      degraded++;
      decreased++;
    } else if (passesThreshold && oldConf < 0.9) {
      // Hypothetically, stricter threshold improves quality of remaining nodes
      improved++;
      increased++;
      return {
        ...node,
        confidence: Math.min(oldConf + 0.05, 1), // Simulated improvement
      };
    } else {
      unchanged++;
    }

    return node;
  });

  // Filter edges connected to failing nodes
  const passingNodeIds = new Set(
    simulatedNodes
      .filter((n) => (n.confidence || 0) >= newThreshold)
      .map((n) => n.id)
  );

  const simulatedEdges = graph.edges.filter(
    (edge) => passingNodeIds.has(edge.source) && passingNodeIds.has(edge.target)
  );

  const affectedNodes = graph.nodes.length - simulatedNodes.filter((n) =>
    passingNodeIds.has(n.id)
  ).length;

  const affectedEdges = graph.edges.length - simulatedEdges.length;

  return {
    affectedNodes,
    affectedEdges,
    confidenceChange: { increased, decreased, unchanged },
    qualityImpact: { improved, degraded },
    simulatedGraph: {
      nodes: simulatedNodes.filter((n) => passingNodeIds.has(n.id)),
      edges: simulatedEdges,
      metadata: graph.metadata,
    },
  };
}

/**
 * Apply model version swap simulation
 */
function applyModelVersionSwap(
  graph: LineageGraph,
  oldModelId: string,
  newModelId: string
): SimulationResult {
  let increased = 0;
  let decreased = 0;
  let unchanged = 0;

  const simulatedNodes = graph.nodes.map((node) => {
    if (node.modelId === oldModelId) {
      // Simulate confidence change (new model is hypothetically better)
      const oldConf = node.confidence || 0;
      const newConf = Math.min(oldConf + 0.1, 1); // +10% confidence boost

      if (newConf > oldConf) increased++;
      else unchanged++;

      return {
        ...node,
        modelId: newModelId,
        confidence: newConf,
      };
    }
    unchanged++;
    return node;
  });

  const simulatedEdges = graph.edges.map((edge) => {
    if (edge.modelId === oldModelId) {
      return { ...edge, modelId: newModelId };
    }
    return edge;
  });

  const affectedNodes = simulatedNodes.filter((n) => n.modelId === newModelId).length;
  const affectedEdges = simulatedEdges.filter((e) => e.modelId === newModelId).length;

  return {
    affectedNodes,
    affectedEdges,
    confidenceChange: { increased, decreased, unchanged },
    qualityImpact: { improved: increased, degraded: 0 },
    simulatedGraph: {
      nodes: simulatedNodes,
      edges: simulatedEdges,
      metadata: graph.metadata,
    },
  };
}

/**
 * Simulate adding a validation step
 */
function applyAddValidationStep(
  graph: LineageGraph,
  targetDataset: string
): SimulationResult {
  // Simulate improved confidence for validated nodes
  const simulatedNodes = graph.nodes.map((node) => {
    if (node.dataset === targetDataset && node.confidence) {
      return {
        ...node,
        confidence: Math.min(node.confidence + 0.15, 1), // +15% boost
      };
    }
    return node;
  });

  const affectedNodes = simulatedNodes.filter(
    (n) => n.dataset === targetDataset && n.confidence
  ).length;

  return {
    affectedNodes,
    affectedEdges: 0,
    confidenceChange: { increased: affectedNodes, decreased: 0, unchanged: graph.nodes.length - affectedNodes },
    qualityImpact: { improved: affectedNodes, degraded: 0 },
    simulatedGraph: {
      nodes: simulatedNodes,
      edges: graph.edges,
      metadata: graph.metadata,
    },
  };
}

// ============================================================================
// Component
// ============================================================================

export function WhatIfSimulator({
  originalGraph,
  onSimulationChange,
  className,
}: WhatIfSimulatorProps) {
  const [simulationType, setSimulationType] = useState<SimulationRuleType>('confidence_threshold');
  const [confidenceThreshold, setConfidenceThreshold] = useState(70);
  const [oldModelId, setOldModelId] = useState('');
  const [newModelId, setNewModelId] = useState('');
  const [targetDataset, setTargetDataset] = useState('');
  const [isSimulating, setIsSimulating] = useState(false);

  // Available models and datasets from original graph
  const availableModels = useMemo(
    () => Array.from(originalGraph.metadata.models),
    [originalGraph.metadata.models]
  );

  const availableDatasets = useMemo(
    () => Array.from(originalGraph.metadata.datasets),
    [originalGraph.metadata.datasets]
  );

  // Run simulation
  const handleRunSimulation = () => {
    setIsSimulating(true);

    let result: SimulationResult | null = null;

    switch (simulationType) {
      case 'confidence_threshold':
        result = applyConfidenceThresholdChange(originalGraph, confidenceThreshold / 100);
        break;
      case 'model_version':
        if (oldModelId && newModelId) {
          result = applyModelVersionSwap(originalGraph, oldModelId, newModelId);
        }
        break;
      case 'add_validation':
        if (targetDataset) {
          result = applyAddValidationStep(originalGraph, targetDataset);
        }
        break;
    }

    onSimulationChange(result);
    setIsSimulating(true);
  };

  const handleReset = () => {
    setIsSimulating(false);
    onSimulationChange(null);
  };

  return (
    <Card className={cn('h-full flex flex-col', className)}>
      <CardHeader className="pb-3 space-y-0">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Beaker className="h-4 w-4 text-muted-foreground" />
            <CardTitle className="text-sm font-semibold">What-If Simulator</CardTitle>
          </div>
          {isSimulating && (
            <Button variant="ghost" size="sm" onClick={handleReset} className="h-6 px-2 text-xs">
              <RotateCcw className="h-3 w-3 mr-1" />
              Reset
            </Button>
          )}
        </div>
        <p className="text-xs text-muted-foreground mt-2">
          Test hypothetical changes before production
        </p>
      </CardHeader>

      <Separator />

      <ScrollArea className="flex-1">
        <CardContent className="pt-4 space-y-4">
          {/* Simulation Type Selector */}
          <div className="space-y-2">
            <Label className="text-xs font-semibold uppercase tracking-wide">Simulation Type</Label>
            <div className="grid grid-cols-1 gap-2">
              {[
                { value: 'confidence_threshold', label: 'Confidence Threshold', icon: TrendingUp },
                { value: 'model_version', label: 'Model Version Swap', icon: AlertTriangle },
                { value: 'add_validation', label: 'Add Validation Step', icon: CheckCircle2 },
              ].map((type) => {
                const Icon = type.icon;
                return (
                  <button
                    key={type.value}
                    onClick={() => setSimulationType(type.value as SimulationRuleType)}
                    className={cn(
                      'flex items-center gap-2 px-3 py-2 rounded border text-left transition-all',
                      simulationType === type.value
                        ? 'bg-accent text-accent-foreground border-accent'
                        : 'hover:bg-muted/50 border-border'
                    )}
                  >
                    <Icon className="h-4 w-4 flex-shrink-0" />
                    <span className="text-xs font-medium">{type.label}</span>
                  </button>
                );
              })}
            </div>
          </div>

          <Separator />

          {/* Confidence Threshold Parameters */}
          {simulationType === 'confidence_threshold' && (
            <div className="space-y-3">
              <Label className="text-xs font-semibold">New Threshold: {confidenceThreshold}%</Label>
              <Slider
                value={[confidenceThreshold]}
                onValueChange={(v) => setConfidenceThreshold(v[0])}
                min={0}
                max={100}
                step={5}
                className="w-full"
              />
              <div className="flex items-start gap-2 p-2 bg-muted/50 rounded text-xs">
                <Info className="h-3.5 w-3.5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <p className="text-muted-foreground">
                  Nodes below this threshold will be filtered out, improving overall quality.
                </p>
              </div>
            </div>
          )}

          {/* Model Version Swap Parameters */}
          {simulationType === 'model_version' && (
            <div className="space-y-3">
              <div className="space-y-2">
                <Label className="text-xs font-semibold">Old Model</Label>
                <select
                  value={oldModelId}
                  onChange={(e) => setOldModelId(e.target.value)}
                  className="w-full px-2 py-1.5 text-xs border border-border rounded bg-background"
                >
                  <option value="">Select model...</option>
                  {availableModels.map((model) => (
                    <option key={model} value={model}>
                      {model}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-2">
                <Label className="text-xs font-semibold">New Model ID</Label>
                <Input
                  placeholder="e.g., ml_model_v2"
                  value={newModelId}
                  onChange={(e) => setNewModelId(e.target.value)}
                  className="h-8 text-xs"
                />
              </div>
              <div className="flex items-start gap-2 p-2 bg-muted/50 rounded text-xs">
                <Info className="h-3.5 w-3.5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <p className="text-muted-foreground">
                  Simulates swapping model version. Assumes +10% confidence boost from new model.
                </p>
              </div>
            </div>
          )}

          {/* Add Validation Step Parameters */}
          {simulationType === 'add_validation' && (
            <div className="space-y-3">
              <div className="space-y-2">
                <Label className="text-xs font-semibold">Target Dataset</Label>
                <select
                  value={targetDataset}
                  onChange={(e) => setTargetDataset(e.target.value)}
                  className="w-full px-2 py-1.5 text-xs border border-border rounded bg-background"
                >
                  <option value="">Select dataset...</option>
                  {availableDatasets.map((dataset) => (
                    <option key={dataset} value={dataset}>
                      {dataset}
                    </option>
                  ))}
                </select>
              </div>
              <div className="flex items-start gap-2 p-2 bg-muted/50 rounded text-xs">
                <Info className="h-3.5 w-3.5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <p className="text-muted-foreground">
                  Simulates adding validation step. Assumes +15% confidence boost for validated records.
                </p>
              </div>
            </div>
          )}

          <Separator />

          {/* Run Simulation Button */}
          <Button
            onClick={handleRunSimulation}
            disabled={
              (simulationType === 'model_version' && (!oldModelId || !newModelId)) ||
              (simulationType === 'add_validation' && !targetDataset)
            }
            className="w-full gap-2"
          >
            <Play className="h-4 w-4" />
            {isSimulating ? 'Update Simulation' : 'Run Simulation'}
          </Button>
        </CardContent>
      </ScrollArea>
    </Card>
  );
}
