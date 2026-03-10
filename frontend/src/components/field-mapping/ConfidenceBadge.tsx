/**
 * ConfidenceBadge Component
 *
 * Visual indicator for AI confidence scores with color coding and tooltips.
 */

import { Badge } from '@/components/ui/badge';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { getConfidenceColor, formatConfidence, type ConfidenceBreakdown } from '@/api/field-mapping';
import { CheckCircle, AlertTriangle, AlertCircle } from 'lucide-react';

interface ConfidenceBadgeProps {
  confidence: number;
  breakdown?: ConfidenceBreakdown;
  showIcon?: boolean;
  showLabel?: boolean;
  size?: 'sm' | 'md' | 'lg';
}

export function ConfidenceBadge({
  confidence,
  breakdown,
  showIcon = true,
  showLabel = true,
  size = 'md',
}: ConfidenceBadgeProps) {
  const color = getConfidenceColor(confidence);
  const formattedConfidence = formatConfidence(confidence);

  // Determine icon based on confidence level
  const Icon = confidence >= 0.9
    ? CheckCircle
    : confidence >= 0.7
    ? AlertTriangle
    : AlertCircle;

  // Color classes based on confidence
  const colorClasses: Record<string, string> = {
    green: 'bg-green-100 text-green-800 border-green-200 dark:bg-green-900 dark:text-green-100',
    yellow: 'bg-yellow-100 text-yellow-800 border-yellow-200 dark:bg-yellow-900 dark:text-yellow-100',
    red: 'bg-red-100 text-red-800 border-red-200 dark:bg-red-900 dark:text-red-100',
  };

  const iconSizes: Record<string, string> = {
    sm: 'h-3 w-3',
    md: 'h-3.5 w-3.5',
    lg: 'h-4 w-4',
  };

  const badge = (
    <Badge
      variant="outline"
      className={`${colorClasses[color]} flex items-center gap-1.5`}
    >
      {showIcon && <Icon className={iconSizes[size]} />}
      {showLabel && <span>{formattedConfidence}</span>}
    </Badge>
  );

  // If breakdown is provided, wrap in tooltip
  if (breakdown) {
    return (
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>{badge}</TooltipTrigger>
          <TooltipContent>
            <div className="space-y-1">
              <div className="font-semibold text-sm">Confidence Breakdown</div>
              <div className="text-xs space-y-0.5">
                <div className="flex justify-between gap-3">
                  <span>Statistical:</span>
                  <span className="font-medium">{formatConfidence(breakdown.statistical)}</span>
                </div>
                {breakdown.semantic !== undefined && (
                  <div className="flex justify-between gap-3">
                    <span>Semantic:</span>
                    <span className="font-medium">{formatConfidence(breakdown.semantic)}</span>
                  </div>
                )}
                {breakdown.graph !== undefined && (
                  <div className="flex justify-between gap-3">
                    <span>Graph:</span>
                    <span className="font-medium">{formatConfidence(breakdown.graph)}</span>
                  </div>
                )}
                {breakdown.symbolic !== undefined && (
                  <div className="flex justify-between gap-3">
                    <span>Symbolic:</span>
                    <span className="font-medium">{formatConfidence(breakdown.symbolic)}</span>
                  </div>
                )}
              </div>
              <div className="text-xs text-muted-foreground pt-1 border-t">
                <div className="font-medium">What does this mean?</div>
                {confidence >= 0.9 && (
                  <div>High confidence - strong match found</div>
                )}
                {confidence >= 0.7 && confidence < 0.9 && (
                  <div>Medium confidence - likely correct, review recommended</div>
                )}
                {confidence < 0.7 && (
                  <div>Low confidence - manual review required</div>
                )}
              </div>
            </div>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return badge;
}
