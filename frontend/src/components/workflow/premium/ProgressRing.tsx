/**
 * Progress Ring Component
 * Premium circular progress indicator with smooth animations
 *
 * Features:
 * - SVG-based circular progress
 * - Gradient stroke with glow effect
 * - Smooth transitions via Framer Motion
 * - Optional center label/icon
 * - Indeterminate loading state
 */

import React from 'react';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';

export interface ProgressRingProps {
  /** Progress value 0-100 */
  value?: number;
  /** Size in pixels */
  size?: number;
  /** Stroke width in pixels */
  strokeWidth?: number;
  /** Color variant */
  variant?: 'primary' | 'success' | 'warning' | 'error';
  /** Show indeterminate spinner */
  indeterminate?: boolean;
  /** Center content */
  children?: React.ReactNode;
  /** Custom class name */
  className?: string;
}

const variantColors = {
  primary: {
    stroke: 'url(#gradient-primary)',
    bg: 'stroke-blue-100 dark:stroke-blue-950/50',
    glow: 'drop-shadow-[0_0_6px_rgba(0,120,212,0.4)]',
  },
  success: {
    stroke: 'url(#gradient-success)',
    bg: 'stroke-green-100 dark:stroke-green-950/50',
    glow: 'drop-shadow-[0_0_6px_rgba(16,124,16,0.4)]',
  },
  warning: {
    stroke: 'url(#gradient-warning)',
    bg: 'stroke-amber-100 dark:stroke-amber-950/50',
    glow: 'drop-shadow-[0_0_6px_rgba(255,185,0,0.4)]',
  },
  error: {
    stroke: 'url(#gradient-error)',
    bg: 'stroke-red-100 dark:stroke-red-950/50',
    glow: 'drop-shadow-[0_0_6px_rgba(209,52,56,0.4)]',
  },
};

export function ProgressRing({
  value = 0,
  size = 48,
  strokeWidth = 4,
  variant = 'primary',
  indeterminate = false,
  children,
  className,
}: ProgressRingProps) {
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference - (value / 100) * circumference;

  const colors = variantColors[variant];

  return (
    <div
      className={cn('relative inline-flex items-center justify-center', className)}
      style={{ width: size, height: size }}
    >
      {/* SVG Progress Ring */}
      <svg
        width={size}
        height={size}
        className="transform -rotate-90"
      >
        <defs>
          {/* Gradient definitions */}
          <linearGradient id="gradient-primary" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#0078D4" />
            <stop offset="100%" stopColor="#00BCF2" />
          </linearGradient>
          <linearGradient id="gradient-success" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#107C10" />
            <stop offset="100%" stopColor="#00CC6A" />
          </linearGradient>
          <linearGradient id="gradient-warning" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#FFB900" />
            <stop offset="100%" stopColor="#FF8C00" />
          </linearGradient>
          <linearGradient id="gradient-error" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#D13438" />
            <stop offset="100%" stopColor="#E74856" />
          </linearGradient>
        </defs>

        {/* Background circle */}
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={strokeWidth}
          className={colors.bg}
        />

        {/* Progress circle */}
        {indeterminate ? (
          <motion.circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={colors.stroke}
            strokeWidth={strokeWidth}
            strokeLinecap="round"
            strokeDasharray={circumference}
            className={colors.glow}
            initial={{ strokeDashoffset: circumference }}
            animate={{
              strokeDashoffset: [
                circumference * 0.75,
                circumference * 0.25,
                circumference * 0.75,
              ],
              rotate: [0, 360],
            }}
            transition={{
              strokeDashoffset: {
                duration: 2,
                repeat: Infinity,
                ease: 'easeInOut',
              },
              rotate: {
                duration: 2,
                repeat: Infinity,
                ease: 'linear',
              },
            }}
          />
        ) : (
          <motion.circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={colors.stroke}
            strokeWidth={strokeWidth}
            strokeLinecap="round"
            strokeDasharray={circumference}
            className={colors.glow}
            initial={{ strokeDashoffset: circumference }}
            animate={{ strokeDashoffset: offset }}
            transition={{
              duration: 0.6,
              ease: [0.4, 0, 0.2, 1],
            }}
          />
        )}
      </svg>

      {/* Center content */}
      {children && (
        <div className="absolute inset-0 flex items-center justify-center">
          {children}
        </div>
      )}
    </div>
  );
}

/**
 * Progress Ring with percentage label
 */
export function ProgressRingWithLabel({
  value = 0,
  size = 48,
  variant = 'primary',
  className,
}: Omit<ProgressRingProps, 'children'>) {
  return (
    <ProgressRing value={value} size={size} variant={variant} className={className}>
      <motion.span
        className="text-xs font-bold tabular-nums"
        initial={{ opacity: 0, scale: 0.8 }}
        animate={{ opacity: 1, scale: 1 }}
        key={value}
        transition={{ duration: 0.2 }}
      >
        {Math.round(value)}%
      </motion.span>
    </ProgressRing>
  );
}
