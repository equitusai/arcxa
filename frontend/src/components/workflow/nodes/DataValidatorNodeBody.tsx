/**
 * Data Validator Node Body
 * Enforce NOT NULL, REGEX, RANGE validation rules
 */

import React from 'react';
import { ShieldCheck, AlertCircle, AlertTriangle, ChevronRight } from 'lucide-react';
import type { DataValidatorConfig } from '@/lib/workflow-etl-config';

export interface DataValidatorNodeBodyProps {
  config?: DataValidatorConfig;
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
  validationResults?: {
    valid_count: number;
    invalid_count: number;
    warning_count: number;
  };
  onAddRule?: () => void;
  onEditRule?: (index: number) => void;
}

function formatRuleType(ruleType: unknown): string {
  if (typeof ruleType === 'string') {
    return ruleType.replace(/_/g, ' ');
  }

  if (!ruleType || typeof ruleType !== 'object') {
    return 'CUSTOM';
  }

  const entries = Object.entries(ruleType as Record<string, unknown>);
  if (entries.length === 1) {
    const [type, params] = entries[0];
    if (params && typeof params === 'object' && 'pattern' in (params as Record<string, unknown>)) {
      return `${type} (${String((params as Record<string, unknown>).pattern)})`;
    }

    return type.replace(/_/g, ' ');
  }

  return 'CUSTOM';
}

