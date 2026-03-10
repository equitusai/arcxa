/**
 * Live Activity Feed
 * Real-time activity stream with filtering
 */

import { useState } from 'react';
import { Activity, Database, FileText, Settings, AlertCircle, CheckCircle2 } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export interface ActivityEvent {
  id: string;
  type: 'workflow' | 'datasource' | 'ontology' | 'system';
  message: string;
  timestamp: Date;
  status?: 'success' | 'error' | 'warning';
  metadata?: Record<string, any>;
}

export interface LiveActivityFeedProps {
  events: ActivityEvent[];
  showLiveIndicator?: boolean;
  maxEvents?: number;
  className?: string;
}

const eventIcons = {
  workflow: FileText,
  datasource: Database,
  ontology: Settings,
  system: Activity,
};

const eventColors = {
  workflow: 'text-blue-500',
  datasource: 'text-purple-500',
  ontology: 'text-green-500',
  system: 'text-orange-500',
};

const statusIcons = {
  success: CheckCircle2,
  error: AlertCircle,
  warning: AlertCircle,
};

export function LiveActivityFeed({
  events,
  showLiveIndicator = true,
  maxEvents = 10,
  className,
}: LiveActivityFeedProps) {
  const [filter, setFilter] = useState<string>('all');
  const [expandedEvent, setExpandedEvent] = useState<string | null>(null);

  const filteredEvents = events
    .filter((event) => filter === 'all' || event.type === filter)
    .slice(0, maxEvents);

  const eventTypes = ['all', ...new Set(events.map((e) => e.type))];

  return (
    <Card className={cn('p-6', className)}>
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Activity className="w-4 h-4 text-primary" />
          <h3 className="text-sm font-semibold">Activity Feed</h3>
          {showLiveIndicator && (
            <div className="flex items-center gap-1.5">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
              </span>
              <span className="text-xs text-green-600 dark:text-green-400 font-medium">Live</span>
            </div>
          )}
        </div>

        {/* Filter Buttons */}
        <div className="flex gap-1">
          {eventTypes.map((type) => (
            <Button
              key={type}
              variant={filter === type ? 'default' : 'ghost'}
              size="sm"
              onClick={() => setFilter(type)}
              className="text-xs capitalize"
            >
              {type}
            </Button>
          ))}
        </div>
      </div>

      {/* Events List */}
      <div className="space-y-3 max-h-[400px] overflow-y-auto">
        {filteredEvents.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground text-sm">
            No activities to display
          </div>
        ) : (
          filteredEvents.map((event, index) => {
            const EventIcon = eventIcons[event.type];
            const StatusIcon = event.status && statusIcons[event.status];

            return (
              <div
                key={event.id}
                className={cn(
                  'flex items-start gap-3 p-3 rounded-lg border border-border bg-background/50',
                  'transition-all duration-300 hover:bg-accent/50',
                  index === 0 && 'animate-in slide-in-from-top-2 duration-300'
                )}
                onClick={() => setExpandedEvent(expandedEvent === event.id ? null : event.id)}
              >
                <div className={cn('p-2 rounded-md bg-muted', eventColors[event.type])}>
                  <EventIcon className="w-4 h-4" />
                </div>

                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-2">
                    <p className="text-sm text-foreground line-clamp-2">{event.message}</p>
                    {StatusIcon && (
                      <StatusIcon
                        className={cn(
                          'w-4 h-4 flex-shrink-0',
                          event.status === 'success' && 'text-green-500',
                          event.status === 'error' && 'text-red-500',
                          event.status === 'warning' && 'text-amber-500'
                        )}
                      />
                    )}
                  </div>

                  <div className="flex items-center gap-2 mt-1">
                    <span className="text-xs text-muted-foreground">
                      {formatRelativeTime(event.timestamp)}
                    </span>
                    <Badge variant="outline" className="text-xs capitalize">
                      {event.type}
                    </Badge>
                  </div>

                  {/* Expanded Metadata */}
                  {expandedEvent === event.id && event.metadata && (
                    <div className="mt-2 p-2 rounded bg-muted/50 text-xs font-mono">
                      {Object.entries(event.metadata).map(([key, value]) => (
                        <div key={key} className="flex gap-2">
                          <span className="text-muted-foreground">{key}:</span>
                          <span>{String(value)}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </Card>
  );
}

function formatRelativeTime(date: Date): string {
  const seconds = Math.floor((new Date().getTime() - date.getTime()) / 1000);

  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}
