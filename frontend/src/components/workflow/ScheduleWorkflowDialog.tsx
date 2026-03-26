/**
 * Schedule Workflow Dialog Component
 * Enhanced dialog for scheduling automatic workflow execution with timezone support
 */

import React, { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Switch } from '@/components/ui/switch';
import { Calendar, Clock, Loader2, Timer, Globe, ChevronDown } from 'lucide-react';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';
import type { ScheduleWorkflowRequest } from '@/api/types';

interface ScheduleWorkflowDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workflowId: string;
  workflowName: string;
  supportsInputlessExecution?: boolean;
  onSchedule: (request: ScheduleWorkflowRequest) => Promise<void>;
  isScheduling: boolean;
}

// Common timezones
const TIMEZONES = [
  { value: 'UTC', label: 'UTC (Coordinated Universal Time)' },
  { value: 'America/New_York', label: 'Eastern Time (US & Canada)' },
  { value: 'America/Chicago', label: 'Central Time (US & Canada)' },
  { value: 'America/Denver', label: 'Mountain Time (US & Canada)' },
  { value: 'America/Los_Angeles', label: 'Pacific Time (US & Canada)' },
  { value: 'Europe/London', label: 'London (GMT/BST)' },
  { value: 'Europe/Paris', label: 'Paris (CET/CEST)' },
  { value: 'Europe/Berlin', label: 'Berlin (CET/CEST)' },
  { value: 'Asia/Tokyo', label: 'Tokyo (JST)' },
  { value: 'Asia/Shanghai', label: 'Shanghai (CST)' },
  { value: 'Asia/Singapore', label: 'Singapore (SGT)' },
  { value: 'Asia/Dubai', label: 'Dubai (GST)' },
  { value: 'Australia/Sydney', label: 'Sydney (AEDT/AEST)' },
];

// Schedule templates organized by category
const SCHEDULE_TEMPLATES = {
  'Common Schedules': [
    { label: 'Daily at 9 AM', cron: '0 9 * * *', description: 'Every day at 9:00 AM' },
    { label: 'Hourly', cron: '0 * * * *', description: 'Every hour on the hour' },
    { label: 'Every 30 minutes', cron: '*/30 * * * *', description: 'Twice per hour' },
    { label: 'Weekdays at 9 AM', cron: '0 9 * * 1-5', description: 'Monday-Friday at 9:00 AM' },
  ],
  'Daily Schedules': [
    { label: 'Midnight', cron: '0 0 * * *', description: 'Every day at midnight' },
    { label: '6 AM', cron: '0 6 * * *', description: 'Every day at 6:00 AM' },
    { label: 'Noon', cron: '0 12 * * *', description: 'Every day at noon' },
    { label: '6 PM', cron: '0 18 * * *', description: 'Every day at 6:00 PM' },
  ],
  'Weekly Schedules': [
    { label: 'Monday 9 AM', cron: '0 9 * * 1', description: 'Every Monday at 9:00 AM' },
    { label: 'Friday 5 PM', cron: '0 17 * * 5', description: 'Every Friday at 5:00 PM' },
    { label: 'Sunday midnight', cron: '0 0 * * 0', description: 'Every Sunday at midnight' },
  ],
  'Monthly Schedules': [
    { label: '1st of month at midnight', cron: '0 0 1 * *', description: 'First day of every month' },
    { label: '15th of month at noon', cron: '0 12 15 * *', description: 'Middle of every month' },
    { label: 'Last day of month', cron: '0 0 L * *', description: 'Last day of every month' },
  ],
};

/**
 * Parse cron expression to human-readable description
 */
