/**
 * Count Up Number Component
 *
 * Smooth animated number transitions with configurable decimals
 * Professional alternative to jarring instant value changes
 * Based on pattern from EnhancedStatCard
 */

import { useEffect, useState, useRef } from 'react';

interface CountUpNumberProps {
  /** Target value to count up to */
  value: number;
  /** Number of decimal places (default: 0) */
  decimals?: number;
  /** Duration of animation in milliseconds (default: 800) */
  duration?: number;
  /** Optional prefix (e.g., "$") */
  prefix?: string;
  /** Optional suffix (e.g., "%", "ms") */
  suffix?: string;
  /** Custom className */
  className?: string;
  /** Whether to format with thousand separators (default: false) */
  formatThousands?: boolean;
}

export function CountUpNumber({
  value,
  decimals = 0,
  duration = 800,
  prefix = '',
  suffix = '',
  className,
  formatThousands = false,
}: CountUpNumberProps) {
  const [displayValue, setDisplayValue] = useState(value);
  const [isAnimating, setIsAnimating] = useState(false);
  const prevValueRef = useRef(value);
  const rafRef = useRef<number>();
  const startTimeRef = useRef<number>();

  useEffect(() => {
    // Skip animation if value hasn't changed
    if (prevValueRef.current === value) {
      return;
    }

    const startValue = prevValueRef.current;
    const endValue = value;
    const change = endValue - startValue;

    // For very small changes, just update instantly
    if (Math.abs(change) < 0.01) {
      setDisplayValue(endValue);
      prevValueRef.current = endValue;
      return;
    }

    setIsAnimating(true);
    startTimeRef.current = undefined;

    const animate = (currentTime: number) => {
      if (!startTimeRef.current) {
        startTimeRef.current = currentTime;
      }

      const elapsed = currentTime - startTimeRef.current;
      const progress = Math.min(elapsed / duration, 1);

      // Ease-out cubic easing for smooth deceleration
      const easedProgress = 1 - Math.pow(1 - progress, 3);
      const currentValue = startValue + change * easedProgress;

      setDisplayValue(currentValue);

      if (progress < 1) {
        rafRef.current = requestAnimationFrame(animate);
      } else {
        setDisplayValue(endValue);
        setIsAnimating(false);
        prevValueRef.current = endValue;
      }
    };

    rafRef.current = requestAnimationFrame(animate);

    return () => {
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
      }
    };
  }, [value, duration]);

  // Format the display value
  const formatValue = (num: number): string => {
    const fixed = num.toFixed(decimals);
    if (formatThousands) {
      const [integer, decimal] = fixed.split('.');
      const formattedInteger = integer.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
      return decimal ? `${formattedInteger}.${decimal}` : formattedInteger;
    }
    return fixed;
  };

  return (
    <span className={className} aria-live="polite">
      {prefix}
      {formatValue(displayValue)}
      {suffix}
    </span>
  );
}

/**
 * Simplified count-up for integers only (no decimals)
 */
export function CountUpInteger({
  value,
  duration = 800,
  prefix = '',
  suffix = '',
  className,
}: Omit<CountUpNumberProps, 'decimals' | 'formatThousands'>) {
  return (
    <CountUpNumber
      value={value}
      decimals={0}
      duration={duration}
      prefix={prefix}
      suffix={suffix}
      className={className}
      formatThousands={true}
    />
  );
}

/**
 * Count-up for percentages
 */
export function CountUpPercentage({
  value,
  decimals = 0,
  duration = 800,
  className,
}: Pick<CountUpNumberProps, 'value' | 'decimals' | 'duration' | 'className'>) {
  return (
    <CountUpNumber
      value={value}
      decimals={decimals}
      duration={duration}
      suffix="%"
      className={className}
    />
  );
}
