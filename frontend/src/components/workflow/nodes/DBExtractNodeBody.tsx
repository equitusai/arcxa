/**
 * Database Extract Node Body
 * Extract data from registered datasources with incremental support
 */

import React, { useState } from 'react';
import { Database, Table, Code, Clock, Columns } from 'lucide-react';
import { InlineBadgeToggle } from '../widgets';
import type { DBExtractConfig } from '@/lib/workflow-etl-config';

export interface DBExtractNodeBodyProps {
  config?: DBExtractConfig;
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
  onDatasourceClick?: () => void;
  onTableClick?: () => void;
  onQueryClick?: () => void;
  onIncrementalToggle?: (enabled: boolean) => void;
}

export function DBExtractNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onDatasourceClick,
  onTableClick,
  onQueryClick,
  onIncrementalToggle,
}: DBExtractNodeBodyProps) {
  const extractMode = config?.query ? 'query' : 'table';

  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Extracting data...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-blue-500 to-blue-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows extracted
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && config) {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* Datasource */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Database className="w-3 h-3" />
            <span className="font-medium">Datasource</span>
          </div>
          <button
            onClick={onDatasourceClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.datasource_id || 'Not configured'}
          </button>
        </div>

        {/* Extract mode: Table or Query */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Extract Mode</div>
          <InlineBadgeToggle
            value={extractMode}
            options={[
              { value: 'table', label: 'Table', color: 'success' },
              { value: 'query', label: 'Query', color: 'secondary' },
            ]}
            onChange={(value) => {
              // TODO: Implement mode toggle callback
            }}
          />
        </div>

        {/* Table name (if table mode) */}
        {extractMode === 'table' && config.table_name && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Table className="w-3 h-3" />
              <span className="font-medium">Table</span>
            </div>
            <button
              onClick={onTableClick}
              className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
            >
              {config.table_name}
            </button>
          </div>
        )}

        {/* Query (if query mode) */}
        {extractMode === 'query' && config.query && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Code className="w-3 h-3" />
              <span className="font-medium">Custom Query</span>
            </div>
            <button
              onClick={onQueryClick}
              className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors font-mono bg-muted rounded px-2 py-1"
            >
              {config.query.length > 50
                ? config.query.substring(0, 50) + '...'
                : config.query}
            </button>
          </div>
        )}

        {/* Incremental extraction */}
        {config.incremental && (
          <div className="p-2 bg-blue-50 border border-blue-200 rounded text-xs space-y-1">
            <div className="flex items-center gap-1.5 text-blue-700 font-medium">
              <Clock className="w-3 h-3" />
              Incremental Extraction
            </div>
            {config.incremental_column && (
              <div className="text-blue-600 pl-4.5">
                Column: {config.incremental_column}
              </div>
            )}
            {config.last_value && (
              <div className="text-blue-600 pl-4.5">
                Last value: {String(config.last_value)}
              </div>
            )}
          </div>
        )}

        {/* Metrics */}
        {metrics && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {metrics.rowsProcessed.toLocaleString()} rows extracted
              </div>
            )}
            {metrics.size && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {(metrics.size / 1024).toFixed(1)} KB
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
  if (config && status === 'idle') {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* Datasource */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Database className="w-3 h-3" />
            <span className="font-medium">Datasource</span>
          </div>
          <button
            onClick={onDatasourceClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.datasource_id || 'Not configured'}
          </button>
        </div>

        {/* Extract mode */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Extract Mode</div>
          <InlineBadgeToggle
            value={extractMode}
            options={[
              { value: 'table', label: 'Table', color: 'success' },
              { value: 'query', label: 'Query', color: 'secondary' },
            ]}
            onChange={(value) => {
              // TODO: Implement mode toggle callback
            }}
          />
        </div>

        {/* Table name */}
        {extractMode === 'table' && config.table_name && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Table className="w-3 h-3" />
              <span className="font-medium">Table</span>
            </div>
            <button
              onClick={onTableClick}
              className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
            >
              {config.table_name}
            </button>
          </div>
        )}

        {/* Query */}
        {extractMode === 'query' && config.query && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Code className="w-3 h-3" />
              <span className="font-medium">Custom Query</span>
            </div>
            <button
              onClick={onQueryClick}
              className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors font-mono bg-muted rounded px-2 py-1 truncate"
            >
              {config.query.length > 50
                ? config.query.substring(0, 50) + '...'
                : config.query}
            </button>
          </div>
        )}

        {/* Incremental extraction */}
        {config.incremental && (
          <div className="p-2 bg-blue-50 border border-blue-200 rounded text-xs space-y-1">
            <div className="flex items-center gap-1.5 text-blue-700 font-medium">
              <Clock className="w-3 h-3" />
              Incremental Extraction
            </div>
            {config.incremental_column && (
              <div className="text-blue-600 pl-4.5">
                Column: {config.incremental_column}
              </div>
            )}
            {config.last_value && (
              <div className="text-blue-600 pl-4.5">
                Last value: {String(config.last_value)}
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure database extraction
      </div>
    </div>
  );
}
