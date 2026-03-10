/**
 * Schedule Status Badge
 * Displays workflow schedule status and next run time
 * Addresses UX Issue C-2: Make schedule status prominent and accessible
 */

import React from 'react';
import { CalendarClock, Clock, Loader2, CalendarX, Globe } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { Badge } from '@/components/ui/badge';

export interface ScheduleStatusBadgeProps {
  /**
   * Primary schedule data from backend (first enabled or first overall)
   */
  schedule?: {
    enabled: boolean;
    cron_expression: string;
    timezone?: string;
    next_run_time?: string;
  };

  /**
   * Total number of schedules for this workflow
   * Used to show "2 schedules" indicator
   */
  scheduleCount?: number;

  /**
   * Whether schedule data is loading
   */
  isLoading?: boolean;

  /**
   * Click handler to open schedule dialog
   */
  onClick?: () => void;

  /**
   * Whether the badge is disabled (e.g., workflow not saved)
   */
  disabled?: boolean;

  /**
   * Compact mode (smaller display)
   */
  compact?: boolean;
}

/**
 * Parse cron expression to human-readable frequency
 */
function getCronFrequency(cronExpression: string): string {
  // Basic cron parsing - can be enhanced
  const parts = cronExpression.split(' ');

  if (parts.length < 5) return 'Custom schedule';

  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  // Daily at specific time
  if (dayOfMonth === '*' && month === '*' && dayOfWeek === '*') {
    return `Daily at ${hour.padStart(2, '0')}:${minute.padStart(2, '0')}`;
  }

  // Weekly on specific day
  if (dayOfMonth === '*' && month === '*' && dayOfWeek !== '*') {
    const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
    const dayName = days[parseInt(dayOfWeek)] || 'Unknown';
    return `Weekly on ${dayName}`;
  }

  // Monthly on specific day
  if (dayOfMonth !== '*' && month === '*') {
    return `Monthly on day ${dayOfMonth}`;
  }

  // Hourly
  if (hour === '*' && dayOfMonth === '*' && month === '*') {
    return 'Hourly';
  }

  return 'Custom schedule';
}

/**
 * Format next run time to relative time
 */
function formatNextRunTime(nextRunTime: string): string {
  const now = new Date();
  const next = new Date(nextRunTime);
  const diffMs = next.getTime() - now.getTime();

  if (diffMs < 0) return 'Overdue';

  const diffMinutes = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffMinutes < 1) return 'in < 1 min';
  if (diffMinutes < 60) return `in ${diffMinutes} min`;
  if (diffHours < 24) return `in ${diffHours}h ${diffMinutes % 60}m`;
  if (diffDays < 7) return `in ${diffDays} day${diffDays !== 1 ? 's' : ''}`;

  // For longer times, show date
  return next.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  });
}

export function ScheduleStatusBadge({
  schedule,
  scheduleCount = 0,
  isLoading = false,
  onClick,
  disabled = false,
  compact = false,
}: ScheduleStatusBadgeProps) {
  // Loading state
  if (isLoading) {
    return (
      <Button
        variant="ghost"
        size={compact ? "sm" : "default"}
        className={cn(
          "gap-2 cursor-default",
          compact && "h-8 px-2"
        )}
        disabled
      >
        <Loader2 className={cn("animate-spin", compact ? "h-3 w-3" : "h-4 w-4")} />
        <span className={cn("text-muted-foreground", compact && "text-xs")}>
          Loading schedule...
        </span>
      </Button>
    );
  }

  // Scheduled state
  if (schedule && schedule.enabled) {
    const frequency = getCronFrequency(schedule.cron_expression);
    const nextRun = schedule.next_run_time
      ? formatNextRunTime(schedule.next_run_time)
      : 'Not calculated';

    return (
      <Button
        variant="outline"
        size={compact ? "sm" : "default"}
        className={cn(
          "gap-2 border-blue-200 bg-blue-50 hover:bg-blue-100 text-blue-900 hover:text-blue-950",
          compact && "h-8 px-2"
        )}
        onClick={onClick}
        disabled={disabled}
      >
        <CalendarClock className={cn(compact ? "h-3 w-3" : "h-4 w-4")} />
        <div className={cn("flex items-center gap-2", compact && "gap-1")}>
          <Badge
            variant="secondary"
            className={cn(
              "bg-blue-600 text-white hover:bg-blue-700",
              compact && "text-xs px-1.5 py-0"
            )}
          >
            Scheduled
          </Badge>
          {scheduleCount > 1 && (
            <Badge
              variant="outline"
              className={cn(
                "bg-blue-100 text-blue-700 border-blue-300",
                compact && "text-[10px] px-1 py-0 h-4"
              )}
            >
              {scheduleCount} schedules
            </Badge>
          )}
          {!compact && (
            <>
              <span className="text-xs text-blue-700">
                {frequency}
              </span>
              <span className="text-xs text-blue-600">•</span>
              <div className="flex items-center gap-1">
                <Clock className="h-3 w-3 text-blue-600" />
                <span className="text-xs font-medium text-blue-800">
                  {nextRun}
                </span>
              </div>
              {schedule.timezone && (
                <>
                  <span className="text-xs text-blue-600">•</span>
                  <div className="flex items-center gap-1">
                    <Globe className="h-3 w-3 text-blue-600" />
                    <span className="text-xs text-blue-700">
                      {schedule.timezone}
                    </span>
                  </div>
                </>
              )}
            </>
          )}
        </div>
      </Button>
    );
  }

  // Unscheduled state (compact mode)
  if (compact) {
    return (
      <Button
        variant="ghost"
        size="sm"
        className="h-8 px-2 gap-1.5 text-muted-foreground hover:text-foreground"
        onClick={onClick}
        disabled={disabled}
      >
        <CalendarX className="h-3 w-3" />
        <span className="text-xs">No schedule</span>
      </Button>
    );
  }

  // Unscheduled state (default mode)
  return (
    <Button
      variant="outline"
      size="default"
      className="gap-2 border-amber-200 bg-amber-50 hover:bg-amber-100 text-amber-900 hover:text-amber-950"
      onClick={onClick}
      disabled={disabled}
    >
      <CalendarX className="h-4 w-4" />
      <div className="flex items-center gap-2">
        <span className="text-sm">Not scheduled</span>
        <span className="text-xs text-amber-700">
          Click to add schedule
        </span>
      </div>
    </Button>
  );
}
