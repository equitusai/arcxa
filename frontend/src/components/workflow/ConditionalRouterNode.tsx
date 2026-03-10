/**
 * Professional Enterprise Conditional Router Node Component
 * Diamond-shaped node for if-then-else branching logic with beautiful design
 * Phase 2.3: Dark mode support with SVG gradients
 */

import React, { useMemo } from 'react';
import { Handle, Position, NodeProps } from 'reactflow';
import { Badge } from '@/components/ui/badge';
import { GitBranch, CheckCircle, Loader2, XCircle, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { ConditionalRouterConfig } from '@/api/types';

export interface ConditionalRouterNodeData {
  label: string;
  step_type: 'conditional_router';
  config?: ConditionalRouterConfig;
  executionStatus?: 'idle' | 'executing' | 'success' | 'error';
  executionBranch?: 'true' | 'false'; // Which branch was taken
  validationError?: string;
}

export function ConditionalRouterNode({ data, selected }: NodeProps<ConditionalRouterNodeData>) {
  // Phase 2.3: Dark mode detection
  const isDark = useMemo(
    () => document.documentElement.classList.contains('dark'),
    []
  );

  const getStateColor = () => {
    if (data.executionStatus === 'executing') return '#0078D4';
    if (data.executionStatus === 'success') return '#107C10';
    if (data.executionStatus === 'error') return '#D13438';
    if (data.validationError) return '#D13438';
    if (selected) return '#0078D4';
    return '#8764B8'; // Purple for conditional
  };

  const getFillGradient = () => {
    const prefix = isDark ? 'dark-' : '';

    if (data.executionStatus === 'executing') {
      return `url(#${prefix}gradient-executing)`;
    }
    if (data.executionStatus === 'success') {
      return `url(#${prefix}gradient-success)`;
    }
    if (data.executionStatus === 'error') {
      return `url(#${prefix}gradient-error)`;
    }
    if (data.validationError) {
      return `url(#${prefix}gradient-error)`;
    }
    return `url(#${prefix}gradient-idle)`;
  };

  const getShadowFilter = () => {
    if (selected) {
      return isDark
        ? 'drop-shadow(0 0 0 1px rgba(255,255,255,0.1)) drop-shadow(0 0 0 3px #0078D4) drop-shadow(0 2px 8px rgba(0,120,212,0.30))'
        : 'drop-shadow(0 0 0 1px white) drop-shadow(0 0 0 3px #0078D4) drop-shadow(0 2px 8px rgba(0,120,212,0.20))';
    }
    if (data.executionStatus === 'executing') {
      return isDark
        ? 'drop-shadow(0 0 12px rgba(0,120,212,0.4)) drop-shadow(0 2px 6px rgba(0,0,0,0.3))'
        : 'drop-shadow(0 0 12px rgba(0,120,212,0.3)) drop-shadow(0 2px 6px rgba(0,0,0,0.1))';
    }
    if (data.executionStatus === 'success') {
      return isDark
        ? 'drop-shadow(0 1px 4px rgba(16,124,16,0.25)) drop-shadow(0 1px 2px rgba(0,0,0,0.25))'
        : 'drop-shadow(0 1px 4px rgba(16,124,16,0.15)) drop-shadow(0 1px 2px rgba(0,0,0,0.06))';
    }
    if (data.executionStatus === 'error') {
      return isDark
        ? 'drop-shadow(0 1px 4px rgba(209,52,56,0.25)) drop-shadow(0 1px 2px rgba(0,0,0,0.25))'
        : 'drop-shadow(0 1px 4px rgba(209,52,56,0.15)) drop-shadow(0 1px 2px rgba(0,0,0,0.06))';
    }
    return isDark
      ? 'drop-shadow(0 1px 2px rgba(0,0,0,0.25))'
      : 'drop-shadow(0 1px 2px rgba(0,0,0,0.06))';
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
          {/* Gradient Definitions - Light Mode */}
          <defs>
            <linearGradient id="gradient-idle" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#F3E8FF" />
              <stop offset="100%" stopColor="#E9D5FF" />
            </linearGradient>
            <linearGradient id="gradient-executing" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#E3F2FD" />
              <stop offset="100%" stopColor="#BBDEFB" />
            </linearGradient>
            <linearGradient id="gradient-success" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#E8F5E9" />
              <stop offset="100%" stopColor="#C8E6C9" />
            </linearGradient>
            <linearGradient id="gradient-error" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#FFEBEE" />
              <stop offset="100%" stopColor="#FFCDD2" />
            </linearGradient>

            {/* Dark Mode Gradients */}
            <linearGradient id="dark-gradient-idle" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="rgba(135,100,184,0.2)" />
              <stop offset="100%" stopColor="rgba(135,100,184,0.3)" />
            </linearGradient>
            <linearGradient id="dark-gradient-executing" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="rgba(0,120,212,0.2)" />
              <stop offset="100%" stopColor="rgba(0,120,212,0.3)" />
            </linearGradient>
            <linearGradient id="dark-gradient-success" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="rgba(16,124,16,0.2)" />
              <stop offset="100%" stopColor="rgba(16,124,16,0.3)" />
            </linearGradient>
            <linearGradient id="dark-gradient-error" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="rgba(209,52,56,0.2)" />
              <stop offset="100%" stopColor="rgba(209,52,56,0.3)" />
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
            stroke={isDark ? "rgba(255,255,255,0.15)" : "rgba(255,255,255,0.6)"}
            strokeWidth="1"
          />

          {/* Branch Taken Highlight (after execution) */}
          {data.executionBranch === 'true' && (
            <circle
              cx="150"
              cy="50"
              r="5"
              fill="#107C10"
              stroke={isDark ? "rgba(255,255,255,0.2)" : "white"}
              strokeWidth="2"
            />
          )}
          {data.executionBranch === 'false' && (
            <circle
              cx="10"
              cy="50"
              r="5"
              fill="#D13438"
              stroke={isDark ? "rgba(255,255,255,0.2)" : "white"}
              strokeWidth="2"
            />
          )}
        </svg>

        {/* Content Overlay */}
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none px-6">
          {/* Icon */}
          <GitBranch className="h-5 w-5 text-purple-600 dark:text-purple-400 mb-1.5" strokeWidth={2.5} />

          {/* IF Label */}
          <div className="text-[10px] font-bold text-purple-600 dark:text-purple-400 uppercase tracking-wider mb-1">
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
            <div className="mt-1.5 flex items-center gap-0.5 px-1.5 py-0.5 bg-card border border-red-600 dark:border-red-500 rounded text-[9px] font-medium text-red-600 dark:text-red-500 pointer-events-auto">
              <AlertTriangle className="h-2.5 w-2.5" />
              Invalid
            </div>
          )}
        </div>

        {/* Execution Status Indicator */}
        {data.executionStatus && data.executionStatus !== 'idle' && (
          <div className="absolute top-0 left-1/2 -translate-x-1/2 -translate-y-1/2 bg-card rounded-full p-1 shadow-md border-2 border-card">
            {data.executionStatus === 'executing' && (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-blue-600 dark:text-blue-400" strokeWidth={2.5} />
            )}
            {data.executionStatus === 'success' && (
              <CheckCircle className="h-3.5 w-3.5 text-green-600 dark:text-green-500" strokeWidth={2.5} />
            )}
            {data.executionStatus === 'error' && (
              <XCircle className="h-3.5 w-3.5 text-red-600 dark:text-red-500" strokeWidth={2.5} />
            )}
          </div>
        )}
      </div>

      {/* Handles - Positioned at diamond points with dark mode */}
      {/* Top Handle (Target) */}
      <Handle
        type="target"
        position={Position.Top}
        className={cn(
          '!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!w-3.5 hover:!h-3.5',
          isDark
            ? '!border-slate-700 !bg-slate-600 hover:!bg-purple-500'
            : '!border-white !bg-slate-400 hover:!bg-purple-600'
        )}
        style={{ top: 3 }}
      />

      {/* Right Handle (Source - TRUE) */}
      <Handle
        type="source"
        position={Position.Right}
        id="true"
        className={cn(
          '!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!w-3.5 hover:!h-3.5',
          isDark
            ? '!border-slate-700 !bg-slate-600 hover:!bg-green-500'
            : '!border-white !bg-slate-400 hover:!bg-green-600'
        )}
        style={{ right: 6 }}
      />

      {/* Left Handle (Source - FALSE) */}
      <Handle
        type="source"
        position={Position.Left}
        id="false"
        className={cn(
          '!w-2.5 !h-2.5 !border-2 !shadow-sm transition-all hover:!w-3.5 hover:!h-3.5',
          isDark
            ? '!border-slate-700 !bg-slate-600 hover:!bg-red-500'
            : '!border-white !bg-slate-400 hover:!bg-red-600'
        )}
        style={{ left: 6 }}
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
