/**
 * Auto-save Indicator Component
 * Displays save status and last saved time
 */

import { Loader2, Check, AlertCircle, Cloud } from 'lucide-react';
import { cn } from '@/lib/utils';
import { formatDistanceToNow } from 'date-fns';

interface AutoSaveIndicatorProps {
  isSaving: boolean;
  lastSaved: Date | null;
  error: Error | null;
  className?: string;
}

export function AutoSaveIndicator({
  isSaving,
  lastSaved,
  error,
  className,
}: AutoSaveIndicatorProps) {
  if (error) {
    return (
      <div className={cn('flex items-center gap-1.5 text-xs text-error', className)}>
        <AlertCircle className="h-3.5 w-3.5" />
        <span>Failed to save</span>
      </div>
    );
  }

  if (isSaving) {
    return (
      <div className={cn('flex items-center gap-1.5 text-xs text-muted-foreground', className)}>
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        <span>Saving...</span>
      </div>
    );
  }

  if (lastSaved) {
    return (
      <div className={cn('flex items-center gap-1.5 text-xs text-muted-foreground', className)}>
        <Check className="h-3.5 w-3.5 text-success" />
        <span>Saved {formatDistanceToNow(lastSaved, { addSuffix: true })}</span>
      </div>
    );
  }

  return (
    <div className={cn('flex items-center gap-1.5 text-xs text-muted-foreground', className)}>
      <Cloud className="h-3.5 w-3.5" />
      <span>Not saved</span>
    </div>
  );
}
