/**
 * Execution Progress Component
 * Displays real-time execution progress overlay
 */

import { motion, AnimatePresence } from 'framer-motion';
import { Loader2, CheckCircle, XCircle } from 'lucide-react';
import { Progress } from '@/components/ui/progress';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import type { WorkflowExecutionResult } from '@/api/types';

interface ExecutionProgressProps {
  isExecuting: boolean;
  currentStepId: string | null;
  result: WorkflowExecutionResult | null;
  totalSteps: number;
  completedSteps: number;
}

export function ExecutionProgress({
  isExecuting,
  currentStepId,
  result,
  totalSteps,
  completedSteps,
}: ExecutionProgressProps) {
  if (!isExecuting && !result) return null;

  const progress = totalSteps > 0 ? (completedSteps / totalSteps) * 100 : 0;

  return (
    <AnimatePresence>
      {(isExecuting || result) && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -20 }}
          className="fixed bottom-6 right-6 w-96 z-50"
        >
          <Card className="shadow-lg border-2">
            <div className="p-4 space-y-3">
              {/* Header */}
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {isExecuting ? (
                    <Loader2 className="h-5 w-5 animate-spin text-primary" />
                  ) : result?.success ? (
                    <CheckCircle className="h-5 w-5 text-success" />
                  ) : (
                    <XCircle className="h-5 w-5 text-error" />
                  )}
                  <h3 className="text-sm font-semibold">
                    {isExecuting
                      ? 'Executing Workflow'
                      : result?.success
                      ? 'Execution Complete'
                      : 'Execution Failed'}
                  </h3>
                </div>

                {result && (
                  <Badge variant={result.success ? 'success' : 'destructive'}>
                    {(result.confidence * 100).toFixed(0)}%
                  </Badge>
                )}
              </div>

              {/* Progress bar */}
              <div className="space-y-1.5">
                <Progress value={progress} className="h-2" />
                <div className="flex items-center justify-between text-xs text-muted-foreground">
                  <span>
                    Step {completedSteps} of {totalSteps}
                  </span>
                  {currentStepId && (
                    <span className="font-medium">Processing: {currentStepId}</span>
                  )}
                </div>
              </div>

              {/* Execution details */}
              {result && (
                <div className="pt-2 border-t border-border">
                  <div className="grid grid-cols-2 gap-2 text-xs">
                    <div>
                      <span className="text-muted-foreground">Duration:</span>
                      <span className="ml-1 font-medium">{result.duration_ms}ms</span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">Steps:</span>
                      <span className="ml-1 font-medium">
                        {result.step_results.filter(s => s.success).length}/{result.step_results.length} succeeded
                      </span>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </Card>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
