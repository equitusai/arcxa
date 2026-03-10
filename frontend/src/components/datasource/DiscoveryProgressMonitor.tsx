/**
 * Discovery Progress Monitor
 * Tracks datasource-backed schema discovery progress from the coordinator.
 */

import React, { useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Database,
  Table as TableIcon,
  CheckCircle2,
  XCircle,
  Loader2,
  Clock,
  AlertTriangle,
  X,
  Activity,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { DiscoveryProgress, DiscoveryResult } from '@/types/discovery';
import { useDiscoveryStream, useCancelDiscovery } from '@/hooks/useSchemaDiscovery';

interface DiscoveryProgressMonitorProps {
  datasource_id: string;
  discovery_id: string;
  onComplete?: (result: DiscoveryResult) => void;
  onError?: (error: string) => void;
  onCancel?: () => void;
}

export function DiscoveryProgressMonitor({
  datasource_id,
  discovery_id,
  onComplete,
  onError,
  onCancel,
}: DiscoveryProgressMonitorProps) {
  const [logs, setLogs] = React.useState<
    Array<{ timestamp: string; message: string; type: 'info' | 'success' | 'error' }>
  >([]);

  const cancelMutation = useCancelDiscovery();

  const addLog = (message: string, type: 'info' | 'success' | 'error' = 'info') => {
    const timestamp = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, { timestamp, message, type }]);
  };

  const { progress, isConnected, error: streamError } = useDiscoveryStream(
    datasource_id,
    discovery_id,
    {
      onProgress: (nextProgress) => {
        addLog(
          `${nextProgress.current_step} (${Math.round(nextProgress.percent_complete)}%)`,
          'info'
        );
      },
      onComplete: (result) => {
        addLog('Discovery completed successfully.', 'success');
        onComplete?.(result);
      },
      onError: (message) => {
        addLog(message, 'error');
        onError?.(message);
      },
    }
  );

  const handleCancel = async () => {
    if (!confirm('Are you sure you want to cancel this discovery?')) {
      return;
    }

    await cancelMutation.mutateAsync({ datasource_id, discovery_id });
    onCancel?.();
  };

  useEffect(() => {
    if (streamError) {
      addLog(streamError, 'error');
    }
  }, [streamError]);

  const isCompleted = progress?.status === 'completed';
  const isFailed = progress?.status === 'failed';
  const isCancelled = progress?.status === 'cancelled';
  const isRunning = progress?.status === 'queued' || progress?.status === 'running';

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Database className="h-5 w-5" />
                Schema Discovery in Progress
              </CardTitle>
              <CardDescription>
                Discovery ID: {discovery_id}
                {isConnected && (
                  <Badge variant="outline" className="ml-2 text-green-600 border-green-600">
                    Live
                  </Badge>
                )}
              </CardDescription>
            </div>
            {isRunning && (
              <Button
                variant="outline"
                size="sm"
                onClick={handleCancel}
                disabled={cancelMutation.isPending}
              >
                {cancelMutation.isPending ? (
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                ) : (
                  <X className="h-4 w-4 mr-2" />
                )}
                Cancel
              </Button>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium">Overall Progress</span>
              <span className="text-muted-foreground">
                {Math.round(progress?.percent_complete || 0)}%
              </span>
            </div>
            <Progress value={progress?.percent_complete || 0} className="h-2" />
          </div>

          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
            <StatCard
              icon={TableIcon}
              label="Tables Discovered"
              value={progress?.tables_discovered || 0}
              helper={
                progress?.total_tables ? `of ${progress.total_tables.toLocaleString()}` : undefined
              }
              color="text-blue-600"
            />
            <StatCard
              icon={Activity}
              label="Status"
              value={formatStatus(progress?.status)}
              color="text-purple-600"
            />
            <StatCard
              icon={Clock}
              label="Updated"
              value={progress ? new Date(progress.updated_at).toLocaleTimeString() : '--'}
              color="text-green-600"
            />
          </div>

          <div className="rounded-lg border p-4">
            <div className="text-xs uppercase tracking-wide text-muted-foreground mb-2">
              Current Step
            </div>
            <div className="font-medium">{progress?.current_step || 'Waiting for progress...'}</div>
          </div>

          {isCompleted && (
            <Alert className="border-green-200 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-800">
                Discovery completed successfully.
              </AlertDescription>
            </Alert>
          )}

          {isFailed && (
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertDescription>
                Discovery failed. Review the activity log for details.
              </AlertDescription>
            </Alert>
          )}

          {isCancelled && (
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>Discovery was cancelled.</AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <Clock className="h-4 w-4" />
            Activity Log
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ScrollArea className="h-64 border rounded-lg p-3">
            <AnimatePresence>
              {logs.length === 0 ? (
                <div className="text-center text-sm text-muted-foreground py-8">
                  Waiting for discovery events...
                </div>
              ) : (
                <div className="space-y-2">
                  {logs.map((log, index) => (
                    <motion.div
                      key={`${log.timestamp}-${index}`}
                      initial={{ opacity: 0, y: -10 }}
                      animate={{ opacity: 1, y: 0 }}
                      className="flex items-start gap-2 text-xs font-mono"
                    >
                      <span className="text-muted-foreground">{log.timestamp}</span>
                      <span
                        className={cn(
                          log.type === 'error' && 'text-destructive',
                          log.type === 'success' && 'text-green-600',
                          log.type === 'info' && 'text-foreground'
                        )}
                      >
                        {log.message}
                      </span>
                    </motion.div>
                  ))}
                </div>
              )}
            </AnimatePresence>
          </ScrollArea>
        </CardContent>
      </Card>

      {progress?.errors && progress.errors.length > 0 && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            <div className="font-semibold mb-1">Errors encountered:</div>
            <ul className="list-disc list-inside text-sm space-y-1">
              {progress.errors.map((entry, index) => (
                <li key={`${entry}-${index}`}>{entry}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}

interface StatCardProps {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: string | number;
  helper?: string;
  color: string;
}

function StatCard({ icon: Icon, label, value, helper, color }: StatCardProps) {
  return (
    <div className="border rounded-lg p-3 space-y-1">
      <div className="flex items-center gap-2">
        <Icon className={cn('h-4 w-4', color)} />
        <span className="text-xs text-muted-foreground">{label}</span>
      </div>
      <div className="text-2xl font-bold">{value}</div>
      {helper && <div className="text-xs text-muted-foreground">{helper}</div>}
    </div>
  );
}

function formatStatus(status: DiscoveryProgress['status'] | undefined): string {
  switch (status) {
    case 'queued':
      return 'Queued';
    case 'running':
      return 'Running';
    case 'completed':
      return 'Completed';
    case 'failed':
      return 'Failed';
    case 'cancelled':
      return 'Cancelled';
    default:
      return 'Pending';
  }
}
