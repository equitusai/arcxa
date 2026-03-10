/**
 * Node Badge Component
 * Premium badges for workflow nodes with icon + label
 *
 * Features:
 * - Icon + text combination
 * - Category-based color coding
 * - Glass morphism effect
 * - Smooth hover states
 * - Optional pulse animation for active states
 */

import React from 'react';
import { motion } from 'framer-motion';
import { LucideIcon } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { ETLCategory } from '@/lib/workflow-etl-config';

export interface NodeBadgeProps {
  icon: LucideIcon;
  label: string;
  category: ETLCategory | 'prediction' | 'logic' | 'aggregation' | 'routing' | 'transformation';
  active?: boolean;
  className?: string;
}

const categoryStyles = {
  // ETL Categories - Corporate Color Theory (Oracle Redwood × Microsoft Fluent)
  // ✅ CORRECT: Colored backgrounds (category coding) + Neutral text (readability)
  // Light mode: neutral text (gray-900) on colored bg (category-50/100) = 7:1+ contrast (WCAG AAA)
  // Dark mode: neutral text (white/gray-100) on colored bg (category-950/900) = 7:1+ contrast (WCAG AAA)
  // Principle: Background provides visual coding; text provides readability
  extract: {
    bg: 'bg-gradient-to-br from-blue-50 to-blue-100/80 dark:from-blue-950/40 dark:to-blue-900/30',
    border: 'border-blue-300/60 dark:border-blue-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(0,120,212,0.15)] dark:shadow-[0_2px_8px_rgba(0,120,212,0.25)]',
  },
  transform: {
    bg: 'bg-gradient-to-br from-green-50 to-green-100/80 dark:from-green-950/40 dark:to-green-900/30',
    border: 'border-green-300/60 dark:border-green-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(0,204,106,0.15)] dark:shadow-[0_2px_8px_rgba(0,204,106,0.25)]',
  },
  quality: {
    bg: 'bg-gradient-to-br from-red-50 to-red-100/80 dark:from-red-950/40 dark:to-red-900/30',
    border: 'border-red-300/60 dark:border-red-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(231,72,86,0.15)] dark:shadow-[0_2px_8px_rgba(231,72,86,0.25)]',
  },
  load: {
    bg: 'bg-gradient-to-br from-purple-50 to-purple-100/80 dark:from-purple-950/40 dark:to-purple-900/30',
    border: 'border-purple-300/60 dark:border-purple-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(135,100,184,0.15)] dark:shadow-[0_2px_8px_rgba(135,100,184,0.25)]',
  },
  orchestration: {
    bg: 'bg-gradient-to-br from-orange-50 to-orange-100/80 dark:from-orange-950/40 dark:to-orange-900/30',
    border: 'border-orange-300/60 dark:border-orange-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(255,140,0,0.15)] dark:shadow-[0_2px_8px_rgba(255,140,0,0.25)]',
  },
  // ML/Fusion Categories
  prediction: {
    bg: 'bg-gradient-to-br from-blue-50 to-blue-100/80 dark:from-blue-950/40 dark:to-blue-900/30',
    border: 'border-blue-300/60 dark:border-blue-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(0,120,212,0.15)] dark:shadow-[0_2px_8px_rgba(0,120,212,0.25)]',
  },
  logic: {
    bg: 'bg-gradient-to-br from-amber-50 to-amber-100/80 dark:from-amber-950/40 dark:to-amber-900/30',
    border: 'border-amber-300/60 dark:border-amber-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(255,185,0,0.15)] dark:shadow-[0_2px_8px_rgba(255,185,0,0.25)]',
  },
  aggregation: {
    bg: 'bg-gradient-to-br from-cyan-50 to-cyan-100/80 dark:from-cyan-950/40 dark:to-cyan-900/30',
    border: 'border-cyan-300/60 dark:border-cyan-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(0,188,242,0.15)] dark:shadow-[0_2px_8px_rgba(0,188,242,0.25)]',
  },
  routing: {
    bg: 'bg-gradient-to-br from-violet-50 to-violet-100/80 dark:from-violet-950/40 dark:to-violet-900/30',
    border: 'border-violet-300/60 dark:border-violet-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(135,100,184,0.15)] dark:shadow-[0_2px_8px_rgba(135,100,184,0.25)]',
  },
  transformation: {
    bg: 'bg-gradient-to-br from-emerald-50 to-emerald-100/80 dark:from-emerald-950/40 dark:to-emerald-900/30',
    border: 'border-emerald-300/60 dark:border-emerald-700/50',
    text: 'text-gray-900 dark:text-white',
    icon: 'text-gray-800 dark:text-gray-100',
    glow: 'shadow-[0_2px_8px_rgba(0,204,106,0.15)] dark:shadow-[0_2px_8px_rgba(0,204,106,0.25)]',
  },
};

export function NodeBadge({
  icon: Icon,
  label,
  category,
  active = false,
  className
}: NodeBadgeProps) {
  const styles = categoryStyles[category];

  return (
    <motion.div
      className={cn(
        'inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border backdrop-blur-sm',
        styles.bg,
        styles.border,
        styles.glow,
        active && 'ring-2 ring-offset-1 ring-offset-white dark:ring-offset-neutral-900',
        active && styles.border,
        className
      )}
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      whileHover={{ scale: 1.02 }}
      transition={{ duration: 0.15 }}
    >
      {/* Pulse effect for active state */}
      {active && (
        <motion.div
          className={cn('absolute -inset-0.5 rounded-md opacity-75', styles.bg)}
          initial={{ opacity: 0.4, scale: 1 }}
          animate={{
            opacity: [0.4, 0.6, 0.4],
            scale: [1, 1.05, 1],
          }}
          transition={{
            duration: 2,
            repeat: Infinity,
            ease: 'easeInOut',
          }}
        />
      )}

      {/* Icon */}
      <Icon
        className={cn(styles.icon, 'relative z-10 flex-shrink-0')}
        size={14}
        strokeWidth={2.5}
      />

      {/* Label */}
      <span
        className={cn(
          'relative z-10 text-[10px] font-bold tracking-wide uppercase leading-none',
          styles.text
        )}
      >
        {label}
      </span>
    </motion.div>
  );
}

/**
 * Compact node badge variant (icon only with tooltip)
 */
export function NodeBadgeCompact({
  icon: Icon,
  label,
  category,
  className
}: NodeBadgeProps) {
  const styles = categoryStyles[category];

  return (
    <motion.div
      className={cn(
        'inline-flex items-center justify-center w-6 h-6 rounded-md border backdrop-blur-sm',
        styles.bg,
        styles.border,
        styles.glow,
        className
      )}
      title={label}
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      whileHover={{ scale: 1.1 }}
      transition={{ duration: 0.15 }}
    >
      <Icon
        className={styles.icon}
        size={12}
        strokeWidth={2.5}
      />
    </motion.div>
  );
}
