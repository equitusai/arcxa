/**
 * Professional Enterprise Workflow Node Component
 * Beautiful design with refined shadows, gradients, and polish
 * Phase 2.3: Dark mode support with dynamic theming
 */

import React, { useMemo } from 'react';
import { Handle, Position, NodeProps } from 'reactflow';
import { AlertTriangle, CheckCircle, Loader2, XCircle, Copy, Trash2, Settings } from 'lucide-react';
import { cn } from '@/lib/utils';
import { getStepTypeConfig } from '@/lib/workflow-step-config';
import { getETLStepTypeConfig, isETLStepType } from '@/lib/workflow-etl-config';
import { Button } from '@/components/ui/button';
import type { StepType } from '@/api/types';
import {
  CSVSourceNodeBody,
  SemanticMapperNodeBody,
  DBLoaderNodeBody,
  DBExtractNodeBody,
  RDFLoaderNodeBody,
  FieldTransformerNodeBody,
  DataValidatorNodeBody,
  DataJoinerNodeBody,
  AggregatorNodeBody,
  DeduplicatorNodeBody,
  CSVExporterNodeBody,
} from './nodes';

export interface WorkflowNodeData {
  label: string;
  step_type: StepType;
  config?: any;
  // Execution state
  executionStatus?: 'idle' | 'executing' | 'success' | 'error';
  executionConfidence?: number;
  executionDuration?: number;
  // Validation
  validationError?: string;
  // Action callbacks (provided by parent WorkflowDesigner)
  onDuplicate?: (nodeId: string) => void;
  onDelete?: (nodeId: string) => void;
  onConfigure?: (nodeId: string) => void;
}

