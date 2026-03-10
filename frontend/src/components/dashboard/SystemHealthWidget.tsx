/**
 * System Health Widget
 * Component-level health monitoring
 */

import { Activity, AlertCircle, CheckCircle2, XCircle } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { cn } from '@/lib/utils';

export interface ComponentHealth {
  name: string;
  status: 'healthy' | 'degraded' | 'down';
  message?: string;
}

export interface SystemHealthWidgetProps {
  overallHealth: number; // 0-100
  components: ComponentHealth[];
  onDiagnose?: () => void;
  onRefresh?: () => void;
  className?: string;
}

const statusConfig = {
  healthy: {
    icon: CheckCircle2,
    color: 'text-green-500',
    bgColor: 'bg-green-500/10',
    borderColor: 'border-green-500/30',
  },
  degraded: {
    icon: AlertCircle,
    color: 'text-amber-500',
    bgColor: 'bg-amber-500/10',
    borderColor: 'border-amber-500/30',
  },
  down: {
    icon: XCircle,
    color: 'text-red-500',
    bgColor: 'bg-red-500/10',
    borderColor: 'border-red-500/30',
  },
};

export function SystemHealthWidget({
  overallHealth,
  components,
  onDiagnose,
  onRefresh,
  className,
}: SystemHealthWidgetProps) {
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

  return (
    <Card className={cn('p-6', className)}>
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Activity className="w-4 h-4 text-primary" />
          <h3 className="text-sm font-semibold">System Health</h3>
        </div>

        <div className="flex items-baseline gap-1">
          <span className={cn('text-2xl font-bold', healthColor)}>{overallHealth}</span>
          <span className="text-xs text-muted-foreground">/100</span>
        </div>
      </div>

      {/* Health Progress Bar */}
      <div className="mb-6">
        <Progress value={overallHealth} className="h-2">
          <div
            className={cn('h-full transition-all duration-500', progressColor)}
            style={{ width: `${overallHealth}%` }}
          />
        </Progress>
        <p className="text-xs text-muted-foreground mt-1">
          {overallHealth >= 90 && 'All systems operational'}
          {overallHealth >= 70 && overallHealth < 90 && 'Some systems degraded'}
          {overallHealth < 70 && 'Critical issues detected'}
        </p>
      </div>

      {/* Component Status List */}
      <div className="space-y-2 mb-4">
        {components.map((component) => {
          const config = statusConfig[component.status];
          const StatusIcon = config.icon;

          return (
            <div
              key={component.name}
              className={cn(
                'flex items-center justify-between p-2 rounded-md border',
                config.bgColor,
                config.borderColor
              )}
            >
              <div className="flex items-center gap-2">
                <StatusIcon className={cn('w-3 h-3', config.color)} />
                <span className="text-xs font-medium">{component.name}</span>
              </div>
              {component.message && (
                <span className="text-xs text-muted-foreground">{component.message}</span>
              )}
            </div>
          );
        })}
      </div>

      {/* Actions */}
      <div className="flex gap-2">
        {onDiagnose && (
          <Button variant="outline" size="sm" onClick={onDiagnose} className="flex-1 text-xs">
            Run Diagnostics
          </Button>
        )}
        {onRefresh && (
          <Button variant="ghost" size="sm" onClick={onRefresh} className="flex-1 text-xs">
            Refresh
          </Button>
        )}
      </div>
    </Card>
  );
}
