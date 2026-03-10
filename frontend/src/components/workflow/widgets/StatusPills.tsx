/**
 * Status Pills Component
 * Display status counts with color coding (e.g., ✓ 4 Approved, ⏳ 2 Pending)
 */

import React from 'react';
import { CheckCircle, Clock, XCircle } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface StatusPill {
  label: string;
  count: number;
  color: 'success' | 'warning' | 'danger' | 'secondary';
  icon?: 'check' | 'clock' | 'x';
  onClick?: () => void;
}

interface StatusPillsProps {
  pills: StatusPill[];
  className?: string;
}

const iconMap = {
  check: CheckCircle,
  clock: Clock,
  x: XCircle,
};

const colorStyles = {
  success: {
    bg: 'bg-green-50',
    text: 'text-green-700',
    hover: 'hover:bg-green-100',
  },
  warning: {
    bg: 'bg-amber-50',
    text: 'text-amber-700',
    hover: 'hover:bg-amber-100',
  },
  danger: {
    bg: 'bg-red-50',
    text: 'text-red-700',
    hover: 'hover:bg-red-100',
  },
  secondary: {
    bg: 'bg-neutral-50',
    text: 'text-neutral-700',
    hover: 'hover:bg-neutral-100',
  },
};

export function StatusPills({ pills, className }: StatusPillsProps) {
  return (
    <div className={cn('flex flex-wrap gap-2', className)}>
      {pills.map((pill, idx) => {
        const style = colorStyles[pill.color];
        const Icon = pill.icon ? iconMap[pill.icon] : null;

        return (
          <button
            key={idx}
            onClick={pill.onClick}
            disabled={!pill.onClick}
            className={cn(
              'inline-flex items-center gap-1.5 px-2 py-1 rounded text-xs font-semibold transition-colors',
              style.bg,
              style.text,
              pill.onClick ? cn('cursor-pointer', style.hover) : 'cursor-default'
            )}
            title={pill.onClick ? `Click to filter ${pill.label}` : undefined}
          >
            {Icon && <Icon className="w-3 h-3" />}
            <span>{pill.count}</span>
          </button>
        );
      })}
    </div>
  );
}
