/**
 * Quality Badge Component
 * Displays dataset quality score with color-coded badge
 */

import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

interface QualityBadgeProps {
  score: number;
  className?: string;
  showLabel?: boolean;
}

export function QualityBadge({ score, className, showLabel = true }: QualityBadgeProps) {
  // Determine variant based on score
  const getVariant = (score: number) => {
    if (score >= 80) return 'success';
    if (score >= 60) return 'warning';
    return 'destructive';
  };

  // Get label text
  const getLabel = (score: number) => {
    if (score >= 80) return 'Good';
    if (score >= 60) return 'Fair';
    return 'Poor';
  };

  const variant = getVariant(score);
  const label = getLabel(score);

  return (
    <Badge
      variant={variant}
      className={cn('font-mono tabular-nums', className)}
    >
      {score}%
      {showLabel && (
        <>
          {' '}
          <span className="font-sans ml-1">• {label}</span>
        </>
      )}
    </Badge>
  );
}
