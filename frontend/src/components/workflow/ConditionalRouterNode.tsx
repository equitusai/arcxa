/**
 * Professional Enterprise Conditional Router Node Component
 * Diamond-shaped node for if-then-else branching logic with beautiful design
 * Phase 2.3: Theme-safe styling for light and dark modes
 */

import React from 'react';
import { Handle, Position, NodeProps } from 'reactflow';
import { GitBranch, CheckCircle, Loader2, XCircle, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { ConditionalRouterConfig } from '@/api/types';
import { getWorkflowCategoryColor } from '@/lib/workflow-colors';

export interface ConditionalRouterNodeData {
  label: string;
  step_type: 'conditional_router';
  config?: ConditionalRouterConfig;
  executionStatus?: 'idle' | 'executing' | 'success' | 'error';
  executionBranch?: 'true' | 'false'; // Which branch was taken
  validationError?: string;
}

export function ConditionalRouterNode({ data, selected, id }: NodeProps<ConditionalRouterNodeData>) {
  const routingColor = getWorkflowCategoryColor('routing');
  const gradientPrefix = `conditional-router-${String(id).replace(/[^a-zA-Z0-9_-]/g, '-')}`;

  const getStateColor = () => {
    if (data.executionStatus === 'executing') return 'hsl(var(--accent))';
    if (data.executionStatus === 'success') return 'hsl(var(--success))';
    if (data.executionStatus === 'error') return 'hsl(var(--error))';
    if (data.validationError) return 'hsl(var(--error))';
    if (selected) return 'hsl(var(--accent))';
    return routingColor.border;
  };

  const getFillGradient = () => {
    if (data.executionStatus === 'executing') return `url(#${gradientPrefix}-executing)`;
    if (data.executionStatus === 'success') return `url(#${gradientPrefix}-success)`;
    if (data.executionStatus === 'error' || data.validationError) return `url(#${gradientPrefix}-error)`;
    return `url(#${gradientPrefix}-idle)`;
  };

  const getShadowFilter = () => {
    if (selected) {
      return 'drop-shadow(0 0 14px hsl(var(--accent) / 0.22)) drop-shadow(0 12px 24px hsl(var(--accent) / 0.12))';
    }
    if (data.executionStatus === 'executing') {
      return 'drop-shadow(0 0 14px hsl(var(--accent) / 0.18)) drop-shadow(0 10px 20px hsl(var(--foreground) / 0.08))';
    }
    if (data.executionStatus === 'success') {
      return 'drop-shadow(0 0 12px hsl(var(--success) / 0.16)) drop-shadow(0 10px 20px hsl(var(--foreground) / 0.08))';
    }
    if (data.executionStatus === 'error') {
      return 'drop-shadow(0 0 12px hsl(var(--error) / 0.18)) drop-shadow(0 10px 20px hsl(var(--foreground) / 0.08))';
    }
    return 'drop-shadow(0 8px 18px hsl(var(--foreground) / 0.08))';
  };

  const conditionText = data.config?.condition || 'No condition set';
  const isShortCondition = conditionText.length <= 25;

  return (
    <div className="relative group">
      {/* Executing shimmer effect */}
      {data.executionStatus === 'executing' && (
        <div
          className="absolute -inset-2 rounded-lg overflow-hidden pointer-events-none"
          style={{
            background: 'linear-gradient(90deg, transparent 0%, rgba(0,120,212,0.15) 50%, transparent 100%)',
            backgroundSize: '200% 100%',
            animation: 'shimmer 2s linear infinite',
          }}
        />
      )}

      {/* Diamond Container */}
      <div className={cn(
        'relative transition-all duration-200',
        data.executionStatus === 'success' && 'workflow-node-success'
      )}>
        {/* SVG Diamond Shape */}
        <svg
          width="160"
          height="100"
          viewBox="0 0 160 100"
          className="transition-all duration-200"
          style={{
            filter: getShadowFilter(),
          }}
        >
          <defs>
            <linearGradient id={`${gradientPrefix}-idle`} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor={routingColor.surface} />
              <stop offset="100%" stopColor={routingColor.subtle} />
            </linearGradient>
            <linearGradient id={`${gradientPrefix}-executing`} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="hsl(var(--accent))" stopOpacity="0.12" />
              <stop offset="100%" stopColor="hsl(var(--accent))" stopOpacity="0.22" />
            </linearGradient>
            <linearGradient id={`${gradientPrefix}-success`} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="hsl(var(--success))" stopOpacity="0.12" />
              <stop offset="100%" stopColor="hsl(var(--success))" stopOpacity="0.22" />
            </linearGradient>
            <linearGradient id={`${gradientPrefix}-error`} x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="hsl(var(--error))" stopOpacity="0.12" />
              <stop offset="100%" stopColor="hsl(var(--error))" stopOpacity="0.22" />
            </linearGradient>
          </defs>

          {/* Diamond Shape */}
          <path
            d="M 80 6 L 150 50 L 80 94 L 10 50 Z"
            fill={getFillGradient()}
            stroke={getStateColor()}
            strokeWidth={selected ? 2.5 : 1.5}
          />

          {/* Inner highlight for depth */}
          <path
            d="M 80 10 L 146 50 L 80 90 L 14 50 Z"
            fill="none"
            stroke="hsl(var(--background))"
            strokeOpacity="0.72"
            strokeWidth="1"
          />

          {/* Branch Taken Highlight (after execution) */}
          {data.executionBranch === 'true' && (
            <circle
              cx="150"
              cy="50"
              r="5"
              fill="hsl(var(--success))"
              stroke="hsl(var(--background))"
              strokeWidth="2"
            />
          )}
          {data.executionBranch === 'false' && (
            <circle
              cx="10"
              cy="50"
              r="5"
              fill="hsl(var(--error))"
              stroke="hsl(var(--background))"
              strokeWidth="2"
            />
          )}
        </svg>

        {/* Content Overlay */}
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none px-6">
          {/* Icon */}
          <GitBranch className="h-5 w-5 mb-1.5" style={{ color: routingColor.text }} strokeWidth={2.5} />

          {/* IF Label */}
          <div className="text-[10px] font-bold uppercase tracking-wider mb-1" style={{ color: routingColor.text }}>
            IF
          </div>

          {/* Condition */}
          <div
            className={cn(
              'font-mono font-semibold text-center text-foreground px-2 leading-tight',
              isShortCondition ? 'text-[11px]' : 'text-[9px]'
            )}
            style={{
              maxWidth: '120px',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              display: '-webkit-box',
              WebkitLineClamp: isShortCondition ? 2 : 3,
              WebkitBoxOrient: 'vertical',
            }}
            title={conditionText}
          >
            {conditionText}
          </div>

          {/* Node Label (smaller, below condition) */}
          {data.label && (
            <div className="text-[9px] text-muted-foreground mt-1.5 truncate max-w-[110px]">
              {data.label}
            </div>
          )}

          {/* Validation Error Badge */}
          {data.validationError && (
            <div
              className="mt-1.5 flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[9px] font-medium pointer-events-auto"
              style={{
                backgroundColor: 'hsl(var(--error) / 0.08)',
                border: '1px solid hsl(var(--error) / 0.25)',
                color: 'hsl(var(--error))',
              }}
            >
              <AlertTriangle className="h-2.5 w-2.5" />
              Invalid
            </div>
          )}
        </div>

        {/* Execution Status Indicator */}
        {data.executionStatus && data.executionStatus !== 'idle' && (
          <div className="absolute top-0 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-card rounded-full p-1 shadow-md border-2 border-card">
            {data.executionStatus === 'executing' && (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-accent" strokeWidth={2.5} />
            )}
            {data.executionStatus === 'success' && (
              <CheckCircle className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />
            )}
            {data.executionStatus === 'error' && (
              <XCircle className="h-3.5 w-3.5 text-error" strokeWidth={2.5} />
            )}
          </div>
        )}
      </div>

      {/* Handles - Positioned at diamond points with dark mode */}
      {/* Top Handle (Target) */}
      <Handle
        type="target"
        position={Position.Top}
        className="!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!scale-110"
        style={{ top: 3, backgroundColor: routingColor.base, borderColor: 'hsl(var(--background))' }}
      />

      {/* Right Handle (Source - TRUE) */}
      <Handle
        type="source"
        position={Position.Right}
        id="true"
        className="!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!scale-110"
        style={{ right: 6, backgroundColor: 'hsl(var(--success))', borderColor: 'hsl(var(--background))' }}
      />

      {/* Left Handle (Source - FALSE) */}
      <Handle
        type="source"
        position={Position.Left}
        id="false"
        className="!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!scale-110"
        style={{ left: 6, backgroundColor: 'hsl(var(--error))', borderColor: 'hsl(var(--background))' }}
      />

      {/* Branch Labels (on hover) */}
      <div className="absolute right-0 top-1/2 -translate-y-1/2 translate-x-full opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none ml-3">
        <div className="px-2 py-0.5 bg-[#107C10] text-white text-[10px] font-semibold rounded shadow-md">
          TRUE
        </div>
      </div>
      <div className="absolute left-0 top-1/2 -translate-y-1/2 -translate-x-full opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none mr-3">
        <div className="px-2 py-0.5 bg-[#D13438] text-white text-[10px] font-semibold rounded shadow-md">
          FALSE
        </div>
      </div>
    </div>
  );
}
