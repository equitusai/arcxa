/**
 * Enhanced Stat Card
 * Premium stat card with gradients, sparklines, and animations
 */

import { useEffect, useState } from 'react';
import { LucideIcon, TrendingUp, TrendingDown, ArrowRight } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Sparkline } from './Sparkline';
import { cn } from '@/lib/utils';

export interface EnhancedStatCardProps {
  title: string;
  value: number | string;
  icon: LucideIcon;
  trend?: {
    value: number;
    isPositive: boolean;
  };
  sparklineData?: number[];
  gradient?: string;
  action?: {
    label: string;
    onClick: () => void;
  };
  status?: 'success' | 'warning' | 'error' | 'info';
  className?: string;
}

export function EnhancedStatCard({
  title,
  value,
  icon: Icon,
  trend,
  sparklineData = [],
  gradient,
  action,
  status = 'info',
  className,
}: EnhancedStatCardProps) {
  const [displayValue, setDisplayValue] = useState(0);
  const numericValue = typeof value === 'number' ? value : parseInt(value) || 0;

  // Count-up animation
  useEffect(() => {
    if (typeof value === 'number') {
      const duration = 1000;
      const steps = 60;
      const increment = value / steps;
      let current = 0;

      const timer = setInterval(() => {
        current += increment;
        if (current >= value) {
          setDisplayValue(value);
          clearInterval(timer);
        } else {
          setDisplayValue(Math.floor(current));
        }
      }, duration / steps);

      return () => clearInterval(timer);
    }
  }, [value]);

  const statusGradients = {
    success: 'from-emerald-500/20 to-emerald-600/10',
    warning: 'from-amber-500/20 to-amber-600/10',
    error: 'from-red-500/20 to-red-600/10',
    info: 'from-blue-500/20 to-blue-600/10',
  };

  const statusBorders = {
    success: 'border-emerald-500/30 hover:border-emerald-500/50',
    warning: 'border-amber-500/30 hover:border-amber-500/50',
    error: 'border-red-500/30 hover:border-red-500/50',
    info: 'border-blue-500/30 hover:border-blue-500/50',
  };

  const statusIcons = {
    success: 'text-emerald-500',
    warning: 'text-amber-500',
    error: 'text-red-500',
    info: 'text-blue-500',
  };

  return (
    <Card
      className={cn(
        'relative overflow-hidden transition-all duration-300 hover:shadow-lg hover:-translate-y-1',
        'bg-gradient-to-br',
        gradient || statusGradients[status],
        statusBorders[status],
        className
      )}
    >
      <div className="p-6">
        {/* Header */}
        <div className="flex items-start justify-between mb-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground">{title}</p>
            <div className="flex items-baseline gap-2 mt-1">
              <h3 className="text-3xl font-bold">
                {typeof value === 'number' ? displayValue.toLocaleString() : value}
              </h3>
              {trend && (
                <div
                  className={cn(
                    'flex items-center gap-1 text-xs font-medium',
                    trend.isPositive ? 'text-emerald-600' : 'text-red-600'
                  )}
                >
                  {trend.isPositive ? (
                    <TrendingUp className="w-3 h-3" />
                  ) : (
                    <TrendingDown className="w-3 h-3" />
                  )}
                  {Math.abs(trend.value)}%
                </div>
              )}
            </div>
          </div>
          <div className={cn('p-3 rounded-lg bg-background/50', statusIcons[status])}>
            <Icon className="w-5 h-5" />
          </div>
        </div>

        {/* Sparkline */}
        {sparklineData.length > 0 && (
          <div className="mb-4">
            <Sparkline
              data={sparklineData}
              width={200}
              height={40}
              color="currentColor"
              className={cn('opacity-70', statusIcons[status])}
            />
          </div>
        )}

        {/* Action Button */}
        {action && (
          <Button
            variant="ghost"
            size="sm"
            onClick={action.onClick}
            className="w-full justify-between opacity-0 group-hover:opacity-100 transition-opacity"
          >
            {action.label}
            <ArrowRight className="w-4 h-4" />
          </Button>
        )}
      </div>
    </Card>
  );
}
