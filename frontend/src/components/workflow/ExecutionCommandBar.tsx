/**
 * Execution Command Bar
 * Primary workflow execution controls with Play/Stop/Pause buttons
 * Addresses UX Issue C-1: Make execution controls prominent and accessible
 */

import React from 'react';
import { Play, Square, Pause, ChevronRight, Loader2, CheckCircle, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface ExecutionCommandBarProps {
  workflowId?: string;
  workflowName?: string;
  executionStatus?: 'idle' | 'running' | 'paused' | 'success' | 'error';
  progress?: {
    currentStep: number;
    totalSteps: number;
    stepName?: string;
    percentage?: number;
  };
  canExecute: boolean;
  canStop: boolean;
  canPause: boolean;
  canResume: boolean;
  onExecute: () => void;
  onStop: () => void;
  onPause: () => void;
  onResume: () => void;
  validationErrors?: string[];
}

export function ExecutionCommandBar({
  workflowId,
  workflowName,
  executionStatus = 'idle',
  progress,
  canExecute,
  canStop,
  canPause,
  canResume,
  onExecute,
  onStop,
  onPause,
  onResume,
  validationErrors = [],
}: ExecutionCommandBarProps) {
  const hasErrors = validationErrors.length > 0;

  // Determine primary action based on state
  const getPrimaryAction = () => {
    if (executionStatus === 'paused' && canResume) {
      return {
        label: 'Resume',
        icon: Play,
        onClick: onResume,
        disabled: false,
        variant: 'default' as const,
        shortcut: '⌘↵',
      };
    }

    if (executionStatus === 'running') {
      return {
        label: 'Running',
        icon: Loader2,
        onClick: () => {},
        disabled: true,
        variant: 'default' as const,
        shortcut: null,
      };
    }

    return {
      label: 'Execute Workflow',
      icon: Play,
      onClick: onExecute,
      disabled: !canExecute,
      variant: 'default' as const,
      shortcut: '⌘↵',
    };
  };

  const primaryAction = getPrimaryAction();
  const PrimaryIcon = primaryAction.icon;

  return (
    <div
      className={cn(
        'h-[48px] border-b bg-gradient-to-r from-background to-muted/20',
        'flex items-center justify-between px-4 gap-4',
        'sticky top-0 z-10 backdrop-blur-sm'
      )}
      style={{
        boxShadow: '0 1px 3px rgba(0,0,0,0.06), 0 1px 2px rgba(0,0,0,0.04)',
      }}
    >
      {/* Left: Workflow Info */}
      <div className="flex items-center gap-2 min-w-0">
        <ChevronRight className="w-4 h-4 text-muted-foreground flex-shrink-0" />
        <span className="text-sm font-medium text-foreground truncate">
          {workflowName || 'Untitled Workflow'}
        </span>
        {workflowId && (
          <span className="text-xs text-muted-foreground font-mono">
            {workflowId.slice(0, 8)}
          </span>
        )}
      </div>

      {/* Center: Execution Progress (when running) */}
      {executionStatus === 'running' && progress && (
        <div className="flex-1 max-w-md">
          <div className="space-y-1">
            <div className="flex items-center justify-between text-xs">
              <span className="text-muted-foreground truncate">
                {progress.stepName || `Step ${progress.currentStep} of ${progress.totalSteps}`}
              </span>
              <span className="font-semibold text-foreground ml-2">
                {progress.percentage !== undefined
                  ? `${progress.percentage}%`
                  : `${progress.currentStep}/${progress.totalSteps}`}
              </span>
            </div>
            <div className="h-1.5 bg-muted rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-blue-500 to-blue-400 transition-all duration-300"
                style={{
                  width: `${progress.percentage || (progress.currentStep / progress.totalSteps) * 100}%`,
                }}
              />
            </div>
          </div>
        </div>
      )}

      {/* Center: Status Message (when paused/success/error) */}
      {executionStatus === 'paused' && (
        <div className="flex items-center gap-2 text-sm text-amber-700 bg-amber-50 px-3 py-1.5 rounded">
          <Pause className="w-4 h-4" />
          <span className="font-medium">Workflow Paused</span>
          {progress && (
            <span className="text-xs text-amber-600">
              at step {progress.currentStep} of {progress.totalSteps}
            </span>
          )}
        </div>
      )}

      {executionStatus === 'success' && (
        <div className="flex items-center gap-2 text-sm text-green-700 bg-green-50 px-3 py-1.5 rounded">
          <CheckCircle className="w-4 h-4" />
          <span className="font-medium">Execution Complete</span>
        </div>
      )}

      {executionStatus === 'error' && (
        <div className="flex items-center gap-2 text-sm text-red-700 bg-red-50 px-3 py-1.5 rounded">
          <XCircle className="w-4 h-4" />
          <span className="font-medium">Execution Failed</span>
        </div>
      )}

      {/* Right: Action Buttons */}
      <div className="flex items-center gap-2">
        {/* Validation Errors Indicator */}
        {hasErrors && executionStatus === 'idle' && (
          <div className="flex items-center gap-1.5 text-xs text-amber-700 bg-amber-50 px-2 py-1 rounded">
            <XCircle className="w-3.5 h-3.5" />
            <span>{validationErrors.length} validation error{validationErrors.length !== 1 ? 's' : ''}</span>
          </div>
        )}

        {/* Pause Button (when running) */}
        {executionStatus === 'running' && canPause && (
          <Button
            size="sm"
            variant="outline"
            onClick={onPause}
            className="gap-1.5"
          >
            <Pause className="w-4 h-4" />
            Pause
          </Button>
        )}

        {/* Stop Button (when running or paused) */}
        {(executionStatus === 'running' || executionStatus === 'paused') && canStop && (
          <Button
            size="sm"
            variant="destructive"
            onClick={onStop}
            className="gap-1.5"
          >
            <Square className="w-4 h-4" />
            Stop
          </Button>
        )}

        {/* Primary Action Button (Execute/Resume) */}
        <Button
          size="sm"
          variant={primaryAction.variant}
          onClick={primaryAction.onClick}
          disabled={primaryAction.disabled}
          className={cn(
            'gap-1.5 min-w-[140px]',
            !primaryAction.disabled && 'shadow-md hover:shadow-lg transition-shadow'
          )}
        >
          <PrimaryIcon
            className={cn('w-4 h-4', executionStatus === 'running' && 'animate-spin')}
          />
          <span>{primaryAction.label}</span>
          {primaryAction.shortcut && (
            <kbd className="ml-1 hidden sm:inline-flex h-5 items-center gap-1 rounded border border-border bg-muted px-1.5 font-mono text-[10px] font-medium opacity-70">
              {primaryAction.shortcut}
            </kbd>
          )}
        </Button>
      </div>
    </div>
  );
}
