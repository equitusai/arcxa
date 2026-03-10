/**
 * Refresh Indicator
 *
 * Subtle, non-blocking refresh spinner shown in corner of cards during background data fetches
 * Professional alternative to full-page loading states
 */

import { motion } from 'framer-motion';
import { RefreshCw } from 'lucide-react';
import { cn } from '@/lib/utils';

interface RefreshIndicatorProps {
  /** Position of the indicator (default: top-right) */
  position?: 'top-right' | 'top-left' | 'bottom-right' | 'bottom-left';
  /** Size of the icon (default: 16) */
  size?: number;
  /** Custom className */
  className?: string;
}

export function RefreshIndicator({
  position = 'top-right',
  size = 16,
  className,
}: RefreshIndicatorProps) {
  const positionClasses = {
    'top-right': 'top-3 right-3',
    'top-left': 'top-3 left-3',
    'bottom-right': 'bottom-3 right-3',
    'bottom-left': 'bottom-3 left-3',
  };

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.8 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.8 }}
      transition={{ duration: 0.15 }}
      className={cn(
        'absolute z-10 pointer-events-none',
        positionClasses[position],
        className
      )}
      aria-label="Refreshing data"
    >
      <div className="p-1.5 rounded-md bg-background/80 backdrop-blur-sm border border-border shadow-sm">
        <RefreshCw
          className="text-muted-foreground animate-spin"
          size={size}
          style={{ animationDuration: '1s' }}
        />
      </div>
    </motion.div>
  );
}

/**
 * Inline refresh indicator for use in text or button contexts
 */
export function InlineRefreshIndicator({ size = 14, className }: { size?: number; className?: string }) {
  return (
    <RefreshCw
      className={cn('inline-block text-muted-foreground animate-spin', className)}
      size={size}
      style={{ animationDuration: '1s' }}
    />
  );
}
