import React, { useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { LucideIcon } from 'lucide-react';
import { motion } from 'framer-motion';

interface StatCardProps {
  title: string;
  value: string | number;
  icon: LucideIcon;
  trend?: {
    value: number;
    isPositive: boolean;
  };
  color?: 'entity' | 'model' | 'success' | 'warning';
  delay?: number;
}

export function StatCard({
  title,
  value,
  icon: Icon,
  trend,
  color = 'entity',
  delay = 0
}: StatCardProps) {
  const [displayValue, setDisplayValue] = useState<string | number>(0);

  // Animate number count-up effect
  useEffect(() => {
    if (typeof value === 'number') {
      let current = 0;
      const increment = value / 30; // 30 frames
      const timer = setInterval(() => {
        current += increment;
        if (current >= value) {
          setDisplayValue(value);
          clearInterval(timer);
        } else {
          setDisplayValue(Math.floor(current));
        }
      }, 30);
      return () => clearInterval(timer);
    } else {
      setDisplayValue(value);
    }
  }, [value]);

  const colorClasses = {
    entity: 'text-entity bg-entity/10',
    model: 'text-model bg-model/10',
    success: 'text-success bg-success/10',
    warning: 'text-warning bg-warning/10',
  };

  const colorClass = colorClasses[color];

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.15, delay }}
    >
      <Card className="h-full hover:border-border-emphasis transition-colors duration-150">
        <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
          <CardTitle className="text-xs font-bold text-muted-foreground uppercase tracking-wide">
            {title}
          </CardTitle>
          <div className={`p-2 rounded-sm ${colorClass}`}>
            <Icon className="h-5 w-5" />
          </div>
        </CardHeader>
        <CardContent>
          <div className="text-2xl font-bold text-foreground mb-1">{displayValue}</div>
          {trend && (
            <p className={`text-xs font-semibold ${trend.isPositive ? 'text-success' : 'text-error'}`}>
              {trend.isPositive ? '↑ +' : '↓ '}{trend.value}% from last month
            </p>
          )}
        </CardContent>
      </Card>
    </motion.div>
  );
}
