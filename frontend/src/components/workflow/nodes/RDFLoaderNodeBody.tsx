/**
 * RDF Loader Node Body
 * Load entities into triple store with lineage tracking
 */

import React from 'react';
import { Database, GitBranch, Key, Layers, Package } from 'lucide-react';
import type { RDFLoaderConfig } from '@/lib/workflow-etl-config';

export interface RDFLoaderNodeBodyProps {
  config?: RDFLoaderConfig;
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
  onGraphClick?: () => void;
  onEntityTypeClick?: () => void;
  onIdFieldClick?: () => void;
}

export function RDFLoaderNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onGraphClick,
  onEntityTypeClick,
  onIdFieldClick,
}: RDFLoaderNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Loading to RDF store...</span>
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
              {metrics.rowsProcessed.toLocaleString()} entities created
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
        {/* Target graph */}
        {config.target_graph && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Database className="w-3 h-3" />
              <span className="font-medium">Target Graph</span>
            </div>
            <button
              onClick={onGraphClick}
              className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate font-mono bg-muted rounded px-2 py-1"
            >
              {config.target_graph}
            </button>
          </div>
        )}

        {/* Entity type */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Layers className="w-3 h-3" />
            <span className="font-medium">Entity Type</span>
          </div>
          <button
            onClick={onEntityTypeClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.entity_type}
          </button>
        </div>

        {/* ID field */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Key className="w-3 h-3" />
            <span className="font-medium">ID Field</span>
          </div>
          <button
            onClick={onIdFieldClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.id_field}
          </button>
        </div>

        {/* Configuration summary */}
        <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Configuration</div>
          <div className="flex items-start gap-1.5">
            <Package className="w-3 h-3 text-muted-foreground mt-0.5 flex-shrink-0" />
            <span className="text-muted-foreground">
              Batch size: {config.batch_size ? config.batch_size.toLocaleString() : '1000 (default)'}
            </span>
          </div>
          {config.capture_lineage && (
            <div className="flex items-start gap-1.5">
              <GitBranch className="w-3 h-3 text-green-500 mt-0.5 flex-shrink-0" />
              <span className="text-green-700 dark:text-green-500">Lineage capture enabled</span>
            </div>
          )}
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
                {metrics.rowsProcessed.toLocaleString()} entities created
              </div>
            )}
            <div className="text-xs text-muted-foreground pl-2.5">
              {((metrics.rowsProcessed || 0) * 3).toLocaleString()} triples stored (avg)
            </div>
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
        {/* Target graph */}
        {config.target_graph && (
          <div>
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Database className="w-3 h-3" />
              <span className="font-medium">Target Graph</span>
            </div>
            <button
              onClick={onGraphClick}
              className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate font-mono bg-muted rounded px-2 py-1"
            >
              {config.target_graph}
            </button>
          </div>
        )}

        {/* Entity type */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Layers className="w-3 h-3" />
            <span className="font-medium">Entity Type</span>
          </div>
          <button
            onClick={onEntityTypeClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.entity_type}
          </button>
        </div>

        {/* ID field */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Key className="w-3 h-3" />
            <span className="font-medium">ID Field</span>
          </div>
          <button
            onClick={onIdFieldClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.id_field}
          </button>
        </div>

        {/* Configuration summary */}
        <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Configuration</div>
          <div className="flex items-start gap-1.5">
            <Package className="w-3 h-3 text-muted-foreground mt-0.5 flex-shrink-0" />
            <span className="text-muted-foreground">
              Batch size: {config.batch_size ? config.batch_size.toLocaleString() : '1000 (default)'}
            </span>
          </div>
          {config.capture_lineage && (
            <div className="flex items-start gap-1.5">
              <GitBranch className="w-3 h-3 text-green-500 mt-0.5 flex-shrink-0" />
              <span className="text-green-700 dark:text-green-500">Lineage capture enabled</span>
            </div>
          )}
        </div>
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure RDF loader
      </div>
    </div>
  );
}
