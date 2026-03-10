/**
 * CSV Exporter Node Body
 * Export data to CSV file with configurable options
 */

import React from 'react';
import { FileText, Download, Settings, CheckCircle } from 'lucide-react';
import type { CSVExporterConfig } from '@/lib/workflow-etl-config';

export interface CSVExporterNodeBodyProps {
  config?: CSVExporterConfig;
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
  onPathClick?: () => void;
  onSettingsClick?: () => void;
  onColumnsClick?: () => void;
  onDownloadFile?: () => void;
}

export function CSVExporterNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onPathClick,
  onSettingsClick,
  onColumnsClick,
  onDownloadFile,
}: CSVExporterNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Exporting to CSV...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-cyan-500 to-cyan-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows written
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && config) {
    const fileSize = metrics?.size ? `${(metrics.size / 1024 / 1024).toFixed(2)} MB` : null;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Output path */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <FileText className="w-3 h-3" />
            <span className="font-medium">Output File</span>
          </div>
          <button
            onClick={onPathClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate font-mono bg-muted rounded px-2 py-1"
          >
            {config.output_path}
          </button>
        </div>

        {/* Export settings */}
        <div className="p-2 bg-muted border border-neutral-200 rounded">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-1.5 text-xs text-foreground">
              <Settings className="w-3 h-3" />
              <span className="font-medium">Export Settings</span>
            </div>
            {onSettingsClick && (
              <button
                onClick={onSettingsClick}
                className="text-xs text-blue-600 hover:text-blue-700"
              >
                Edit
              </button>
            )}
          </div>

          <div className="space-y-1 text-xs">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Delimiter:</span>
              <span className="font-mono text-foreground">
                {config.delimiter === ',' ? 'Comma (,)' :
                 config.delimiter === '\t' ? 'Tab (\\t)' :
                 config.delimiter === ';' ? 'Semicolon (;)' :
                 config.delimiter === '|' ? 'Pipe (|)' :
                 `'${config.delimiter}'`}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Encoding:</span>
              <span className="font-mono text-foreground">{config.encoding}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Include header:</span>
              <span className={`font-medium ${config.include_header ? 'text-green-700' : 'text-muted-foreground'}`}>
                {config.include_header ? 'Yes' : 'No'}
              </span>
            </div>
          </div>
        </div>

        {/* Success metrics */}
        {metrics && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <CheckCircle className="w-3 h-3" />
              Export Complete
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-muted-foreground pl-4.5">
                {metrics.rowsProcessed.toLocaleString()} rows exported
              </div>
            )}
            {fileSize && (
              <div className="text-xs text-muted-foreground pl-4.5">
                File size: {fileSize}
              </div>
            )}
            {metrics.duration && (
              <div className="text-xs text-muted-foreground pl-4.5">
                Completed in {metrics.duration}ms
              </div>
            )}
          </div>
        )}

        {/* Download button (if available) */}
        <button
          type="button"
          onClick={onDownloadFile}
          disabled={!config?.output_path || !onDownloadFile}
          className="w-full px-2 py-1.5 text-xs font-medium text-green-700 bg-green-50 hover:bg-green-100 border border-green-200 rounded transition-colors flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:bg-green-50"
        >
          <Download className="w-3 h-3" />
          Download File
        </button>
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
        {/* Output path */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <FileText className="w-3 h-3" />
            <span className="font-medium">Output File</span>
          </div>
          <button
            onClick={onPathClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate font-mono bg-muted rounded px-2 py-1"
          >
            {config.output_path}
          </button>
        </div>

        {/* Export settings */}
        <div className="p-2 bg-muted border border-neutral-200 rounded">
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center gap-1.5 text-xs text-foreground">
              <Settings className="w-3 h-3" />
              <span className="font-medium">Export Settings</span>
            </div>
            {onSettingsClick && (
              <button
                onClick={onSettingsClick}
                className="text-xs text-blue-600 hover:text-blue-700"
              >
                Edit
              </button>
            )}
          </div>

          <div className="space-y-1 text-xs">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Delimiter:</span>
              <span className="font-mono text-foreground">
                {config.delimiter === ',' ? 'Comma (,)' :
                 config.delimiter === '\t' ? 'Tab (\\t)' :
                 config.delimiter === ';' ? 'Semicolon (;)' :
                 config.delimiter === '|' ? 'Pipe (|)' :
                 `'${config.delimiter}'`}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Encoding:</span>
              <span className="font-mono text-foreground">{config.encoding}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Include header:</span>
              <span className={`font-medium ${config.include_header ? 'text-green-700' : 'text-muted-foreground'}`}>
                {config.include_header ? 'Yes' : 'No'}
              </span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5 mb-2">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure CSV export
      </div>
      {onPathClick && (
        <button
          onClick={onPathClick}
          className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
        >
          Set Output Path
        </button>
      )}
    </div>
  );
}
