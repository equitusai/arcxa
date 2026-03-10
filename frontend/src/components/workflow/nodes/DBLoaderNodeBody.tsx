/**
 * Database Loader Node Body
 * Displays datasource, table, load mode, and batch configuration
 */

import React from 'react';
import { Database, Upload, Key, Layers } from 'lucide-react';
import { InlineBadgeToggle } from '../widgets';
import type { DBLoaderConfig } from '@/lib/workflow-etl-config';

export interface DBLoaderNodeBodyProps {
  config?: DBLoaderConfig;
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
  onModeChange?: (mode: 'insert' | 'upsert' | 'replace') => void;
}

export function DBLoaderNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onDatasourceClick,
  onTableClick,
  onModeChange,
}: DBLoaderNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Loading to database...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-purple-500 to-purple-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows loaded
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

        {/* Table */}
        {config.table_name && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Layers className="w-3 h-3" />
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

        {/* Load mode */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Load Mode</div>
          <InlineBadgeToggle
            value={config.mode}
            options={[
              { value: 'insert', label: 'Insert', color: 'success' },
              { value: 'upsert', label: 'Upsert', color: 'warning' },
              { value: 'replace', label: 'Replace', color: 'danger' },
            ]}
            onChange={(value) => {
              if (onModeChange) {
                onModeChange(value as 'insert' | 'upsert' | 'replace');
              }
            }}
          />
        </div>

        {/* Configuration summary */}
        <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Configuration</div>
          {config.key_fields && config.key_fields.length > 0 && (
            <div className="flex items-start gap-1.5">
              <Key className="w-3 h-3 text-muted-foreground mt-0.5 flex-shrink-0" />
              <span className="text-muted-foreground">
                Keys: {config.key_fields.join(', ')}
              </span>
            </div>
          )}
          <div className="flex items-start gap-1.5">
            <Upload className="w-3 h-3 text-muted-foreground mt-0.5 flex-shrink-0" />
            <span className="text-muted-foreground">
              Batch size: {config.batch_size ? config.batch_size.toLocaleString() : '1000 (default)'}
            </span>
          </div>
        </div>

        {/* Metrics */}
        {metrics && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {metrics.rowsProcessed.toLocaleString()} rows loaded
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

        {/* Table */}
        {config.table_name && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Layers className="w-3 h-3" />
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

        {/* Load mode */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Load Mode</div>
          <InlineBadgeToggle
            value={config.mode}
            options={[
              { value: 'insert', label: 'Insert', color: 'success' },
              { value: 'upsert', label: 'Upsert', color: 'warning' },
              { value: 'replace', label: 'Replace', color: 'danger' },
            ]}
            onChange={(value) => {
              if (onModeChange) {
                onModeChange(value as 'insert' | 'upsert' | 'replace');
              }
            }}
          />
        </div>

        {/* Configuration summary */}
        <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Configuration</div>
          {config.key_fields && config.key_fields.length > 0 && (
            <div className="flex items-start gap-1.5">
              <Key className="w-3 h-3 text-muted-foreground mt-0.5 flex-shrink-0" />
              <span className="text-muted-foreground">
                Keys: {config.key_fields.join(', ')}
              </span>
            </div>
          )}
          <div className="flex items-start gap-1.5">
            <Upload className="w-3 h-3 text-muted-foreground mt-0.5 flex-shrink-0" />
            <span className="text-muted-foreground">
              Batch size: {config.batch_size ? config.batch_size.toLocaleString() : '1000 (default)'}
            </span>
          </div>
        </div>
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure database loader
      </div>
    </div>
  );
}
