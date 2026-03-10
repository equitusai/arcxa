/**
 * Field Transformer Node Body
 * Apply transformations (TRIM, LOWER, REGEX, etc.) to fields
 */

import React from 'react';
import { Wand2, ChevronRight } from 'lucide-react';
import type { FieldTransformerConfig } from '@/lib/workflow-etl-config';

export interface FieldTransformerNodeBodyProps {
  config?: FieldTransformerConfig;
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
  onAddTransformation?: () => void;
  onEditTransformation?: (index: number) => void;
}

export function FieldTransformerNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onAddTransformation,
  onEditTransformation,
}: FieldTransformerNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Transforming fields...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-orange-500 to-orange-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows transformed
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && config) {
    const transformationCount = config.transformations?.length || 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Transformation count */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-xs text-foreground">
            <Wand2 className="w-3 h-3" />
            <span className="font-medium">
              {transformationCount} Transformation{transformationCount !== 1 ? 's' : ''}
            </span>
          </div>
          {onAddTransformation && (
            <button
              onClick={onAddTransformation}
              className="text-xs text-blue-600 hover:text-blue-700 font-medium"
            >
              + Add
            </button>
          )}
        </div>

        {/* Transformation list */}
        {config.transformations && config.transformations.length > 0 && (
          <div className="space-y-1.5">
            {config.transformations.slice(0, 3).map((transform, idx) => (
              <button
                key={idx}
                onClick={() => onEditTransformation?.(idx)}
                className="w-full p-2 bg-muted hover:bg-muted border border-neutral-200 rounded text-left transition-colors"
              >
                <div className="flex items-center justify-between">
                  <div className="text-xs">
                    <div className="font-medium text-foreground mb-0.5">
                      {transform.field}
                    </div>
                    <div className="text-muted-foreground">
                      {transform.operations.length} operation{transform.operations.length !== 1 ? 's' : ''}
                      {transform.operations.length > 0 && (
                        <span className="ml-1">
                          ({transform.operations.map(op => op.type).join(', ')})
                        </span>
                      )}
                    </div>
                  </div>
                  <ChevronRight className="w-3 h-3 text-neutral-400" />
                </div>
              </button>
            ))}
            {config.transformations.length > 3 && (
              <div className="text-xs text-muted-foreground text-center py-1">
                + {config.transformations.length - 3} more...
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
                {metrics.rowsProcessed.toLocaleString()} rows transformed
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
    const transformationCount = config.transformations?.length || 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Transformation count */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-xs text-foreground">
            <Wand2 className="w-3 h-3" />
            <span className="font-medium">
              {transformationCount} Transformation{transformationCount !== 1 ? 's' : ''}
            </span>
          </div>
          {onAddTransformation && (
            <button
              onClick={onAddTransformation}
              className="text-xs text-blue-600 hover:text-blue-700 font-medium"
            >
              + Add
            </button>
          )}
        </div>

        {/* Transformation list */}
        {config.transformations && config.transformations.length > 0 ? (
          <div className="space-y-1.5">
            {config.transformations.slice(0, 3).map((transform, idx) => (
              <button
                key={idx}
                onClick={() => onEditTransformation?.(idx)}
                className="w-full p-2 bg-muted hover:bg-muted border border-neutral-200 rounded text-left transition-colors"
              >
                <div className="flex items-center justify-between">
                  <div className="text-xs">
                    <div className="font-medium text-foreground mb-0.5">
                      {transform.field}
                    </div>
                    <div className="text-muted-foreground">
                      {transform.operations.length} operation{transform.operations.length !== 1 ? 's' : ''}
                      {transform.operations.length > 0 && (
                        <span className="ml-1">
                          ({transform.operations.map(op => op.type).join(', ')})
                        </span>
                      )}
                    </div>
                  </div>
                  <ChevronRight className="w-3 h-3 text-neutral-400" />
                </div>
              </button>
            ))}
            {config.transformations.length > 3 && (
              <div className="text-xs text-muted-foreground text-center py-1">
                + {config.transformations.length - 3} more...
              </div>
            )}
          </div>
        ) : (
          <div className="text-xs text-muted-foreground text-center py-2">
            No transformations configured
          </div>
        )}
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5 mb-2">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to add transformations
      </div>
      {onAddTransformation && (
        <button
          onClick={onAddTransformation}
          className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
        >
          + Add First Transformation
        </button>
      )}
    </div>
  );
}