function parseCronDescription(cron: string): string {
  const parts = cron.split(' ');
  if (parts.length < 5) return 'Custom schedule';

  const [minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  // Check templates first for exact matches
  for (const category of Object.values(SCHEDULE_TEMPLATES)) {
    for (const template of category) {
      if (template.cron === cron) {
        return template.description;
      }
    }
  }

  // Parse common patterns
  if (dayOfMonth === '*' && month === '*' && dayOfWeek === '*') {
    if (minute === '*' && hour === '*') return 'Every minute';
    if (minute === '0' && hour === '*') return 'Hourly';
    if (minute === '*/15') return 'Every 15 minutes';
    if (minute === '*/30') return 'Every 30 minutes';
    return `Daily at ${hour.padStart(2, '0')}:${minute.padStart(2, '0')}`;
  }

  if (dayOfMonth === '*' && month === '*' && dayOfWeek !== '*') {
    const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
    const dayName = days[parseInt(dayOfWeek)] || dayOfWeek;
    return `Weekly on ${dayName} at ${hour.padStart(2, '0')}:${minute.padStart(2, '0')}`;
  }

  if (dayOfMonth !== '*' && month === '*') {
    return `Monthly on day ${dayOfMonth} at ${hour.padStart(2, '0')}:${minute.padStart(2, '0')}`;
  }

  return 'Custom schedule';
}

/**
 * Calculate next N occurrences of a cron schedule
 * Simplified version - for production use a library like cron-parser
 */
function calculateNextRuns(cron: string, timezone: string, count: number = 5): Date[] {
  const runs: Date[] = [];
  const now = new Date();

  // Parse cron expression
  const parts = cron.split(' ');
  if (parts.length < 5) return runs;

  const [minuteStr, hourStr, dayOfMonthStr, monthStr, dayOfWeekStr] = parts;

  // Simple implementation for common patterns
  // For production, use a proper cron parser library

  let currentDate = new Date(now);
  currentDate.setSeconds(0);
  currentDate.setMilliseconds(0);

  const minute = minuteStr === '*' ? -1 : parseInt(minuteStr);
  const hour = hourStr === '*' ? -1 : parseInt(hourStr);

  // Handle simple daily schedules (e.g., "0 9 * * *")
  if (dayOfMonthStr === '*' && monthStr === '*' && dayOfWeekStr === '*' && hour >= 0 && minute >= 0) {
    for (let i = 0; i < count * 2 && runs.length < count; i++) {
      const nextRun = new Date(currentDate);
      nextRun.setHours(hour);
      nextRun.setMinutes(minute);

      if (nextRun > now) {
        runs.push(new Date(nextRun));
      }

      currentDate.setDate(currentDate.getDate() + 1);
    }
  }
  // Handle hourly schedules (e.g., "0 * * * *")
  else if (hourStr === '*' && dayOfMonthStr === '*' && monthStr === '*' && dayOfWeekStr === '*' && minute >= 0) {
    for (let i = 0; i < count * 2 && runs.length < count; i++) {
      const nextRun = new Date(currentDate);
      nextRun.setMinutes(minute);

      if (nextRun > now) {
        runs.push(new Date(nextRun));
      }

      currentDate.setHours(currentDate.getHours() + 1);
    }
  }
  // Handle weekly schedules (e.g., "0 9 * * 1")
  else if (dayOfMonthStr === '*' && monthStr === '*' && dayOfWeekStr !== '*' && hour >= 0 && minute >= 0) {
    const targetDayOfWeek = parseInt(dayOfWeekStr);

    for (let i = 0; i < count * 14 && runs.length < count; i++) {
      if (currentDate.getDay() === targetDayOfWeek) {
        const nextRun = new Date(currentDate);
        nextRun.setHours(hour);
        nextRun.setMinutes(minute);

        if (nextRun > now) {
          runs.push(new Date(nextRun));
        }
      }

      currentDate.setDate(currentDate.getDate() + 1);
    }
  }
  // Handle monthly schedules (e.g., "0 0 1 * *")
  else if (dayOfMonthStr !== '*' && monthStr === '*' && hour >= 0 && minute >= 0) {
    const targetDay = parseInt(dayOfMonthStr);

    for (let i = 0; i < count * 2 && runs.length < count; i++) {
      if (currentDate.getDate() === targetDay) {
        const nextRun = new Date(currentDate);
        nextRun.setHours(hour);
        nextRun.setMinutes(minute);

        if (nextRun > now) {
          runs.push(new Date(nextRun));
        }
      }

      currentDate.setDate(currentDate.getDate() + 1);
    }
  }

  return runs.slice(0, count);
}

/**
 * Format date for display with timezone
 */
function formatScheduledTime(date: Date, timezone: string): string {
  const options: Intl.DateTimeFormatOptions = {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    timeZone: timezone,
    timeZoneName: 'short',
  };

  return date.toLocaleString('en-US', options);
}

export function ScheduleWorkflowDialog({
  open,
  onOpenChange,
  workflowId,
  workflowName,
  supportsInputlessExecution = false,
  onSchedule,
  isScheduling,
}: ScheduleWorkflowDialogProps) {
  const [scheduleType, setScheduleType] = useState<'cron' | 'interval' | 'onetime'>('cron');
  const [cronExpression, setCronExpression] = useState('0 9 * * *');
  const [timezone, setTimezone] = useState('UTC');
  const [intervalSeconds, setIntervalSeconds] = useState(3600);
  const [scheduledAt, setScheduledAt] = useState('');
  const [inputData, setInputData] = useState('{\n  "data": "sample input"\n}');
  const [contextData, setContextData] = useState('{\n  "tenant_id": "default"\n}');
  const [useExternalInput, setUseExternalInput] = useState(!supportsInputlessExecution);
  const [enabled, setEnabled] = useState(true);
  const [nextRuns, setNextRuns] = useState<Date[]>([]);

  useEffect(() => {
    if (!open) {
      return;
    }

    setUseExternalInput(!supportsInputlessExecution);
    setInputData((current) => {
      if (supportsInputlessExecution && current === '{\n  "data": "sample input"\n}') {
        return 'null';
      }

      if (!supportsInputlessExecution && current.trim() === 'null') {
        return '{\n  "data": "sample input"\n}';
      }

      return current;
    });
  }, [open, supportsInputlessExecution]);

  // Calculate next runs whenever cron expression or timezone changes
  useEffect(() => {
    if (scheduleType === 'cron' && cronExpression) {
      try {
        const runs = calculateNextRuns(cronExpression, timezone, 5);
        setNextRuns(runs);
      } catch (error) {
        setNextRuns([]);
      }
    } else {
      setNextRuns([]);
    }
  }, [cronExpression, timezone, scheduleType]);

  const handleSchedule = async () => {
    try {
      const input = useExternalInput ? JSON.parse(inputData) : null;
      const context = contextData.trim() ? JSON.parse(contextData) : undefined;

      const request: ScheduleWorkflowRequest = {
        input,
        context,
        enabled,
        timezone, // Send timezone to backend (requires backend v0.3.0+)
        ...(scheduleType === 'cron' && { cron_expression: cronExpression }),
        ...(scheduleType === 'interval' && { interval_seconds: intervalSeconds }),
        ...(scheduleType === 'onetime' && { scheduled_at: scheduledAt }),
      };

      await onSchedule(request);
      onOpenChange(false);
    } catch (error: any) {
      if (error.message?.includes('JSON')) {
        toast.error('Invalid JSON in input or context');
      } else {
        toast.error('Failed to schedule workflow');
      }
    }
  };

  const cronDescription = parseCronDescription(cronExpression);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Schedule Workflow: {workflowName}</DialogTitle>
          <DialogDescription>
            Configure automatic execution with timezone support.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 mt-4">
          {/* Schedule Type Selection */}
          <Tabs value={scheduleType} onValueChange={(v) => setScheduleType(v as any)}>
            <TabsList className="grid w-full grid-cols-3">
              <TabsTrigger value="cron" className="gap-2">
                <Clock className="h-4 w-4" />
                Cron
              </TabsTrigger>
              <TabsTrigger value="interval" className="gap-2">
                <Timer className="h-4 w-4" />
                Interval
              </TabsTrigger>
              <TabsTrigger value="onetime" className="gap-2">
                <Calendar className="h-4 w-4" />
                One-Time
              </TabsTrigger>
            </TabsList>

            {/* Cron Schedule */}
            <TabsContent value="cron" className="space-y-4 mt-4">
              <div className="grid grid-cols-2 gap-4">
                {/* Left Column: Cron Expression */}
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label>Cron Expression</Label>
                    <Input
                      value={cronExpression}
                      onChange={(e) => setCronExpression(e.target.value)}
                      placeholder="0 9 * * *"
                      className="font-mono"
                    />
                    <p className="text-xs text-muted-foreground">
                      Standard cron format: minute hour day month weekday
                    </p>
                    {cronDescription && (
                      <div className="p-2 bg-blue-50 border border-blue-200 rounded text-sm">
                        <p className="text-blue-900">→ {cronDescription}</p>
                      </div>
                    )}
                  </div>

                  {/* Timezone Selector */}
                  <div className="space-y-2">
                    <Label className="flex items-center gap-2">
                      <Globe className="h-4 w-4" />
                      Timezone
                    </Label>
                    <Select value={timezone} onValueChange={setTimezone}>
                      <SelectTrigger>
                        <SelectValue placeholder="Select timezone" />
                      </SelectTrigger>
                      <SelectContent>
                        {TIMEZONES.map((tz) => (
                          <SelectItem key={tz.value} value={tz.value}>
                            {tz.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-muted-foreground">
                      Schedule will execute in this timezone
                    </p>
                  </div>

                  {/* Next Runs Preview */}
                  {nextRuns.length > 0 && (
                    <div className="space-y-2">
                      <Label className="text-sm font-semibold">Next 5 Scheduled Runs</Label>
                      <div className="p-3 bg-muted rounded border space-y-1.5">
                        {nextRuns.map((run, index) => (
                          <div
                            key={index}
                            className="flex items-center gap-2 text-xs text-foreground font-mono"
                          >
                            <span className="text-muted-foreground">{index + 1}.</span>
                            <span>{formatScheduledTime(run, timezone)}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>

                {/* Right Column: Schedule Templates */}
                <div className="space-y-3">
                  <Label className="text-sm font-semibold">Schedule Templates</Label>
                  <div className="space-y-3 max-h-[400px] overflow-y-auto pr-2">
                    {Object.entries(SCHEDULE_TEMPLATES).map(([category, templates]) => (
                      <div key={category} className="space-y-1.5">
                        <p className="text-xs font-semibold text-muted-foreground">{category}</p>
                        <div className="space-y-1">
                          {templates.map((template) => (
                            <button
                              key={template.cron}
                              onClick={() => setCronExpression(template.cron)}
                              className={cn(
                                'w-full text-left px-3 py-2 text-xs rounded transition-colors',
                                cronExpression === template.cron
                                  ? 'bg-primary text-primary-foreground'
                                  : 'bg-muted hover:bg-muted/80'
                              )}
                            >
                              <div className="font-medium">{template.label}</div>
                              <div className={cn(
                                'font-mono text-[10px]',
                                cronExpression === template.cron
                                  ? 'text-primary-foreground/80'
                                  : 'text-muted-foreground'
                              )}>
                                {template.cron}
                              </div>
                            </button>
                          ))}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </TabsContent>

            {/* Interval Schedule */}
            <TabsContent value="interval" className="space-y-4 mt-4">
              <div className="space-y-2">
                <Label>Interval (seconds)</Label>
                <Input
                  type="number"
                  value={intervalSeconds}
                  onChange={(e) => setIntervalSeconds(parseInt(e.target.value) || 60)}
                  min={60}
                  step={60}
                />
                <p className="text-xs text-muted-foreground">
                  Execute every {intervalSeconds} seconds
                  {intervalSeconds >= 60 && ` (${Math.floor(intervalSeconds / 60)} minutes)`}
                  {intervalSeconds >= 3600 && ` (${Math.floor(intervalSeconds / 3600)} hours)`}
                </p>
              </div>

              <div className="grid grid-cols-4 gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setIntervalSeconds(300)}
                  className="text-xs"
                >
                  5 min
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setIntervalSeconds(900)}
                  className="text-xs"
                >
                  15 min
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setIntervalSeconds(3600)}
                  className="text-xs"
                >
                  1 hour
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setIntervalSeconds(86400)}
                  className="text-xs"
                >
                  1 day
                </Button>
              </div>
            </TabsContent>

            {/* One-Time Schedule */}
            <TabsContent value="onetime" className="space-y-4 mt-4">
              <div className="space-y-2">
                <Label>Scheduled Date & Time</Label>
                <Input
                  type="datetime-local"
                  value={scheduledAt}
                  onChange={(e) => setScheduledAt(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  Execute once at the specified date and time
                </p>
              </div>
            </TabsContent>
          </Tabs>

          {supportsInputlessExecution ? (
            <div className="space-y-3 rounded-sm border border-border p-4">
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-1">
                  <Label className="text-sm font-medium">External input payload</Label>
                  <p className="text-xs text-muted-foreground">
                    This workflow can run directly from its configured source steps. Keep this
                    off unless you want scheduled runs to inject an additional JSON payload.
                  </p>
                </div>
                <Switch checked={useExternalInput} onCheckedChange={setUseExternalInput} />
              </div>

              {useExternalInput && (
                <div className="space-y-2">
                  <Label>Input Data (JSON)</Label>
                  <Textarea
                    value={inputData}
                    onChange={(e) => setInputData(e.target.value)}
                    className="font-mono text-sm h-32"
                    placeholder='{"data": "sample input"}'
                  />
                  <p className="text-xs text-muted-foreground">
                    Input data that will be passed to the workflow on each execution.
                  </p>
                </div>
              )}
            </div>
          ) : (
            <div className="space-y-2">
              <Label>Input Data (JSON)</Label>
              <Textarea
                value={inputData}
                onChange={(e) => setInputData(e.target.value)}
                className="font-mono text-sm h-32"
                placeholder='{"data": "sample input"}'
              />
              <p className="text-xs text-muted-foreground">
                Input data that will be passed to the workflow on each execution
              </p>
            </div>
          )}

          {/* Context Data (Optional) */}
          <div className="space-y-2">
            <Label>Context (JSON, Optional)</Label>
            <Textarea
              value={contextData}
              onChange={(e) => setContextData(e.target.value)}
              className="font-mono text-sm h-24"
              placeholder='{"tenant_id": "default"}'
            />
            <p className="text-xs text-muted-foreground">
              Optional context data for workflow execution
            </p>
          </div>

          {/* Enabled Toggle */}
          <div className="flex items-center justify-between p-3 bg-muted rounded-sm">
            <div>
              <Label className="text-sm font-medium">Enable Schedule</Label>
              <p className="text-xs text-muted-foreground">
                Schedule will {enabled ? 'start immediately' : 'be created but disabled'}
              </p>
            </div>
            <Switch checked={enabled} onCheckedChange={setEnabled} />
          </div>

          {/* Actions */}
          <div className="flex gap-2 pt-4 border-t">
            <Button
              onClick={handleSchedule}
              disabled={isScheduling}
              className="flex-1"
            >
              {isScheduling ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Scheduling...
                </>
              ) : (
                <>
                  <Clock className="h-4 w-4 mr-2" />
                  Create Schedule
                </>
              )}
            </Button>
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
