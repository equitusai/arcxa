/**
 * Execution History Viewer
 * Displays past workflow executions with status, timing, and confidence
 * Addresses UX Issue C-3: Provide execution history viewer
 */

import React, { useState } from 'react';
import { CheckCircle, XCircle, Clock, TrendingUp, Loader2, AlertCircle, ChevronRight } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import type { WorkflowExecutionSummary } from '@/api/types';
import { cn } from '@/lib/utils';

export interface ExecutionHistoryViewerProps {
  /**
   * Workflow ID to fetch executions for
   */
  workflowId?: string;

  /**
   * Execution history data
   */
  executions?: WorkflowExecutionSummary[];

  /**
   * Whether data is loading
   */
  isLoading?: boolean;

  /**
   * Error message if fetch failed
   */
  error?: string;

  /**
   * Click handler when execution row is clicked
   */
  onExecutionClick?: (execution: WorkflowExecutionSummary) => void;

  /**
   * Refresh data handler
   */
  onRefresh?: () => void;
}

/**
 * Format timestamp to relative time
 */
function formatRelativeTime(timestamp: string): string {
  const now = new Date();
  const date = new Date(timestamp);
  const diffMs = now.getTime() - date.getTime();

  const diffSeconds = Math.floor(diffMs / 1000);
  const diffMinutes = Math.floor(diffSeconds / 60);
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffSeconds < 60) return `${diffSeconds}s ago`;
  if (diffMinutes < 60) return `${diffMinutes}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined,
  });
}

/**
 * Format duration in milliseconds to human-readable
 */
function formatDuration(durationMs: number): string {
  const seconds = Math.floor(durationMs / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);

  if (hours > 0) {
    return `${hours}h ${minutes % 60}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
  }
  if (seconds > 0) {
    return `${seconds}s`;
  }
  return `${durationMs}ms`;
}

/**
 * Format confidence score as percentage
 */
function formatConfidence(confidence: number): string {
  return `${(confidence * 100).toFixed(1)}%`;
}

/**
 * Get status badge color
 */
function getStatusColor(success: boolean): {
  bg: string;
  text: string;
  border: string;
  icon: React.ReactNode;
} {
  if (success) {
    return {
      bg: 'bg-green-50',
      text: 'text-green-700',
      border: 'border-green-200',
      icon: <CheckCircle className="h-4 w-4" />,
    };
  }

  return {
    bg: 'bg-red-50',
    text: 'text-red-700',
    border: 'border-red-200',
    icon: <XCircle className="h-4 w-4" />,
  };
}

export function ExecutionHistoryViewer({
  workflowId,
  executions = [],
  isLoading = false,
  error,
  onExecutionClick,
  onRefresh,
}: ExecutionHistoryViewerProps) {
  const [hoveredRow, setHoveredRow] = useState<string | null>(null);

  // Loading state
  if (isLoading) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground mb-3" />
        <p className="text-sm text-muted-foreground">Loading execution history...</p>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <AlertCircle className="h-8 w-8 text-red-500 mb-3" />
        <p className="text-sm font-medium text-red-700 mb-1">Failed to load execution history</p>
        <p className="text-xs text-red-600 mb-4">{error}</p>
        {onRefresh && (
          <Button size="sm" variant="outline" onClick={onRefresh}>
            Try Again
          </Button>
        )}
      </div>
    );
  }

  // No workflow selected
  if (!workflowId) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <Clock className="h-8 w-8 text-muted-foreground mb-3" />
        <p className="text-sm text-muted-foreground">No workflow selected</p>
        <p className="text-xs text-muted-foreground mt-1">
          Save a workflow to view execution history
        </p>
      </div>
    );
  }

  // Empty state
  if (executions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12">
        <Clock className="h-8 w-8 text-muted-foreground mb-3" />
        <p className="text-sm font-medium text-foreground mb-1">No executions yet</p>
        <p className="text-xs text-muted-foreground mb-4">
          Execute the workflow to see history here
        </p>
        {onRefresh && (
          <Button size="sm" variant="outline" onClick={onRefresh}>
            Refresh
          </Button>
        )}
      </div>
    );
  }

  // Execution list
  return (
    <div className="space-y-2">
      {/* Header */}
      <div className="flex items-center justify-between mb-2">
        <p className="text-sm font-medium text-foreground">
          {executions.length} execution{executions.length !== 1 ? 's' : ''}
        </p>
        {onRefresh && (
          <Button size="sm" variant="ghost" onClick={onRefresh}>
            Refresh
          </Button>
        )}
      </div>

      {/* Execution rows */}
      <div className="space-y-1.5">
        {executions.map((execution) => {
          const statusColors = getStatusColor(execution.success);
          const isHovered = hoveredRow === execution.execution_id;

          return (
            <div
              key={execution.execution_id}
              className={cn(
                'flex items-center gap-3 p-3 rounded-lg border transition-all duration-150',
                onExecutionClick && 'cursor-pointer hover:shadow-sm',
                isHovered ? 'border-primary/50 bg-accent/30' : 'border-border bg-background'
              )}
              onClick={() => onExecutionClick?.(execution)}
              onMouseEnter={() => setHoveredRow(execution.execution_id)}
              onMouseLeave={() => setHoveredRow(null)}
            >
              {/* Status Icon */}
              <div
                className={cn(
                  'flex items-center justify-center w-9 h-9 rounded-full border-2',
                  statusColors.bg,
                  statusColors.border,
                  statusColors.text
                )}
              >
                {statusColors.icon}
              </div>

              {/* Execution Info */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-0.5">
                  <span className="text-sm font-medium text-foreground font-mono truncate">
                    {execution.execution_id.slice(0, 12)}...
                  </span>
                  <Badge
                    variant={execution.success ? 'default' : 'destructive'}
                    className="text-xs"
                  >
                    {execution.success ? 'Success' : 'Failed'}
                  </Badge>
                </div>

                <div className="flex items-center gap-3 text-xs text-muted-foreground">
                  <div className="flex items-center gap-1">
                    <Clock className="h-3 w-3" />
                    <span>{formatRelativeTime(execution.started_at)}</span>
                  </div>
                  <span>•</span>
                  <div className="flex items-center gap-1">
                    <span className="font-medium">{formatDuration(execution.duration_ms)}</span>
                  </div>
                  <span>•</span>
                  <div className="flex items-center gap-1">
                    <TrendingUp className="h-3 w-3" />
                    <span className="font-medium">{formatConfidence(execution.confidence)}</span>
                  </div>
                </div>
              </div>

              {/* Click indicator */}
              {onExecutionClick && (
                <div className={cn('transition-opacity', isHovered ? 'opacity-100' : 'opacity-0')}>
                  <ChevronRight className="h-4 w-4 text-muted-foreground" />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
