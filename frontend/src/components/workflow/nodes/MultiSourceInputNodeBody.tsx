/**
 * Multi-Source Input Node Body
 * Phase 2.1: Display multiple sources with join configuration
 */

import React from 'react';
import { FolderInput, Star, Database, Merge, Calculator } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import type { MultiSourceInputConfig } from '@/lib/workflow-etl-config';

export interface MultiSourceInputNodeBodyProps {
  config?: MultiSourceInputConfig;
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
  onConfigure?: () => void;
}

export function MultiSourceInputNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onConfigure,
}: MultiSourceInputNodeBodyProps) {
  const sources = config?.sources || [];
  const primarySource = sources.find(s => s.isPrimary);
  const secondarySources = sources.filter(s => !s.isPrimary);

  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-neutral-600">Loading sources...</span>
            <span className="font-semibold text-neutral-700">{progress}%</span>
          </div>
          <div className="h-1.5 bg-neutral-100 rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-blue-500 to-blue-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-neutral-500 mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows processed
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && sources.length > 0) {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* Source count */}
        <div className="flex items-center gap-2 text-xs">
          <FolderInput className="w-3.5 h-3.5 text-blue-600" />
          <span className="font-medium text-neutral-700">
            {sources.length} source{sources.length !== 1 ? 's' : ''}
          </span>
        </div>

        {/* Primary source */}
        {primarySource && (
          <div className="p-2 bg-blue-50 border border-blue-200 rounded">
            <div className="flex items-center gap-1.5 text-xs mb-1">
              <Star className="w-3 h-3 text-blue-600 fill-blue-600" />
              <span className="font-semibold text-blue-700">Primary</span>
            </div>
            <div className="text-xs">
              <div className="font-mono font-medium text-neutral-700">{primarySource.alias}</div>
              <div className="text-neutral-500 mt-0.5">{primarySource.sourceName}</div>
              {primarySource.rowCount && (
                <div className="flex items-center gap-1 mt-1 text-neutral-600">
                  <Database className="w-3 h-3" />
                  <span>{primarySource.rowCount.toLocaleString()} rows</span>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Secondary sources with joins */}
        {secondarySources.length > 0 && (
          <div className="space-y-2">
            {secondarySources.map((source, idx) => (
              <div key={source.sourceId} className="p-2 bg-neutral-50 border border-neutral-200 rounded">
                <div className="flex items-center gap-1.5 text-xs mb-1">
                  <Merge className="w-3 h-3 text-green-600" />
                  {source.join && (
                    <Badge variant="outline" className="text-xs">
                      {source.join.type} JOIN
                    </Badge>
                  )}
                  {source.join?.aggregations && source.join.aggregations.length > 0 && (
                    <Badge variant="outline" className="text-xs bg-purple-50 text-purple-700 border-purple-200">
                      <Calculator className="w-2.5 h-2.5 mr-0.5" />
                      {source.join.aggregations.length} agg
                    </Badge>
                  )}
                </div>
                <div className="text-xs">
                  <div className="font-mono font-medium text-neutral-700">{source.alias}</div>
                  {source.join && (
                    <div className="text-neutral-500 mt-0.5 font-mono text-xs">
                      {primarySource?.alias}.{source.join.localField} = {source.alias}.{source.join.foreignField}
                    </div>
                  )}
                  {source.join?.aggregations && source.join.aggregations.length > 0 && (
                    <div className="mt-1 text-purple-700 text-xs">
                      {source.join.aggregations.slice(0, 2).map((agg, i) => (
                        <div key={i} className="font-mono">
                          {agg.operation}({agg.field})
                        </div>
                      ))}
                      {source.join.aggregations.length > 2 && (
                        <div className="text-neutral-500">
                          +{source.join.aggregations.length - 2} more
                        </div>
                      )}
                    </div>
                  )}
                  {source.rowCount && (
                    <div className="flex items-center gap-1 mt-1 text-neutral-600">
                      <Database className="w-3 h-3" />
                      <span>{source.rowCount.toLocaleString()} rows</span>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Metrics */}
        {metrics && (
          <div className="space-y-1 pt-2 border-t border-neutral-100">
            <div className="flex items-center gap-1.5 text-xs text-green-700">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-neutral-600 pl-2.5">
                {metrics.rowsProcessed.toLocaleString()} rows merged
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

  // Configured state (idle)
  if (sources.length > 0 && status === 'idle') {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* Source count */}
        <div className="flex items-center gap-2 text-xs">
          <FolderInput className="w-3.5 h-3.5 text-blue-600" />
          <span className="font-medium text-neutral-700">
            {sources.length} source{sources.length !== 1 ? 's' : ''}
          </span>
        </div>

        {/* Primary source */}
        {primarySource && (
          <div className="p-2 bg-blue-50 border border-blue-200 rounded">
            <div className="flex items-center gap-1.5 text-xs mb-1">
              <Star className="w-3 h-3 text-blue-600 fill-blue-600" />
              <span className="font-semibold text-blue-700">Primary</span>
            </div>
            <div className="text-xs">
              <div className="font-mono font-medium text-neutral-700">{primarySource.alias}</div>
              <div className="text-neutral-500 mt-0.5 truncate">{primarySource.sourceName}</div>
            </div>
          </div>
        )}

        {/* Secondary sources summary */}
        {secondarySources.length > 0 && (
          <div className="text-xs text-neutral-600">
            <div className="flex items-center gap-1.5">
              <Merge className="w-3 h-3" />
              <span>
                +{secondarySources.length} joined source{secondarySources.length !== 1 ? 's' : ''}
              </span>
            </div>
          </div>
        )}

        {/* Configure button */}
        {onConfigure && (
          <button
            onClick={onConfigure}
            className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
          >
            Configure Sources
          </button>
        )}
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5 mb-2">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to select sources
      </div>
      {onConfigure && (
        <button
          onClick={onConfigure}
          className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
        >
          Add Sources from Catalogue
        </button>
      )}
    </div>
  );
}
