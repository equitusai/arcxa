/**
 * Execution Details Dialog
 * Shows detailed step-by-step results for a workflow execution
 * Addresses UX Issue: Provide drill-down capability for execution debugging
 */

import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  CheckCircle,
  XCircle,
  Clock,
  TrendingUp,
  ChevronDown,
  ChevronRight,
  Loader2,
  AlertCircle,
  Download,
  Play,
  Copy,
  Database,
} from 'lucide-react';
import { useExecutionDetails } from '@/hooks/useWorkflows';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';
import { Link } from 'react-router-dom';

export interface ExecutionDetailsDialogProps {
  /**
   * Whether the dialog is open
   */
  open: boolean;

  /**
   * Callback when dialog open state changes
   */
  onOpenChange: (open: boolean) => void;

  /**
   * Execution ID to show details for
   */
  executionId: string | null;

  /**
   * Optional: Re-run callback
   */
  onRerun?: (executionId: string) => void;
}

/**
 * Format timestamp to readable date/time
 */
function formatDateTime(timestamp: string): string {
  const date = new Date(timestamp);
  return date.toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/**
 * Format duration in milliseconds to human-readable
 */
function formatDuration(durationMs: number): string {
  const seconds = Math.floor(durationMs / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);

  if (hours > 0) {
    return `${hours}h ${minutes % 60}m ${seconds % 60}s`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
  }
  if (seconds > 0) {
    return `${seconds}.${Math.floor((durationMs % 1000) / 100)}s`;
  }
  return `${durationMs}ms`;
}

/**
 * Format confidence score as percentage
 */
function formatConfidence(confidence: number): string {
  return `${(confidence * 100).toFixed(1)}%`;
}

/**
 * Copy text to clipboard
 */
function copyToClipboard(text: string, label: string) {
  navigator.clipboard.writeText(text).then(() => {
    toast.success(`${label} copied to clipboard`);
  }).catch(() => {
    toast.error('Failed to copy to clipboard');
  });
}

export function ExecutionDetailsDialog({
  open,
  onOpenChange,
  executionId,
  onRerun,
}: ExecutionDetailsDialogProps) {
  const [expandedSteps, setExpandedSteps] = useState<Set<string>>(new Set());

  // Fetch execution details
  const { data: execution, isLoading, error } = useExecutionDetails(
    executionId || undefined
  );

  // Toggle step expansion
  const toggleStep = (stepId: string) => {
    setExpandedSteps((prev) => {
      const next = new Set(prev);
      if (next.has(stepId)) {
        next.delete(stepId);
      } else {
        next.add(stepId);
      }
      return next;
    });
  };

  // Export execution data as JSON
  const handleExport = () => {
    if (!execution) return;

    const dataStr = JSON.stringify(execution, null, 2);
    const dataBlob = new Blob([dataStr], { type: 'application/json' });
    const url = URL.createObjectURL(dataBlob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `execution-${execution.execution_id}.json`;
    link.click();
    URL.revokeObjectURL(url);

    toast.success('Execution data exported');
  };

  // Loading state
  if (isLoading) {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <div className="flex flex-col items-center justify-center py-12">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground mb-3" />
            <p className="text-sm text-muted-foreground">Loading execution details...</p>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  // Error state
  if (error || !execution) {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
          <div className="flex flex-col items-center justify-center py-12">
            <AlertCircle className="h-8 w-8 text-red-500 mb-3" />
            <p className="text-sm font-medium text-red-700 mb-1">Failed to load execution details</p>
            <p className="text-xs text-red-600">{(error as Error)?.message || 'Unknown error'}</p>
          </div>
        </DialogContent>
      </Dialog>
    );
  }

  const overallSuccess = execution.success;
  const failedStep = execution.step_results?.find(step => !step.success);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            Execution Details
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2"
              onClick={() => copyToClipboard(execution.execution_id, 'Execution ID')}
            >
              <span className="font-mono text-xs text-muted-foreground">
                {execution.execution_id.slice(0, 12)}...
              </span>
              <Copy className="h-3 w-3 ml-1" />
            </Button>
          </DialogTitle>
          <DialogDescription>
            Step-by-step results for workflow execution
          </DialogDescription>
        </DialogHeader>

        {/* Execution Summary */}
        <div
          className={cn(
            'p-4 rounded-lg border-2',
            overallSuccess
              ? 'bg-green-50 border-green-200'
              : 'bg-red-50 border-red-200'
          )}
        >
          <div className="flex items-start justify-between mb-3">
            <div className="flex items-center gap-2">
              {overallSuccess ? (
                <CheckCircle className="h-6 w-6 text-green-600" />
              ) : (
                <XCircle className="h-6 w-6 text-red-600" />
              )}
              <div>
                <h3 className="text-lg font-semibold">
                  {overallSuccess ? 'Execution Successful' : 'Execution Failed'}
                </h3>
                {failedStep && (
                  <p className="text-sm text-red-700">
                    Failed at step: <span className="font-mono">{failedStep.step_id}</span>
                  </p>
                )}
              </div>
            </div>
            <Badge
              variant={overallSuccess ? 'default' : 'destructive'}
              className="text-sm"
            >
              {overallSuccess ? 'Success' : 'Failed'}
            </Badge>
          </div>

          <div className="grid grid-cols-3 gap-4 text-sm">
            <div>
              <div className="flex items-center gap-1 text-muted-foreground mb-1">
                <Clock className="h-3 w-3" />
                <span className="text-xs">Started</span>
              </div>
              <p className="font-medium">{formatDateTime(execution.started_at)}</p>
            </div>

            <div>
              <div className="flex items-center gap-1 text-muted-foreground mb-1">
                <Clock className="h-3 w-3" />
                <span className="text-xs">Duration</span>
              </div>
              <p className="font-medium">{formatDuration(execution.duration_ms)}</p>
            </div>

            <div>
              <div className="flex items-center gap-1 text-muted-foreground mb-1">
                <TrendingUp className="h-3 w-3" />
                <span className="text-xs">Confidence</span>
              </div>
              <p className="font-medium">{formatConfidence(execution.confidence)}</p>
            </div>
          </div>
        </div>

        {(execution.materialized_dataset || execution.final_output !== undefined) && (
          <div className="grid gap-4 md:grid-cols-2 mt-4">
            {execution.materialized_dataset && (
              <div className="rounded-lg border border-border bg-background p-4">
                <div className="flex items-center gap-2 mb-2">
                  <Database className="h-4 w-4 text-muted-foreground" />
                  <h4 className="text-sm font-semibold text-foreground">Materialized Dataset</h4>
                </div>
                <p className="text-sm font-medium text-foreground mb-1">
                  {execution.materialized_dataset.name}
                </p>
                <p className="text-xs text-muted-foreground mb-3">
                  {execution.materialized_dataset.record_count.toLocaleString()} rows
                </p>
                <Button asChild variant="outline" size="sm">
                  <Link to={`/catalogue/${execution.materialized_dataset.dataset_id}`}>
                    Open Dataset
                  </Link>
                </Button>
              </div>
            )}

            {execution.final_output !== undefined && (
              <div className="rounded-lg border border-border bg-background p-4">
                <h4 className="text-sm font-semibold text-foreground mb-2">Final Output</h4>
                <pre className="text-xs bg-muted p-3 rounded border overflow-auto max-h-40">
                  {JSON.stringify(execution.final_output, null, 2)}
                </pre>
              </div>
            )}
          </div>
        )}

        {/* Step Results */}
        <div className="space-y-2 mt-4">
          <h4 className="text-sm font-semibold text-foreground mb-2">
            Step-by-Step Results ({execution.step_results?.length || 0} steps)
          </h4>

          {execution.step_results?.map((step, index) => {
            const isExpanded = expandedSteps.has(step.step_id);
            const stepSuccess = step.success;

            return (
              <div
                key={step.step_id}
                className={cn(
                  'border rounded-lg overflow-hidden transition-all',
                  stepSuccess ? 'border-green-200 bg-green-50/30' : 'border-red-200 bg-red-50/30'
                )}
              >
                {/* Step Header */}
                <button
                  className="w-full p-3 flex items-center gap-3 hover:bg-accent/50 transition-colors"
                  onClick={() => toggleStep(step.step_id)}
                >
                  {/* Expand/Collapse Icon */}
                  {isExpanded ? (
                    <ChevronDown className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                  )}

                  {/* Status Icon */}
                  <div
                    className={cn(
                      'flex items-center justify-center w-8 h-8 rounded-full border-2 flex-shrink-0',
                      stepSuccess
                        ? 'bg-green-50 border-green-200 text-green-700'
                        : 'bg-red-50 border-red-200 text-red-700'
                    )}
                  >
                    {stepSuccess ? (
                      <CheckCircle className="h-4 w-4" />
                    ) : (
                      <XCircle className="h-4 w-4" />
                    )}
                  </div>

                  {/* Step Info */}
                  <div className="flex-1 text-left min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-sm font-medium text-foreground">
                        {index + 1}. {step.step_id}
                      </span>
                      <Badge
                        variant={stepSuccess ? 'default' : 'destructive'}
                        className="text-xs"
                      >
                        {stepSuccess ? 'Success' : 'Failed'}
                      </Badge>
                    </div>
                    <div className="flex items-center gap-3 text-xs text-muted-foreground">
                      <span>{formatDuration(step.duration_ms)}</span>
                      {step.confidence !== undefined && (
                        <>
                          <span>•</span>
                          <span>{formatConfidence(step.confidence)}</span>
                        </>
                      )}
                    </div>
                  </div>
                </button>

                {/* Step Details (Expanded) */}
                {isExpanded && (
                  <div className="border-t bg-background p-4 space-y-3">
                    {/* Error Message */}
                    {step.error && (
                      <div className="p-3 bg-red-50 border border-red-200 rounded">
                        <div className="flex items-center gap-2 mb-2">
                          <AlertCircle className="h-4 w-4 text-red-600" />
                          <span className="text-sm font-semibold text-red-900">Error</span>
                        </div>
                        <p className="text-sm text-red-800 font-mono whitespace-pre-wrap">
                          {step.error}
                        </p>
                      </div>
                    )}

                    {/* Output Data */}
                    {step.output && (
                      <div>
                        <div className="flex items-center justify-between mb-2">
                          <span className="text-xs font-semibold text-foreground">Output Data</span>
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={() =>
                              copyToClipboard(
                                JSON.stringify(step.output, null, 2),
                                'Step output'
                              )
                            }
                          >
                            <Copy className="h-3 w-3 mr-1" />
                            Copy
                          </Button>
                        </div>
                        <pre className="text-xs bg-muted p-3 rounded border overflow-auto max-h-64 font-mono">
                          {JSON.stringify(step.output, null, 2)}
                        </pre>
                      </div>
                    )}

                    {/* No output or error */}
                    {!step.output && !step.error && (
                      <p className="text-xs text-muted-foreground italic">
                        No output data available for this step
                      </p>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center justify-between pt-4 border-t mt-4">
          <div className="flex gap-2">
            {onRerun && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  onRerun(execution.execution_id);
                  onOpenChange(false);
                }}
              >
                <Play className="h-4 w-4 mr-2" />
                Re-run
              </Button>
            )}
            <Button variant="outline" size="sm" onClick={handleExport}>
              <Download className="h-4 w-4 mr-2" />
              Export JSON
            </Button>
          </div>
          <Button variant="default" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