export function DataValidatorNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  validationResults,
  onAddRule,
  onEditRule,
}: DataValidatorNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Validating data...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-red-500 to-red-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} rows validated
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && config) {
    const ruleCount = config.rules?.length || 0;
    const errorRules = config.rules?.filter(r => r.severity === 'error').length || 0;
    const warningRules = config.rules?.filter(r => r.severity === 'warning').length || 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Rule summary */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-xs text-foreground">
            <ShieldCheck className="w-3 h-3" />
            <span className="font-medium">
              {ruleCount} Rule{ruleCount !== 1 ? 's' : ''}
            </span>
          </div>
          {onAddRule && (
            <button
              onClick={onAddRule}
              className="text-xs text-blue-600 hover:text-blue-700 font-medium"
            >
              + Add
            </button>
          )}
        </div>

        {/* Rule breakdown */}
        {ruleCount > 0 && (
          <div className="flex gap-2">
            {errorRules > 0 && (
              <div className="flex items-center gap-1 text-xs text-red-700 bg-red-50 px-2 py-1 rounded">
                <AlertCircle className="w-3 h-3" />
                <span>{errorRules} Error</span>
              </div>
            )}
            {warningRules > 0 && (
              <div className="flex items-center gap-1 text-xs text-amber-700 bg-amber-50 px-2 py-1 rounded">
                <AlertTriangle className="w-3 h-3" />
                <span>{warningRules} Warning</span>
              </div>
            )}
          </div>
        )}

        {/* Validation results */}
        {validationResults && (
          <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-1">
            <div className="font-medium text-foreground mb-1">Validation Results</div>
            <div className="flex items-center justify-between">
              <span className="text-green-700 dark:text-green-500">Valid rows:</span>
              <span className="font-medium">{validationResults.valid_count.toLocaleString()}</span>
            </div>
            {validationResults.invalid_count > 0 && (
              <div className="flex items-center justify-between">
                <span className="text-red-700">Invalid rows:</span>
                <span className="font-medium">{validationResults.invalid_count.toLocaleString()}</span>
              </div>
            )}
            {validationResults.warning_count > 0 && (
              <div className="flex items-center justify-between">
                <span className="text-amber-700">Warnings:</span>
                <span className="font-medium">{validationResults.warning_count.toLocaleString()}</span>
              </div>
            )}
          </div>
        )}

        {/* Rule list */}
        {config.rules && config.rules.length > 0 && (
          <div className="space-y-1.5">
            {config.rules.slice(0, 3).map((rule, idx) => (
              <button
                key={idx}
                onClick={() => onEditRule?.(idx)}
                className="w-full p-2 bg-muted hover:bg-muted border border-neutral-200 rounded text-left transition-colors"
              >
                <div className="flex items-center justify-between">
                  <div className="text-xs">
                    <div className="flex items-center gap-1.5 mb-0.5">
                      {rule.severity === 'error' ? (
                        <AlertCircle className="w-3 h-3 text-red-500" />
                      ) : (
                        <AlertTriangle className="w-3 h-3 text-amber-500" />
                      )}
                      <span className="font-medium text-foreground">{rule.field}</span>
                    </div>
                    <div className="text-muted-foreground pl-4.5">
                      {formatRuleType(rule.rule_type)}
                    </div>
                  </div>
                  <ChevronRight className="w-3 h-3 text-neutral-400" />
                </div>
              </button>
            ))}
            {config.rules.length > 3 && (
              <div className="text-xs text-muted-foreground text-center py-1">
                + {config.rules.length - 3} more...
              </div>
            )}
          </div>
        )}

        {/* Fail on error setting */}
        <div className="flex items-center justify-between text-xs pt-2 border-t border-border">
          <span className="text-muted-foreground">On validation error:</span>
          <span className={`font-medium ${config.fail_on_error ? 'text-red-700' : 'text-amber-700'}`}>
            {config.fail_on_error ? 'Stop processing' : 'Continue with warnings'}
          </span>
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
                {metrics.rowsProcessed.toLocaleString()} rows validated
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
    const ruleCount = config.rules?.length || 0;
    const errorRules = config.rules?.filter(r => r.severity === 'error').length || 0;
    const warningRules = config.rules?.filter(r => r.severity === 'warning').length || 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Rule summary */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-xs text-foreground">
            <ShieldCheck className="w-3 h-3" />
            <span className="font-medium">
              {ruleCount} Rule{ruleCount !== 1 ? 's' : ''}
            </span>
          </div>
          {onAddRule && (
            <button
              onClick={onAddRule}
              className="text-xs text-blue-600 hover:text-blue-700 font-medium"
            >
              + Add
            </button>
          )}
        </div>

        {/* Rule breakdown */}
        {ruleCount > 0 && (
          <div className="flex gap-2">
            {errorRules > 0 && (
              <div className="flex items-center gap-1 text-xs text-red-700 bg-red-50 px-2 py-1 rounded">
                <AlertCircle className="w-3 h-3" />
                <span>{errorRules} Error</span>
              </div>
            )}
            {warningRules > 0 && (
              <div className="flex items-center gap-1 text-xs text-amber-700 bg-amber-50 px-2 py-1 rounded">
                <AlertTriangle className="w-3 h-3" />
                <span>{warningRules} Warning</span>
              </div>
            )}
          </div>
        )}

        {/* Rule list */}
        {config.rules && config.rules.length > 0 ? (
          <div className="space-y-1.5">
            {config.rules.slice(0, 3).map((rule, idx) => (
              <button
                key={idx}
                onClick={() => onEditRule?.(idx)}
                className="w-full p-2 bg-muted hover:bg-muted border border-neutral-200 rounded text-left transition-colors"
              >
                <div className="flex items-center justify-between">
                  <div className="text-xs">
                    <div className="flex items-center gap-1.5 mb-0.5">
                      {rule.severity === 'error' ? (
                        <AlertCircle className="w-3 h-3 text-red-500" />
                      ) : (
                        <AlertTriangle className="w-3 h-3 text-amber-500" />
                      )}
                      <span className="font-medium text-foreground">{rule.field}</span>
                    </div>
                    <div className="text-muted-foreground pl-4.5">
                      {formatRuleType(rule.rule_type)}
                    </div>
                  </div>
                  <ChevronRight className="w-3 h-3 text-neutral-400" />
                </div>
              </button>
            ))}
            {config.rules.length > 3 && (
              <div className="text-xs text-muted-foreground text-center py-1">
                + {config.rules.length - 3} more...
              </div>
            )}
          </div>
        ) : (
          <div className="text-xs text-muted-foreground text-center py-2">
            No validation rules configured
          </div>
        )}

        {/* Fail on error setting */}
        <div className="flex items-center justify-between text-xs pt-2 border-t border-border">
          <span className="text-muted-foreground">On validation error:</span>
          <span className={`font-medium ${config.fail_on_error ? 'text-red-700' : 'text-amber-700'}`}>
            {config.fail_on_error ? 'Stop processing' : 'Continue with warnings'}
          </span>
        </div>
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5 mb-2">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to add validation rules
      </div>
      {onAddRule && (
        <button
          onClick={onAddRule}
          className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
        >
          + Add First Rule
        </button>
      )}
    </div>
  );
}
