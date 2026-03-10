/**
 * Sparkline Component
 * Lightweight SVG line chart for showing trends in stat cards
 */

import { useMemo } from 'react';

export interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  strokeWidth?: number;
  className?: string;
  animate?: boolean;
}

export function Sparkline({
  data,
  width = 80,
  height = 24,
  color = 'currentColor',
  strokeWidth = 1.5,
  className = '',
  animate = true,
}: SparklineProps) {
  const points = useMemo(() => {
    if (data.length === 0) return '';

    const min = Math.min(...data);
    const max = Math.max(...data);
    const range = max - min || 1;

    const xStep = width / (data.length - 1 || 1);
    const yPadding = 2;

    const pathPoints = data.map((value, index) => {
      const x = index * xStep;
      const y = height - yPadding - ((value - min) / range) * (height - yPadding * 2);
      return `${x},${y}`;
    });

    return `M ${pathPoints.join(' L ')}`;
  }, [data, width, height]);

  if (data.length === 0) return null;

  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
    >
      <path
        d={points}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
        style={
          animate
            ? {
                strokeDasharray: '1000',
                strokeDashoffset: '1000',
                animation: 'sparkline-draw 1s ease-out forwards',
              }
            : undefined
        }
      />
      <style>{`
        @keyframes sparkline-draw {
          to {
            stroke-dashoffset: 0;
          }
        }
      `}</style>
    </svg>
  );
}