export function WorkflowNode({ data, selected, id }: NodeProps<WorkflowNodeData>) {
  // Get configuration based on node type (ETL vs ML/Fusion)
  const isETLNode = isETLStepType(data.step_type);
  const stepConfig = isETLNode
    ? getETLStepTypeConfig(data.step_type as any)
    : getStepTypeConfig(data.step_type);
  const StepIcon = stepConfig.icon;

  // Map execution status for node bodies ('executing' -> 'running')
  const nodeBodyStatus = data.executionStatus === 'executing' ? 'running' : data.executionStatus;

  // Phase 2.3: Dark mode detection
  const isDark = useMemo(
    () => document.documentElement.classList.contains('dark'),
    []
  );

  // Node action handlers
  const handleConfigure = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (data.onConfigure) {
      data.onConfigure(id);
    }
  };

  const handleDuplicate = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (data.onDuplicate) {
      data.onDuplicate(id);
    }
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (data.onDelete) {
      data.onDelete(id);
    }
  };

  // State-specific styling with dark mode support
  const getStateStyles = () => {
    const base = {
      borderColor: isDark ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.12)',
      headerBg: stepConfig.color.subtle,
      boxShadow: isDark
        ? '0 1px 2px rgba(0,0,0,0.25)'
        : '0 1px 2px rgba(0,0,0,0.06)',
    };

    if (selected) {
      return {
        ...base,
        borderColor: '#0078D4',
        boxShadow: isDark
          ? '0 0 0 1px rgba(255,255,255,0.1), 0 0 0 3px #0078D4, 0 2px 8px rgba(0,120,212,0.30)'
          : '0 0 0 1px #FFFFFF, 0 0 0 3px #0078D4, 0 2px 8px rgba(0,120,212,0.20)',
      };
    }

    switch (data.executionStatus) {
      case 'executing':
        return {
          borderColor: '#0078D4',
          headerBg: isDark
            ? 'linear-gradient(135deg, rgba(0,120,212,0.2) 0%, rgba(0,120,212,0.3) 100%)'
            : 'linear-gradient(135deg, #E3F2FD 0%, #BBDEFB 100%)',
          boxShadow: isDark
            ? '0 0 12px rgba(0,120,212,0.4), 0 2px 6px rgba(0,0,0,0.3)'
            : '0 0 12px rgba(0,120,212,0.3), 0 2px 6px rgba(0,0,0,0.1)',
        };
      case 'success':
        return {
          borderColor: '#107C10',
          headerBg: isDark
            ? 'linear-gradient(135deg, rgba(16,124,16,0.2) 0%, rgba(16,124,16,0.3) 100%)'
            : 'linear-gradient(135deg, #E8F5E9 0%, #C8E6C9 100%)',
          boxShadow: isDark
            ? '0 1px 4px rgba(16,124,16,0.25), 0 1px 2px rgba(0,0,0,0.25)'
            : '0 1px 4px rgba(16,124,16,0.15), 0 1px 2px rgba(0,0,0,0.06)',
        };
      case 'error':
        return {
          borderColor: '#D13438',
          headerBg: isDark
            ? 'linear-gradient(135deg, rgba(209,52,56,0.2) 0%, rgba(209,52,56,0.3) 100%)'
            : 'linear-gradient(135deg, #FFEBEE 0%, #FFCDD2 100%)',
          boxShadow: isDark
            ? '0 1px 4px rgba(209,52,56,0.25), 0 1px 2px rgba(0,0,0,0.25)'
            : '0 1px 4px rgba(209,52,56,0.15), 0 1px 2px rgba(0,0,0,0.06)',
        };
      default:
        return base;
    }
  };

  // Configuration summary
  const getConfigSummary = () => {
    if (!data.config) return null;

    const summaries: string[] = [];

    if (data.config.model_id) {
      summaries.push(`Model: ${data.config.model_id}`);
    }
    if (data.config.threshold !== undefined) {
      summaries.push(`Threshold: ${data.config.threshold}`);
    }
    if (data.config.rule_id) {
      summaries.push(`Rule: ${data.config.rule_id}`);
    }
    if (data.config.condition) {
      summaries.push(`Condition: ${data.config.condition.substring(0, 20)}...`);
    }
    if (data.config.target_field) {
      summaries.push(`Target: ${data.config.target_field}`);
    }

    return summaries.length > 0 ? summaries : null;
  };

  const stateStyles = getStateStyles();
  const configSummary = getConfigSummary();

  // Determine which handles to show based on node type
  const isExtractNode = ['csv_source', 'db_extract', 'multi_source_input'].includes(data.step_type);
  const isLoadNode = ['rdf_loader', 'db_loader', 'csv_exporter'].includes(data.step_type);

  // Extract nodes: only output (right)
  // Load nodes: only input (left)
  // Transform nodes: both
  const showLeftHandle = !isExtractNode;
  const showRightHandle = !isLoadNode;

  return (
    <div className="relative group">
      {/* Executing shimmer effect */}
      {data.executionStatus === 'executing' && (
        <div
          className="absolute -inset-0.5 rounded-lg overflow-hidden pointer-events-none"
          style={{
            background: 'linear-gradient(90deg, transparent 0%, rgba(0,120,212,0.15) 50%, transparent 100%)',
            backgroundSize: '200% 100%',
            animation: 'shimmer 2s linear infinite',
          }}
        />
      )}

      {/* Node Container */}
      <div
        className={cn(
          'relative bg-card rounded-md overflow-hidden transition-all duration-200 w-[180px]',
          'hover:scale-[1.01]',
          data.executionStatus === 'success' && 'workflow-node-success'
        )}
        style={{
          border: `1.5px solid ${stateStyles.borderColor}`,
          boxShadow: stateStyles.boxShadow,
        }}
      >
        {/* Header */}
        <div
          className="flex items-center gap-2 px-2.5 py-2 border-b"
          style={{
            background: stateStyles.headerBg,
            borderColor: isDark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.06)',
          }}
        >
          <StepIcon
            className="h-4 w-4 flex-shrink-0"
            style={{ color: stepConfig.color.text, strokeWidth: 2.5 }}
          />
          <span
            className="text-[10px] font-bold tracking-wide uppercase truncate"
            style={{ color: stepConfig.color.text }}
          >
            {stepConfig.label}
          </span>
        </div>

        {/* Body - Use specialized node bodies for ETL nodes */}
        {data.step_type === 'csv_source' ? (
          <CSVSourceNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'db_extract' ? (
          <DBExtractNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'semantic_mapper' ? (
          <SemanticMapperNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'field_transformer' ? (
          <FieldTransformerNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'data_joiner' ? (
          <DataJoinerNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'aggregator' ? (
          <AggregatorNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'data_validator' ? (
          <DataValidatorNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'deduplicator' ? (
          <DeduplicatorNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'rdf_loader' ? (
          <RDFLoaderNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'db_loader' ? (
          <DBLoaderNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : data.step_type === 'csv_exporter' ? (
          <CSVExporterNodeBody
            config={data.config}
            status={nodeBodyStatus}
            metrics={{
              rowsProcessed: data.config?.rowsProcessed,
              duration: data.executionDuration,
            }}
          />
        ) : (
          /* Generic body for ML/Fusion nodes */
          <div className="px-2.5 py-2.5">
            {/* Label */}
            <div className="text-[13px] font-semibold leading-tight text-foreground mb-1.5 truncate">
              {data.label}
            </div>

            {/* Config Summary */}
            {configSummary && configSummary.length > 0 && (
              <div className="space-y-0.5">
                {configSummary.slice(0, 2).map((summary, idx) => (
                  <div
                    key={idx}
                    className="text-[11px] text-muted-foreground truncate flex items-center gap-1.5"
                  >
                    <div className="w-0.5 h-0.5 rounded-full bg-muted-foreground/50 flex-shrink-0" />
                    {summary}
                  </div>
                ))}
              </div>
            )}

            {/* Execution Metrics - Confidence Progress Bar */}
            {data.executionConfidence !== undefined && (
              <div className="mt-2 flex items-center gap-2">
                <div className="flex-1 h-1.5 bg-muted rounded-full overflow-hidden">
                  <div
                    className="h-full bg-green-600 dark:bg-green-500 rounded-full transition-all duration-300"
                    style={{ width: `${data.executionConfidence * 100}%` }}
                  />
                </div>
                <span className="text-[11px] font-semibold text-muted-foreground">
                  {(data.executionConfidence * 100).toFixed(0)}%
                </span>
              </div>
            )}

            {/* Execution Duration */}
            {data.executionDuration !== undefined && (
              <div className="mt-1 text-[11px] text-muted-foreground/70">
                {data.executionDuration}ms
              </div>
            )}

            {/* Validation Error */}
            {data.validationError && (
              <div className="mt-2 flex items-center gap-1.5 px-2 py-1 bg-red-50 dark:bg-red-950/30 border border-red-500 dark:border-red-800 rounded text-[11px] font-medium text-red-600 dark:text-red-400">
                <AlertTriangle className="h-3 w-3" />
                Configuration required
              </div>
            )}
          </div>
        )}

        {/* Execution Status Badge */}
        {data.executionStatus && data.executionStatus !== 'idle' && (
          <div className="absolute -top-2 -right-2 bg-card rounded-full p-1.5 shadow-lg border-2 border-card">
            {data.executionStatus === 'executing' && (
              <Loader2 className="h-4 w-4 animate-spin text-blue-600 dark:text-blue-400" strokeWidth={2.5} />
            )}
            {data.executionStatus === 'success' && (
              <CheckCircle className="h-4 w-4 text-green-600 dark:text-green-500" strokeWidth={2.5} />
            )}
            {data.executionStatus === 'error' && (
              <XCircle className="h-4 w-4 text-red-600 dark:text-red-500" strokeWidth={2.5} />
            )}
          </div>
        )}
      </div>

      {/* Handles - Professional circles with dark mode */}
      {/* Left Handle (Input) - Hidden for extract nodes */}
      {showLeftHandle && (
        <Handle
          type="target"
          position={Position.Left}
          className={cn(
            '!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!w-3.5 hover:!h-3.5',
            isDark
              ? '!border-slate-700 !bg-slate-600 hover:!bg-blue-500'
              : '!border-white !bg-slate-400 hover:!bg-blue-600'
          )}
          style={{ left: -5 }}
        />
      )}

      {/* Right Handle (Output) - Hidden for load nodes */}
      {showRightHandle && (
        <Handle
          type="source"
          position={Position.Right}
          className={cn(
            '!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!w-3.5 hover:!h-3.5',
            isDark
              ? '!border-slate-700 !bg-slate-600 hover:!bg-blue-500'
              : '!border-white !bg-slate-400 hover:!bg-blue-600'
          )}
          style={{ right: -5 }}
        />
      )}

      {/* Action Toolbar - Fluent-style persistent + hover with buffer zone */}
      <div
        className={cn(
          "absolute -top-11 right-0 z-10 transition-all duration-100",
          selected
            ? "opacity-100 pointer-events-auto"
            : "opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto"
        )}
        style={{ paddingTop: '12px', marginTop: '-12px' }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex gap-1 bg-card border border-border rounded-md shadow-lg px-1.5 py-1.5">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 hover:bg-blue-50 dark:hover:bg-blue-950/30"
            onClick={handleConfigure}
            title="Configure (Double-click)"
            aria-label="Configure node"
          >
            <Settings className="h-3.5 w-3.5 text-blue-600 dark:text-blue-400" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 hover:bg-accent"
            onClick={handleDuplicate}
            title="Duplicate (Ctrl+D)"
            aria-label="Duplicate node"
          >
            <Copy className="h-3.5 w-3.5 text-muted-foreground" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 hover:bg-red-50 dark:hover:bg-red-950/30"
            onClick={handleDelete}
            title="Delete (Del)"
            aria-label="Delete node"
          >
            <Trash2 className="h-3.5 w-3.5 text-red-600 dark:text-red-500" />
          </Button>
        </div>
      </div>
    </div>
  );
}
