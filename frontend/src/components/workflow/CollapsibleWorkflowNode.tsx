/**
 * Collapsible ETL Workflow Node Component
 * Sophisticated design with Framer Motion animations and inline configuration
 * Phase 2.3: Theme-safe styling for light and dark modes
 */

import React, { useState } from 'react';
import { Handle, Position, NodeProps } from 'reactflow';
import { motion, AnimatePresence } from 'framer-motion';
import { ChevronRight, MoreVertical } from 'lucide-react';
import { cn } from '@/lib/utils';
import { getStepTypeConfig } from '@/lib/workflow-step-config';
import { getETLStepTypeConfig, isETLStepType } from '@/lib/workflow-etl-config';
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
  MultiSourceInputNodeBody,
} from './nodes';

export interface CollapsibleNodeData {
  label: string;
  step_type: StepType;
  config?: any;
  // UI state
  collapsed?: boolean;
  // Execution state
  status?: 'idle' | 'running' | 'success' | 'error';
  progress?: number;
  // Metrics
  metrics?: {
    rowsProcessed?: number;
    duration?: number;
    size?: number;
  };
  error?: {
    message: string;
    details?: string;
  };
}

// Animation variants
const nodeVariants = {
  collapsed: {
    width: 160,
    height: 60,
    transition: { duration: 0.22, ease: [0.4, 0, 0.2, 1] as any },
  },
  expanded: {
    width: 280,
    height: 'auto' as any,
    transition: { duration: 0.22, ease: [0.4, 0, 0.2, 1] as any },
  },
};

const contentVariants = {
  collapsed: {
    opacity: 0,
    height: 0,
    display: 'none',
  },
  expanded: {
    opacity: 1,
    height: 'auto',
    display: 'block',
    transition: { delay: 0.1, duration: 0.16 },
  },
};

