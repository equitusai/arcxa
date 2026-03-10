/**
 * CSV Source Node Body
 * Displays file path, detected fields, and scan timestamp
 */

import React from 'react';
import { FileText, Calendar } from 'lucide-react';
import { FieldList } from '../widgets';
import type { CSVSourceConfig } from '@/lib/workflow-etl-config';

export interface CSVSourceNodeBodyProps {
  config?: CSVSourceConfig;
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
  onEditPath?: () => void;
  onFieldClick?: (fieldName: string) => void;
}

export function CSVSourceNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onEditPath,
  onFieldClick,
}: CSVSourceNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Scanning CSV...</span>
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
              {metrics.rowsProcessed.toLocaleString()} rows scanned
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && config && (config.file_id || config.file_path)) {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* File path */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <FileText className="w-3 h-3" />
            <span className="font-medium">File</span>
          </div>
          <button
            onClick={onEditPath}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
            title={config.file_name || config.file_path}
          >
            {config.file_name || (config.file_path ? config.file_path.split('/').pop() : '') || config.file_path}
          </button>
        </div>

        {/* Detected fields */}
        {config.detected_fields && config.detected_fields.length > 0 && (
          <div>
            <div className="text-xs font-medium text-foreground mb-1.5">
              Detected Fields ({config.detected_fields.length})
            </div>
            <FieldList
              fields={config.detected_fields.map(field => ({
                name: field.name,
                type: field.type,
                onClick: onFieldClick ? () => onFieldClick(field.name) : undefined,
              }))}
              maxVisible={3}
            />
          </div>
        )}

        {/* Scan timestamp */}
        {config.last_scanned && (
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground pt-2 border-t border-border">
            <Calendar className="w-3 h-3" />
            <span>Scanned {new Date(config.last_scanned).toLocaleString()}</span>
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
                {metrics.rowsProcessed.toLocaleString()} rows
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
  if (config && (config.file_id || config.file_path) && status === 'idle') {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* File path */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <FileText className="w-3 h-3" />
            <span className="font-medium">File</span>
          </div>
          <button
            onClick={onEditPath}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
            title={config.file_name || config.file_path}
          >
            {config.file_name || (config.file_path ? config.file_path.split('/').pop() : '') || config.file_path}
          </button>
        </div>

        {/* Detected fields */}
        {config.detected_fields && config.detected_fields.length > 0 && (
          <div>
            <div className="text-xs font-medium text-foreground mb-1.5">
              Detected Fields ({config.detected_fields.length})
            </div>
            <FieldList
              fields={config.detected_fields.map(field => ({
                name: field.name,
                type: field.type,
                onClick: onFieldClick ? () => onFieldClick(field.name) : undefined,
              }))}
              maxVisible={3}
            />
          </div>
        )}

        {/* Scan timestamp */}
        {config.last_scanned && (
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground pt-2 border-t border-border">
            <Calendar className="w-3 h-3" />
            <span>Scanned {new Date(config.last_scanned).toLocaleString()}</span>
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
        Click to select CSV file
      </div>
    </div>
  );
}
