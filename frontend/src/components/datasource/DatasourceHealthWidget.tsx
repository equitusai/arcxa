/**
 * Datasource Health Widget
 * Overall datasource ecosystem health monitoring
 */

import { Database, Activity, AlertCircle, CheckCircle2, XCircle, RefreshCw } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { Datasource } from '@/api/types';

export interface DatasourceHealthWidgetProps {
  datasources: Datasource[];
  onTestAll?: () => void;
  onRefreshAll?: () => void;
  className?: string;
}

export function DatasourceHealthWidget({
  datasources,
  onTestAll,
  onRefreshAll,
  className,
}: DatasourceHealthWidgetProps) {
  // Calculate health metrics
  const total = datasources.length;
  const connected = datasources.filter((ds) => ds.status === 'Connected').length;
  const errors = datasources.filter((ds) =>
    typeof ds.status === 'object' && 'Error' in ds.status
  ).length;
  const degraded = datasources.filter((ds) =>
    typeof ds.status === 'object' && 'Degraded' in ds.status
  ).length;
  const disabled = datasources.filter((ds) => !ds.enabled).length;

  // Overall health percentage (connected / enabled)
  const enabled = total - disabled;
  const overallHealth = enabled > 0 ? Math.round((connected / enabled) * 100) : 0;

  const healthColor =
    overallHealth >= 90
      ? 'text-green-600'
      : overallHealth >= 70
      ? 'text-amber-600'
      : 'text-red-600';

  const progressColor =
    overallHealth >= 90
      ? 'bg-green-500'
      : overallHealth >= 70
      ? 'bg-amber-500'
      : 'bg-red-500';

  // Per-datasource health bars
  const topDatasources = datasources
    .filter((ds) => ds.enabled)
    .slice(0, 5)
    .map((ds) => ({
      id: ds.id, // ✅ Added ID for unique keys
      name: ds.name,
      status: ds.status,
      health: ds.status === 'Connected' ? 100 :
              typeof ds.status === 'object' && 'Degraded' in ds.status ? 60 :
              typeof ds.status === 'object' && 'Error' in ds.status ? 20 : 0,
    }));

  return (
    <Card className={cn('p-6', className)}>
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Activity className="w-4 h-4 text-primary" />
          <h3 className="text-sm font-semibold">Connection Health</h3>
        </div>

        <div className="flex items-baseline gap-1">
          <span className={cn('text-2xl font-bold', healthColor)}>{overallHealth}</span>
          <span className="text-xs text-muted-foreground">/100</span>
        </div>
      </div>

      {/* Overall Health Progress */}
      <div className="mb-6">
        <Progress value={overallHealth} className="h-2">
          <div
            className={cn('h-full transition-all duration-500', progressColor)}
            style={{ width: `${overallHealth}%` }}
          />
        </Progress>
        <p className="text-xs text-muted-foreground mt-1">
          {overallHealth >= 90 && 'All datasources healthy'}
          {overallHealth >= 70 && overallHealth < 90 && 'Some datasources need attention'}
          {overallHealth < 70 && 'Critical: Multiple connection failures'}
        </p>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-2 gap-3 mb-6">
        <div className="flex items-center gap-2 p-3 rounded-lg bg-green-50 border border-green-200">
          <CheckCircle2 className="w-4 h-4 text-green-600" />
          <div>
            <p className="text-xs text-muted-foreground">Connected</p>
            <p className="text-lg font-bold text-green-700">{connected}</p>
          </div>
        </div>

        <div className="flex items-center gap-2 p-3 rounded-lg bg-red-50 border border-red-200">
          <XCircle className="w-4 h-4 text-red-600" />
          <div>
            <p className="text-xs text-muted-foreground">Errors</p>
            <p className="text-lg font-bold text-red-700">{errors}</p>
          </div>
        </div>

        <div className="flex items-center gap-2 p-3 rounded-lg bg-amber-50 border border-amber-200">
          <AlertCircle className="w-4 h-4 text-amber-600" />
          <div>
            <p className="text-xs text-muted-foreground">Degraded</p>
            <p className="text-lg font-bold text-amber-700">{degraded}</p>
          </div>
        </div>

        <div className="flex items-center gap-2 p-3 rounded-lg bg-gray-50 border border-gray-200">
          <Database className="w-4 h-4 text-gray-600" />
          <div>
            <p className="text-xs text-muted-foreground">Disabled</p>
            <p className="text-lg font-bold text-gray-700">{disabled}</p>
          </div>
        </div>
      </div>

      {/* Top Data Sources Health Bars */}
      <div className="space-y-3 mb-4">
        <h4 className="text-xs font-medium text-muted-foreground">Top Data Sources</h4>
        {topDatasources.map((ds) => (
          <div key={ds.id} className="space-y-1"> {/* ✅ Fixed: Use ds.id instead of ds.name */}
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium truncate flex-1">{ds.name}</span>
              <Badge
                variant="outline"
                className={cn(
                  'text-[10px] px-1.5 py-0',
                  ds.health === 100 && 'bg-green-50 text-green-700 border-green-300',
                  ds.health === 60 && 'bg-amber-50 text-amber-700 border-amber-300',
                  ds.health === 20 && 'bg-red-50 text-red-700 border-red-300',
                  ds.health === 0 && 'bg-gray-50 text-gray-700 border-gray-300'
                )}
              >
                {ds.health}%
              </Badge>
            </div>
            <Progress value={ds.health} className="h-1.5">
              <div
                className={cn(
                  'h-full transition-all duration-300',
                  ds.health === 100 && 'bg-green-500',
                  ds.health === 60 && 'bg-amber-500',
                  ds.health === 20 && 'bg-red-500',
                  ds.health === 0 && 'bg-gray-400'
                )}
                style={{ width: `${ds.health}%` }}
              />
            </Progress>
          </div>
        ))}
      </div>

      {/* Action Buttons */}
      <div className="flex gap-2">
        {onTestAll && (
          <Button
            variant="outline"
            size="sm"
            onClick={onTestAll}
            className="flex-1 text-xs gap-1"
          >
            <Activity className="w-3 h-3" />
            Test All
          </Button>
        )}
        {onRefreshAll && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onRefreshAll}
            className="flex-1 text-xs gap-1"
          >
            <RefreshCw className="w-3 h-3" />
            Refresh
          </Button>
        )}
      </div>
    </Card>
  );
}
