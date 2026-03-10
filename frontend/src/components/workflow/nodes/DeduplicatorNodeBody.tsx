/**
 * Deduplicator Node Body
 * Remove duplicate records using exact, fuzzy, or semantic matching
 */

import React from 'react';
import { Copy, Filter, Target } from 'lucide-react';
import { InlineBadgeToggle } from '../widgets';
import type { DeduplicatorConfig } from '@/lib/workflow-etl-config';

export interface DeduplicatorNodeBodyProps {
  config?: DeduplicatorConfig;
  status?: 'idle' | 'running' | 'success' | 'error';
  progress?: number;
  metrics?: {
    rowsProcessed?: number;
    duration?: number;
    size?: number;
  };
  deduplicationMetrics?: {
    total_rows: number;
    duplicate_count: number;
    unique_count: number;
  };
  error?: {
    message: string;
    details?: string;
  };
  onModeChange?: (mode: 'exact' | 'fuzzy' | 'semantic') => void;
  onFieldsClick?: () => void;
  onThresholdChange?: (threshold: number) => void;
}

export function DeduplicatorNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  deduplicationMetrics,
  error,
  onModeChange,
  onFieldsClick,
  onThresholdChange,
}: DeduplicatorNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Detecting duplicates...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-teal-500 to-teal-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows processed
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success/Configured state
  if (config) {
    const keyFieldCount = config.key_fields?.length || 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Deduplication mode */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Dedup Mode</div>
          <InlineBadgeToggle
            value={config.method}
            options={[
              { value: 'exact', label: 'Exact', color: 'success' },
              { value: 'fuzzy', label: 'Fuzzy', color: 'warning' },
              { value: 'semantic', label: 'Semantic', color: 'secondary' },
            ]}
            onChange={(value) => {
              onModeChange?.(value as 'exact' | 'fuzzy' | 'semantic');
            }}
          />
        </div>

        {/* Key fields */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Filter className="w-3 h-3" />
            <span className="font-medium">Key Fields ({keyFieldCount})</span>
          </div>
          {keyFieldCount > 0 ? (
            <button
              onClick={onFieldsClick}
              className="w-full p-2 bg-muted hover:bg-muted border border-neutral-200 rounded text-left text-xs transition-colors"
            >
              <div className="text-foreground font-mono">
                {config.key_fields?.slice(0, 3).join(', ') || ''}
                {keyFieldCount > 3 && ` +${keyFieldCount - 3} more`}
              </div>
            </button>
          ) : (
            <div className="text-xs text-muted-foreground text-center py-2">
              No key fields configured
            </div>
          )}
        </div>

        {/* Threshold (for fuzzy/semantic) */}
        {(config.method === 'fuzzy' || config.method === 'semantic') && (
          <div className="p-2 bg-muted border border-neutral-200 rounded">
            <div className="flex items-center justify-between text-xs mb-1">
              <span className="text-muted-foreground">Match Threshold</span>
              <span className="font-medium text-foreground">{((config.threshold ?? 0.85) * 100).toFixed(0)}%</span>
            </div>
            <div className="h-1.5 bg-neutral-200 rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-teal-500 to-teal-400"
                style={{ width: `${(config.threshold ?? 0.85) * 100}%` }}
              />
            </div>
          </div>
        )}

        {/* Keep strategy */}
        <div className="flex items-center justify-between text-xs pt-2 border-t border-border">
          <span className="text-muted-foreground">On duplicate found:</span>
          <span className="font-medium text-foreground">
            Keep {config.keep}
          </span>
        </div>

        {/* Deduplication results */}
        {deduplicationMetrics && status === 'success' && (
          <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-1">
            <div className="font-medium text-foreground mb-1">Deduplication Results</div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Total rows:</span>
              <span className="font-medium">{deduplicationMetrics.total_rows.toLocaleString()}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-red-700">Duplicates removed:</span>
              <span className="font-medium">{deduplicationMetrics.duplicate_count.toLocaleString()}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-green-700 dark:text-green-500">Unique rows:</span>
              <span className="font-medium">{deduplicationMetrics.unique_count.toLocaleString()}</span>
            </div>
            <div className="flex items-center justify-between pt-1 border-t border-neutral-200">
              <span className="text-muted-foreground">Reduction:</span>
              <span className="font-medium text-teal-700">
                {deduplicationMetrics.total_rows > 0
                  ? ((deduplicationMetrics.duplicate_count / deduplicationMetrics.total_rows) * 100).toFixed(1)
                  : '0.0'
                }%
              </span>
            </div>
          </div>
        )}

        {/* Metrics */}
        {metrics && status === 'success' && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {deduplicationMetrics && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {deduplicationMetrics.unique_count.toLocaleString()} unique rows output
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  // Error state
  if (status === 'error' && error) {
    return (
      <div className="px-3 py-3">
        <div className="p-2 bg-red-50 border border-red-200 rounded text-xs">
          <div className="font-semibold text-red-700 mb-1">{error.message}</div>
          {error.details && <div className="text-red-600">{error.details}</div>}
        </div>
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5 mb-2">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure deduplication
      </div>
      {onFieldsClick && (
        <button
          onClick={onFieldsClick}
          className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
        >
          Select Match Fields
        </button>
      )}
    </div>
  );
}
