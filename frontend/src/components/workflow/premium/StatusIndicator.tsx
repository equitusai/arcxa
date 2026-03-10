/**
 * Status Indicator Component
 * Premium live execution status with pulsing animations
 *
 * Visual States:
 * - Idle: Subtle neutral dot
 * - Executing: Pulsing blue ring with animated core
 * - Success: Green checkmark with celebration effect
 * - Error: Red cross with shake effect
 * - Waiting: Amber clock with rotation
 */

import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { CheckCircle2, XCircle, Clock, Loader2, Circle } from 'lucide-react';
import { cn } from '@/lib/utils';

export type ExecutionStatus = 'idle' | 'waiting' | 'executing' | 'success' | 'error';

export interface StatusIndicatorProps {
  status: ExecutionStatus;
  size?: 'sm' | 'md' | 'lg';
  showLabel?: boolean;
  className?: string;
}

const sizeMap = {
  sm: { icon: 12, ring: 20, pulse: 28 },
  md: { icon: 16, ring: 28, pulse: 38 },
  lg: { icon: 20, ring: 36, pulse: 48 },
};

export function StatusIndicator({
  status,
  size = 'md',
  showLabel = false,
  className
}: StatusIndicatorProps) {
  const dimensions = sizeMap[size];

  const statusConfig = {
    idle: {
      icon: Circle,
      color: 'text-neutral-400 dark:text-neutral-600',
      bgColor: 'bg-neutral-100 dark:bg-neutral-800',
      label: 'Idle',
      glow: '',
    },
    waiting: {
      icon: Clock,
      color: 'text-amber-600 dark:text-amber-500',
      bgColor: 'bg-amber-50 dark:bg-amber-950/30',
      label: 'Waiting',
      glow: 'shadow-[0_0_12px_rgba(255,185,0,0.25)]',
    },
    executing: {
      icon: Loader2,
      color: 'text-blue-600 dark:text-blue-400',
      bgColor: 'bg-blue-50 dark:bg-blue-950/30',
      label: 'Running',
      glow: 'shadow-[0_0_16px_rgba(0,120,212,0.35)]',
    },
    success: {
      icon: CheckCircle2,
      color: 'text-green-600 dark:text-green-500',
      bgColor: 'bg-green-50 dark:bg-green-950/30',
      label: 'Success',
      glow: 'shadow-[0_0_12px_rgba(16,124,16,0.25)]',
    },
    error: {
      icon: XCircle,
      color: 'text-red-600 dark:text-red-500',
      bgColor: 'bg-red-50 dark:bg-red-950/30',
      label: 'Failed',
      glow: 'shadow-[0_0_12px_rgba(209,52,56,0.25)]',
    },
  };

  const config = statusConfig[status];
  const Icon = config.icon;

  return (
    <div className={cn('inline-flex items-center gap-2', className)}>
      <div className="relative inline-flex items-center justify-center">
        {/* Pulsing rings for active states */}
        <AnimatePresence>
          {(status === 'executing' || status === 'waiting') && (
            <>
              {/* Outer pulse ring */}
              <motion.div
                key="pulse-outer"
                className={cn(
                  'absolute rounded-full',
                  status === 'executing'
                    ? 'bg-blue-500/20 dark:bg-blue-400/20'
                    : 'bg-amber-500/20 dark:bg-amber-400/20'
                )}
                style={{
                  width: dimensions.pulse,
                  height: dimensions.pulse
                }}
                initial={{ scale: 0.8, opacity: 0 }}
                animate={{
                  scale: [0.8, 1.2, 0.8],
                  opacity: [0, 0.6, 0]
                }}
                exit={{ scale: 0.8, opacity: 0 }}
                transition={{
                  duration: 2,
                  repeat: Infinity,
                  ease: 'easeInOut',
                }}
              />

              {/* Inner pulse ring */}
              <motion.div
                key="pulse-inner"
                className={cn(
                  'absolute rounded-full',
                  status === 'executing'
                    ? 'bg-blue-500/30 dark:bg-blue-400/30'
                    : 'bg-amber-500/30 dark:bg-amber-400/30'
                )}
                style={{
                  width: dimensions.ring,
                  height: dimensions.ring
                }}
                initial={{ scale: 0.9, opacity: 0 }}
                animate={{
                  scale: [0.9, 1.1, 0.9],
                  opacity: [0, 0.8, 0]
                }}
                exit={{ scale: 0.9, opacity: 0 }}
                transition={{
                  duration: 1.5,
                  repeat: Infinity,
                  ease: 'easeInOut',
                  delay: 0.2,
                }}
              />
            </>
          )}
        </AnimatePresence>

        {/* Status icon container */}
        <motion.div
          className={cn(
            'relative rounded-full flex items-center justify-center border-2 border-white dark:border-neutral-900',
            config.bgColor,
            config.glow
          )}
          style={{
            width: dimensions.ring,
            height: dimensions.ring
          }}
          initial={false}
          animate={
            status === 'success'
              ? { scale: [1, 1.15, 1] }
              : status === 'error'
              ? { x: [0, -2, 2, -2, 2, 0] }
              : {}
          }
          transition={
            status === 'success'
              ? { duration: 0.4, ease: 'easeOut' }
              : status === 'error'
              ? { duration: 0.4, ease: 'easeInOut' }
              : {}
          }
        >
          <Icon
            className={cn(
              config.color,
              status === 'executing' && 'animate-spin',
              status === 'waiting' && 'animate-pulse'
            )}
            style={{ width: dimensions.icon, height: dimensions.icon }}
            strokeWidth={2.5}
          />
        </motion.div>

        {/* Success celebration particles */}
        {status === 'success' && (
          <motion.div
            key="celebration"
            className="absolute inset-0 pointer-events-none"
            initial="hidden"
            animate="visible"
            variants={{
              visible: {
                transition: { staggerChildren: 0.05 }
              }
            }}
          >
            {[0, 45, 90, 135, 180, 225, 270, 315].map((angle, i) => (
              <motion.div
                key={i}
                className="absolute w-1 h-1 rounded-full bg-green-500"
                style={{
                  left: '50%',
                  top: '50%',
                }}
                variants={{
                  hidden: { scale: 0, x: 0, y: 0, opacity: 1 },
                  visible: {
                    scale: [0, 1, 0],
                    x: Math.cos((angle * Math.PI) / 180) * 16,
                    y: Math.sin((angle * Math.PI) / 180) * 16,
                    opacity: [1, 1, 0],
                    transition: {
                      duration: 0.6,
                      ease: 'easeOut',
                    }
                  }
                }}
              />
            ))}
          </motion.div>
        )}
      </div>

      {/* Status label */}
      {showLabel && (
        <motion.span
          className={cn(
            'text-xs font-semibold tracking-wide',
            config.color
          )}
          initial={{ opacity: 0, x: -4 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.2 }}
        >
          {config.label}
        </motion.span>
      )}
    </div>
  );
}
