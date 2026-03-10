/**
 * Enhanced Datasource Stat Card
 * Premium stat card with datasource-specific styling
 */

import { useEffect, useState } from 'react';
import { LucideIcon, TrendingUp, TrendingDown, ArrowRight } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Sparkline } from '@/components/dashboard/Sparkline';
import { cn } from '@/lib/utils';

export interface DatasourceStatCardProps {
  title: string;
  value: number | string;
  icon: LucideIcon;
  trend?: {
    value: number;
    isPositive: boolean;
  };
  sparklineData?: number[];
  action?: {
    label: string;
    onClick: () => void;
  };
  status?: 'success' | 'warning' | 'error' | 'info' | 'neutral';
  className?: string;
}

export function DatasourceStatCard({
  title,
  value,
  icon: Icon,
  trend,
  sparklineData = [],
  action,
  status = 'info',
  className,
}: DatasourceStatCardProps) {
  const [displayValue, setDisplayValue] = useState(0);
  const numericValue = typeof value === 'number' ? value : parseInt(value) || 0;

  // Count-up animation
  useEffect(() => {
    if (typeof value === 'number') {
      const duration = 1200;
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

  const statusConfig = {
    success: {
      gradient: 'from-emerald-500/20 via-emerald-500/10 to-transparent',
      border: 'border-emerald-500/40 hover:border-emerald-500/60',
      icon: 'text-emerald-600',
      iconBg: 'bg-emerald-500/10',
      glow: 'shadow-emerald-500/20',
    },
    warning: {
      gradient: 'from-amber-500/20 via-amber-500/10 to-transparent',
      border: 'border-amber-500/40 hover:border-amber-500/60',
      icon: 'text-amber-600',
      iconBg: 'bg-amber-500/10',
      glow: 'shadow-amber-500/20',
    },
    error: {
      gradient: 'from-red-500/20 via-red-500/10 to-transparent',
      border: 'border-red-500/40 hover:border-red-500/60',
      icon: 'text-red-600',
      iconBg: 'bg-red-500/10',
      glow: 'shadow-red-500/20',
    },
    info: {
      gradient: 'from-blue-500/20 via-blue-500/10 to-transparent',
      border: 'border-blue-500/40 hover:border-blue-500/60',
      icon: 'text-blue-600',
      iconBg: 'bg-blue-500/10',
      glow: 'shadow-blue-500/20',
    },
    neutral: {
      gradient: 'from-gray-500/20 via-gray-500/10 to-transparent',
      border: 'border-gray-300 hover:border-gray-400',
      icon: 'text-gray-600',
      iconBg: 'bg-gray-500/10',
      glow: 'shadow-gray-500/20',
    },
  };

  const config = statusConfig[status];

  return (
    <Card
      className={cn(
        'relative overflow-hidden transition-all duration-300',
        'hover:shadow-xl hover:-translate-y-1',
        'bg-gradient-to-br',
        config.gradient,
        config.border,
        config.glow,
        'group',
        className
      )}
    >
      {/* Animated gradient overlay */}
      <div className="absolute inset-0 bg-gradient-to-br from-background/80 via-background/50 to-transparent pointer-events-none" />

      <div className="relative p-6">
        {/* Header */}
        <div className="flex items-start justify-between mb-4">
          <div className="flex-1">
            <p className="text-sm font-medium text-muted-foreground mb-1">{title}</p>
            <div className="flex items-baseline gap-2">
              <h3 className="text-3xl font-bold tracking-tight">
                {typeof value === 'number' ? displayValue.toLocaleString() : value}
              </h3>
              {trend && (
                <div
                  className={cn(
                    'flex items-center gap-1 text-xs font-semibold px-2 py-0.5 rounded-full',
                    trend.isPositive
                      ? 'bg-emerald-100 text-emerald-700'
                      : 'bg-red-100 text-red-700'
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

          {/* Icon with glow effect */}
          <div
            className={cn(
              'p-3 rounded-xl transition-all duration-300',
              config.iconBg,
              'group-hover:scale-110',
              'shadow-lg',
              config.glow
            )}
          >
            <Icon className={cn('w-6 h-6', config.icon)} />
          </div>
        </div>

        {/* Sparkline */}
        {sparklineData.length > 0 && (
          <div className="mb-4">
            <Sparkline
              data={sparklineData}
              width={240}
              height={48}
              color="currentColor"
              className={cn('opacity-80', config.icon)}
            />
            <div className="flex justify-between text-xs text-muted-foreground mt-1">
              <span>24h trend</span>
              <span>{sparklineData.length} data points</span>
            </div>
          </div>
        )}

        {/* Action Button */}
        {action && (
          <Button
            variant="ghost"
            size="sm"
            onClick={action.onClick}
            className={cn(
              'w-full justify-between',
              'opacity-0 group-hover:opacity-100',
              'transition-all duration-300',
              'hover:bg-accent/50'
            )}
          >
            <span className="text-xs font-medium">{action.label}</span>
            <ArrowRight className="w-3 h-3" />
          </Button>
        )}
      </div>

      {/* Bottom accent bar */}
      <div className={cn('h-1 w-full bg-gradient-to-r', config.icon.replace('text-', 'from-'), 'to-transparent')} />
    </Card>
  );
}
