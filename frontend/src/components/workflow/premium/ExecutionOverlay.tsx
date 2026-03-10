/**
 * Execution Overlay Component
 * Premium visual overlay for nodes during execution
 *
 * Features:
 * - Animated gradient shimmer
 * - Progress bar with percentage
 * - Real-time metrics display
 * - Smooth entrance/exit animations
 * - Glass morphism effects
 */

import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Clock, Zap } from 'lucide-react';
import { cn } from '@/lib/utils';
import { ProgressRing } from './ProgressRing';

export interface ExecutionMetrics {
  rowsProcessed?: number;
  duration?: number;
  progress?: number;
  throughput?: number;
}

export interface ExecutionOverlayProps {
  visible: boolean;
  metrics?: ExecutionMetrics;
  variant?: 'primary' | 'success' | 'warning' | 'error';
  compact?: boolean;
  className?: string;
}

export function ExecutionOverlay({
  visible,
  metrics,
  variant = 'primary',
  compact = false,
  className
}: ExecutionOverlayProps) {
  const progress = metrics?.progress ?? 0;

  const variantStyles = {
    primary: {
      gradient: 'from-blue-500/10 via-blue-400/5 to-transparent',
      shimmer: 'from-transparent via-blue-500/20 to-transparent',
      text: 'text-blue-700 dark:text-blue-300',
      border: 'border-blue-300 dark:border-blue-700',
    },
    success: {
      gradient: 'from-green-500/10 via-green-400/5 to-transparent',
      shimmer: 'from-transparent via-green-500/20 to-transparent',
      text: 'text-green-700 dark:text-green-300',
      border: 'border-green-300 dark:border-green-700',
    },
    warning: {
      gradient: 'from-amber-500/10 via-amber-400/5 to-transparent',
      shimmer: 'from-transparent via-amber-500/20 to-transparent',
      text: 'text-amber-700 dark:text-amber-300',
      border: 'border-amber-300 dark:border-amber-700',
    },
    error: {
      gradient: 'from-red-500/10 via-red-400/5 to-transparent',
      shimmer: 'from-transparent via-red-500/20 to-transparent',
      text: 'text-red-700 dark:text-red-300',
      border: 'border-red-300 dark:border-red-700',
    },
  };

  const styles = variantStyles[variant];

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          className={cn(
            'absolute inset-0 rounded-md overflow-hidden pointer-events-none',
            className
          )}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        >
          {/* Background gradient overlay */}
          <div className={cn('absolute inset-0 bg-gradient-to-br', styles.gradient)} />

          {/* Animated shimmer effect */}
          <motion.div
            className={cn(
              'absolute inset-0 bg-gradient-to-r',
              styles.shimmer
            )}
            initial={{ x: '-100%' }}
            animate={{ x: '200%' }}
            transition={{
              duration: 2,
              repeat: Infinity,
              ease: 'linear',
            }}
          />

          {/* Progress bar at bottom */}
          {!compact && progress > 0 && (
            <div className="absolute bottom-0 left-0 right-0 h-1 bg-black/5 dark:bg-white/5">
              <motion.div
                className={cn(
                  'h-full',
                  variant === 'primary' && 'bg-gradient-to-r from-blue-500 to-blue-400',
                  variant === 'success' && 'bg-gradient-to-r from-green-500 to-green-400',
                  variant === 'warning' && 'bg-gradient-to-r from-amber-500 to-amber-400',
                  variant === 'error' && 'bg-gradient-to-r from-red-500 to-red-400'
                )}
                initial={{ width: 0 }}
                animate={{ width: `${progress}%` }}
                transition={{ duration: 0.3, ease: 'easeOut' }}
              />
            </div>
          )}

          {/* Metrics overlay (full version) */}
          {!compact && metrics && (
            <motion.div
              className="absolute bottom-2 left-2 right-2 flex items-center justify-between"
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.1 }}
            >
              {/* Rows processed */}
              {metrics.rowsProcessed !== undefined && (
                <div className={cn(
                  'flex items-center gap-1 px-2 py-0.5 rounded-full backdrop-blur-sm border text-[10px] font-semibold',
                  styles.border,
                  styles.text,
                  'bg-white/80 dark:bg-neutral-900/80'
                )}>
                  <Zap size={10} strokeWidth={2.5} />
                  <span>{metrics.rowsProcessed.toLocaleString()}</span>
                </div>
              )}

              {/* Duration */}
              {metrics.duration !== undefined && (
                <div className={cn(
                  'flex items-center gap-1 px-2 py-0.5 rounded-full backdrop-blur-sm border text-[10px] font-semibold',
                  styles.border,
                  styles.text,
                  'bg-white/80 dark:bg-neutral-900/80'
                )}>
                  <Clock size={10} strokeWidth={2.5} />
                  <span>{metrics.duration}ms</span>
                </div>
              )}
            </motion.div>
          )}

          {/* Compact progress ring */}
          {compact && progress > 0 && (
            <motion.div
              className="absolute top-2 right-2"
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ delay: 0.1 }}
            >
              <ProgressRing
                value={progress}
                size={32}
                strokeWidth={3}
                variant={variant}
              >
                <span className={cn('text-[9px] font-bold tabular-nums', styles.text)}>
                  {Math.round(progress)}
                </span>
              </ProgressRing>
            </motion.div>
          )}

          {/* Border glow pulse */}
          <motion.div
            className={cn(
              'absolute inset-0 rounded-md border-2',
              styles.border
            )}
            initial={{ opacity: 0.3 }}
            animate={{ opacity: [0.3, 0.6, 0.3] }}
            transition={{
              duration: 1.5,
              repeat: Infinity,
              ease: 'easeInOut',
            }}
          />
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/**
 * Simple pulse overlay for waiting/queued states
 */
export function WaitingOverlay({
  visible,
  className
}: {
  visible: boolean;
  className?: string;
}) {
  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          className={cn(
            'absolute inset-0 rounded-md overflow-hidden pointer-events-none',
            className
          )}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
        >
          {/* Subtle pulse effect */}
          <motion.div
            className="absolute inset-0 bg-amber-500/5 dark:bg-amber-400/5"
            animate={{
              opacity: [0.3, 0.5, 0.3],
            }}
            transition={{
              duration: 2,
              repeat: Infinity,
              ease: 'easeInOut',
            }}
          />

          {/* Dotted border animation */}
          <motion.div
            className="absolute inset-0 rounded-md border-2 border-dashed border-amber-400 dark:border-amber-600"
            animate={{
              opacity: [0.4, 0.7, 0.4],
            }}
            transition={{
              duration: 1.5,
              repeat: Infinity,
              ease: 'easeInOut',
            }}
          />
        </motion.div>
      )}
    </AnimatePresence>
  );
}
