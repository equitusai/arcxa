/**
 * Aggregator Node Body
 * GROUP BY with aggregation functions (SUM, AVG, COUNT, MIN, MAX)
 */

import React from 'react';
import { Sigma, Hash, ChevronRight } from 'lucide-react';
import type { AggregatorConfig } from '@/lib/workflow-etl-config';

export interface AggregatorNodeBodyProps {
  config?: AggregatorConfig;
  status?: 'idle' | 'running' | 'success' | 'error';
  progress?: number;
  metrics?: {
    rowsProcessed?: number;
    duration?: number;
    size?: number;
  };
  error?: {
    message: string;
    details?: string;
  };
  onAddGroupBy?: () => void;
  onAddAggregation?: () => void;
  onEditAggregation?: (index: number) => void;
}

export function AggregatorNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onAddGroupBy,
  onAddAggregation,
  onEditAggregation,
}: AggregatorNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Aggregating data...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-indigo-500 to-indigo-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows aggregated
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success/Configured state
  if (config) {
    const groupByCount = config.group_by?.length || 0;
    const aggregationCount = config.aggregations?.length || 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Group by columns */}
        <div>
          <div className="flex items-center justify-between mb-1">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Hash className="w-3 h-3" />
              <span className="font-medium">Group By ({groupByCount})</span>
            </div>
            {onAddGroupBy && (
              <button
                onClick={onAddGroupBy}
                className="text-xs text-blue-600 hover:text-blue-700 font-medium"
              >
                + Add
              </button>
            )}
          </div>
          {groupByCount > 0 ? (
            <div className="p-2 bg-muted border border-neutral-200 rounded text-xs">
              <div className="text-foreground font-mono">
                {config.group_by?.slice(0, 3).join(', ') || ''}
                {groupByCount > 3 && ` +${groupByCount - 3} more`}
              </div>
            </div>
          ) : (
            <div className="text-xs text-muted-foreground text-center py-2">
              No group by columns configured
            </div>
          )}
        </div>

        {/* Aggregations */}
        <div>
          <div className="flex items-center justify-between mb-1">
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Sigma className="w-3 h-3" />
              <span className="font-medium">Aggregations ({aggregationCount})</span>
            </div>
            {onAddAggregation && (
              <button
                onClick={onAddAggregation}
                className="text-xs text-blue-600 hover:text-blue-700 font-medium"
              >
                + Add
              </button>
            )}
          </div>

          {aggregationCount > 0 ? (
            <div className="space-y-1.5">
              {config.aggregations?.slice(0, 4).map((agg, idx) => (
                <button
                  key={idx}
                  onClick={() => onEditAggregation?.(idx)}
                  className="w-full p-2 bg-muted hover:bg-muted border border-neutral-200 rounded text-left transition-colors"
                >
                  <div className="flex items-center justify-between">
                    <div className="text-xs">
                      <div className="flex items-center gap-1.5 mb-0.5">
                        <span className={`px-1.5 py-0.5 rounded font-medium ${
                          agg.function === 'SUM' ? 'bg-purple-100 text-purple-700' :
                          agg.function === 'AVG' ? 'bg-blue-100 text-blue-700' :
                          agg.function === 'COUNT' ? 'bg-green-100 text-green-700' :
                          agg.function === 'MIN' ? 'bg-orange-100 text-orange-700' :
                          'bg-red-100 text-red-700'
                        }`}>
                          {agg.function}
                        </span>
                        <span className="font-medium text-foreground">{agg.field}</span>
                      </div>
                      {agg.alias && (
                        <div className="text-muted-foreground pl-0.5">
                          as {agg.alias}
                        </div>
                      )}
                    </div>
                    <ChevronRight className="w-3 h-3 text-neutral-400" />
                  </div>
                </button>
              ))}
              {aggregationCount > 4 && (
                <div className="text-xs text-muted-foreground text-center py-1">
                  + {aggregationCount - 4} more...
                </div>
              )}
            </div>
          ) : (
            <div className="text-xs text-muted-foreground text-center py-2">
              No aggregations configured
            </div>
          )}
        </div>

        {/* Metrics */}
        {metrics && status === 'success' && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {metrics.rowsProcessed.toLocaleString()} groups output
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
        Click to configure aggregation
      </div>
      <div className="space-y-2">
        {onAddGroupBy && (
          <button
            onClick={onAddGroupBy}
            className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
          >
            + Add Group By Columns
          </button>
        )}
        {onAddAggregation && (
          <button
            onClick={onAddAggregation}
            className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
          >
            + Add Aggregation
          </button>
        )}
      </div>
    </div>
  );
}
