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
import { getWorkflowCategoryColor, type WorkflowColorCategory } from '@/lib/workflow-colors';

export interface NodeBadgeProps {
  icon: LucideIcon;
  label: string;
  category: ETLCategory | 'prediction' | 'logic' | 'aggregation' | 'routing' | 'transformation';
  active?: boolean;
  className?: string;
}

export function NodeBadge({
  icon: Icon,
  label,
  category,
  active = false,
  className
}: NodeBadgeProps) {
  const styles = getWorkflowCategoryColor(category as WorkflowColorCategory);
  const background = `linear-gradient(135deg, ${styles.surface} 0%, ${styles.subtle} 100%)`;

  return (
    <motion.div
      className={cn(
        'inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border backdrop-blur-sm relative',
        active && 'ring-2 ring-offset-1 ring-offset-background',
        className
      )}
      style={{
        background,
        borderColor: styles.border,
        boxShadow: active
          ? `0 0 0 1px ${styles.border}, 0 10px 24px color-mix(in srgb, ${styles.base} 22%, transparent)`
          : `0 6px 18px color-mix(in srgb, ${styles.base} 14%, transparent)`,
      }}
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      whileHover={{ scale: 1.02 }}
      transition={{ duration: 0.15 }}
    >
      {/* Pulse effect for active state */}
      {active && (
        <motion.div
          className="absolute -inset-0.5 rounded-md opacity-75"
          style={{ background }}
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
        className="relative z-10 flex-shrink-0"
        style={{ color: styles.text }}
        size={14}
        strokeWidth={2.5}
      />

      {/* Label */}
      <span
        className={cn(
          'relative z-10 text-[10px] font-bold tracking-wide uppercase leading-none',
        )}
        style={{ color: styles.text }}
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
  const styles = getWorkflowCategoryColor(category as WorkflowColorCategory);

  return (
    <motion.div
      className={cn(
        'inline-flex items-center justify-center w-6 h-6 rounded-md border backdrop-blur-sm',
        className
      )}
      style={{
        background: `linear-gradient(135deg, ${styles.surface} 0%, ${styles.subtle} 100%)`,
        borderColor: styles.border,
        boxShadow: `0 4px 12px color-mix(in srgb, ${styles.base} 12%, transparent)`,
      }}
      title={label}
      initial={{ opacity: 0, scale: 0.9 }}
      animate={{ opacity: 1, scale: 1 }}
      whileHover={{ scale: 1.1 }}
      transition={{ duration: 0.15 }}
    >
      <Icon
        style={{ color: styles.text }}
        size={12}
        strokeWidth={2.5}
      />
    </motion.div>
  );
}