export function CollapsibleWorkflowNode({ data, selected }: NodeProps<CollapsibleNodeData>) {
  const [isCollapsed, setIsCollapsed] = useState(data.collapsed ?? false);

  // Get configuration based on node type
  const isETLNode = isETLStepType(data.step_type);
  const config = isETLNode
    ? getETLStepTypeConfig(data.step_type as any)
    : getStepTypeConfig(data.step_type);

  const StepIcon = config.icon;

  // Status dot color
  const getStatusColor = () => {
    switch (data.status) {
      case 'running':
        return 'bg-blue-500';
      case 'success':
        return 'bg-green-500';
      case 'error':
        return 'bg-red-500';
      default:
        return 'bg-gray-400 dark:bg-gray-600';
    }
  };

  // Border and shadow based on state with theme-safe tokens
  const getBorderStyle = () => {
    if (selected) {
      return {
        borderColor: 'hsl(var(--accent))',
        boxShadow: '0 0 0 1px hsl(var(--background)), 0 0 0 4px hsl(var(--accent) / 0.34), 0 18px 36px hsl(var(--accent) / 0.18)',
      };
    }

    switch (data.status) {
      case 'running':
        return {
          borderColor: 'hsl(var(--accent))',
          boxShadow: '0 14px 28px hsl(var(--accent) / 0.16), 0 4px 10px hsl(var(--foreground) / 0.06)',
        };
      case 'success':
        return {
          borderColor: 'hsl(var(--success))',
          boxShadow: '0 14px 28px hsl(var(--success) / 0.14), 0 4px 10px hsl(var(--foreground) / 0.06)',
        };
      case 'error':
        return {
          borderColor: 'hsl(var(--error))',
          boxShadow: '0 14px 28px hsl(var(--error) / 0.16), 0 4px 10px hsl(var(--foreground) / 0.06)',
        };
      default:
        return {
          borderColor: 'hsl(var(--border))',
          boxShadow: '0 10px 24px hsl(var(--foreground) / 0.08), 0 2px 6px hsl(var(--foreground) / 0.04)',
        };
    }
  };

  const handleToggle = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsCollapsed(!isCollapsed);
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).dataset.draggable !== 'true') {
      handleToggle(e);
    }
  };

  const borderStyle = getBorderStyle();

  return (
    <motion.div
      className={cn(
        'rounded-lg border-2 bg-card transition-shadow relative group',
        data.status === 'running' && 'animate-pulse-border'
      )}
      style={{
        borderColor: borderStyle.borderColor,
        boxShadow: borderStyle.boxShadow,
      }}
      variants={nodeVariants}
      initial={false}
      animate={isCollapsed ? 'collapsed' : 'expanded'}
      onDoubleClick={handleDoubleClick}
    >
      {/* HEADER (always visible) */}
      <div
        className="flex items-center gap-2 px-3 py-2 border-b border-border"
        style={{
          background: `linear-gradient(135deg, ${config.color.surface} 0%, ${config.color.subtle} 100%)`,
        }}
      >
        {/* Status dot */}
        <div className={cn('w-2 h-2 rounded-full', getStatusColor())} />

        {/* Icon */}
        <div className="w-5 h-5 rounded-sm bg-card border border-border-subtle flex items-center justify-center">
          <StepIcon className="w-4 h-4" style={{ color: config.color.text }} strokeWidth={2.5} />
        </div>

        {/* Label */}
        <span
          className={cn(
            'text-sm font-semibold flex-1',
            isCollapsed && 'truncate max-w-[80px]'
          )}
          style={{ color: config.color.text }}
        >
          {isCollapsed ? config.label : data.label}
        </span>

        {/* Toggle chevron */}
        <button
          onClick={handleToggle}
          className="w-5 h-5 hover:bg-background-secondary rounded-sm transition-colors flex items-center justify-center"
          aria-label={isCollapsed ? 'Expand node' : 'Collapse node'}
        >
          <ChevronRight
            className={cn(
              'w-4 h-4 transition-transform duration-220',
              !isCollapsed && 'rotate-90'
            )}
            style={{ color: config.color.text }}
          />
        </button>

        {/* Menu (visible on hover) */}
        <button className="w-5 h-5 hover:bg-background-secondary rounded-sm transition-colors opacity-0 group-hover:opacity-100 flex items-center justify-center">
          <MoreVertical className="w-4 h-4" style={{ color: config.color.text }} />
        </button>
      </div>

      {/* COLLAPSED FOOTER (minimal status) */}
      <AnimatePresence>
        {isCollapsed && (
          <motion.div
            variants={contentVariants}
            initial="collapsed"
            animate="expanded"
            exit="collapsed"
            className="px-3 py-1.5 border-t border-border flex items-center justify-between"
          >
            <span className="text-xs font-medium text-muted-foreground capitalize">{data.status || 'Idle'}</span>
            {data.progress !== undefined && (
              <span className="text-xs text-muted-foreground/70">{data.progress}%</span>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      {/* EXPANDED CONTENT (full configuration) */}
      <AnimatePresence>
        {!isCollapsed && (
          <motion.div
            variants={contentVariants}
            initial="collapsed"
            animate="expanded"
            exit="collapsed"
            className="border-t border-border"
          >
            {/* Render node-specific body or generic content */}
            {data.step_type === 'csv_source' ? (
              <CSVSourceNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'db_extract' ? (
              <DBExtractNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : false /* multi_source_input check removed */ ? (
              <MultiSourceInputNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'semantic_mapper' ? (
              <SemanticMapperNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'field_transformer' ? (
              <FieldTransformerNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'data_joiner' ? (
              <DataJoinerNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'aggregator' ? (
              <AggregatorNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'data_validator' ? (
              <DataValidatorNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'deduplicator' ? (
              <DeduplicatorNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'rdf_loader' ? (
              <RDFLoaderNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'db_loader' ? (
              <DBLoaderNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : data.step_type === 'csv_exporter' ? (
              <CSVExporterNodeBody
                config={data.config}
                status={data.status}
                progress={data.progress}
                metrics={data.metrics}
                error={data.error}
              />
            ) : (
              /* Generic node body content */
              <div className="px-3 py-3">
                {/* Running state */}
                {data.status === 'running' && data.progress !== undefined && (
                  <div className="mb-3">
                    <div className="flex items-center justify-between text-xs mb-1">
                      <span className="text-muted-foreground">Processing...</span>
                      <span className="font-semibold text-foreground">{data.progress}%</span>
                    </div>
                    <div className="h-1.5 bg-muted rounded-full overflow-hidden">
                      <div
                        className="h-full rounded-full transition-all duration-300"
                        style={{
                          width: `${data.progress}%`,
                          background: 'linear-gradient(90deg, hsl(var(--accent)) 0%, hsl(var(--accent) / 0.8) 100%)',
                        }}
                      />
                    </div>
                    {data.metrics?.rowsProcessed && (
                      <div className="text-xs text-muted-foreground mt-1">
                        {data.metrics.rowsProcessed.toLocaleString()} rows
                      </div>
                    )}
                  </div>
                )}

                {/* Success state */}
                {data.status === 'success' && data.metrics && (
                  <div className="space-y-1">
                    <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
                      <div className="w-1 h-1 rounded-full bg-green-500" />
                      Complete ({data.metrics.duration}ms)
                    </div>
                    {data.metrics.rowsProcessed && (
                      <div className="text-xs text-muted-foreground pl-2.5">
                        {data.metrics.rowsProcessed.toLocaleString()} rows
                      </div>
                    )}
                    {data.metrics.size && (
                      <div className="text-xs text-muted-foreground pl-2.5">
                        {(data.metrics.size / 1024).toFixed(1)} KB
                      </div>
                    )}
                  </div>
                )}

                {/* Error state */}
                {data.status === 'error' && data.error && (
                  <div
                    className="p-2 rounded text-xs"
                    style={{
                      backgroundColor: 'hsl(var(--error) / 0.08)',
                      border: '1px solid hsl(var(--error) / 0.22)',
                    }}
                  >
                    <div className="font-semibold mb-1 text-error">{data.error.message}</div>
                    {data.error.details && (
                      <div style={{ color: 'hsl(var(--error) / 0.82)' }}>{data.error.details}</div>
                    )}
                  </div>
                )}

                {/* Idle/configured state */}
                {data.status === 'idle' && data.config && (
                  <div className="text-xs text-muted-foreground">
                    <div className="font-medium text-foreground mb-1">Configured</div>
                    {/* Configuration summary - will be customized per node type */}
                    <div className="space-y-0.5">
                      {Object.entries(data.config)
                        .slice(0, 2)
                        .map(([key, value]) => (
                          <div key={key} className="flex items-start gap-1">
                            <div className="w-1 h-1 rounded-full bg-muted-foreground/50 mt-1.5 flex-shrink-0" />
                            <span className="truncate">
                              {key}: {String(value).substring(0, 20)}
                            </span>
                          </div>
                        ))}
                    </div>
                  </div>
                )}

                {/* Unconfigured state */}
                {!data.config && data.status !== 'running' && (
                  <div className="text-xs text-amber-600 dark:text-amber-500 flex items-center gap-1.5">
                    <div className="w-1 h-1 rounded-full bg-amber-500" />
                    Click to configure
                  </div>
                )}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Connection handles (always present) with dark mode */}
      <Handle
        type="target"
        position={Position.Left}
        className="!w-3 !h-3 !border-2 !shadow-md transition-all hover:!w-4 hover:!h-4 hover:!scale-110"
        style={{
          left: -6,
          top: '50%',
          transform: 'translateY(-50%)',
          backgroundColor: config.color.base,
          borderColor: 'hsl(var(--background))',
        }}
      />
      <Handle
        type="source"
        position={Position.Right}
        className="!w-3 !h-3 !border-2 !shadow-md transition-all hover:!w-4 hover:!h-4 hover:!scale-110"
        style={{
          right: -6,
          top: '50%',
          transform: 'translateY(-50%)',
          backgroundColor: config.color.base,
          borderColor: 'hsl(var(--background))',
        }}
      />
    </motion.div>
  );
}
