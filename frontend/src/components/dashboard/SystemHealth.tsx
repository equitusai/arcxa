import React from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Activity, Server, Database, Zap } from 'lucide-react';
import { motion } from 'framer-motion';

interface HealthMetric {
  label: string;
  value: number;
  unit?: string;
  status: 'healthy' | 'warning' | 'critical';
}

interface SystemHealthProps {
  metrics: HealthMetric[];
  overallStatus?: 'online' | 'degraded' | 'offline';
}

export function SystemHealth({ metrics, overallStatus = 'online' }: SystemHealthProps) {
  const statusConfig = {
    online: { color: 'text-success', bgColor: 'bg-success/10', label: 'Operational' },
    degraded: { color: 'text-warning', bgColor: 'bg-warning/10', label: 'Degraded' },
    offline: { color: 'text-error', bgColor: 'bg-error/10', label: 'Offline' },
  };

  const status = statusConfig[overallStatus];

  const getProgressColor = (status: string) => {
    switch (status) {
      case 'healthy':
        return 'bg-success';
      case 'warning':
        return 'bg-warning';
      case 'critical':
        return 'bg-error';
      default:
        return 'bg-entity';
    }
  };

  return (
    <Card className="h-full">
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-sm bg-entity/10">
              <Server className="h-5 w-5 text-entity" />
            </div>
            <CardTitle>System Health</CardTitle>
          </div>
          <div className="flex items-center gap-2">
            <motion.div
              className={`w-2 h-2 rounded-full ${status.bgColor} ${status.color}`}
              animate={{ scale: [1, 1.2, 1] }}
              transition={{ duration: 2, repeat: Infinity }}
            />
            <span className={`text-xs font-bold ${status.color} uppercase`}>
              {status.label}
            </span>
          </div>
        </div>
        <CardDescription>Real-time system status and metrics</CardDescription>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="space-y-4">
          {metrics.map((metric, index) => (
            <motion.div
              key={metric.label}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.15, delay: index * 0.05 }}
            >
              <div className="flex items-center justify-between mb-2">
                <span className="text-sm font-semibold text-foreground">{metric.label}</span>
                <span className="text-sm font-mono font-bold text-foreground">
                  {metric.value}{metric.unit || '%'}
                </span>
              </div>
              <div className="relative">
                <div className="h-2 w-full bg-background-tertiary rounded-sm overflow-hidden border border-border">
                  <motion.div
                    className={`h-full ${getProgressColor(metric.status)}`}
                    initial={{ width: 0 }}
                    animate={{ width: `${metric.value}%` }}
                    transition={{ duration: 0.8, delay: index * 0.05 }}
                  />
                </div>
              </div>
            </motion.div>
          ))}
        </div>

        <div className="pt-3 border-t border-border grid grid-cols-2 gap-2">
          <Button size="sm" variant="outline" className="flex items-center gap-2">
            <Activity className="h-4 w-4" />
            <span>Metrics</span>
          </Button>
          <Button size="sm" variant="outline" className="flex items-center gap-2">
            <Database className="h-4 w-4" />
            <span>Logs</span>
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
