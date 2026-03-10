/**
 * Datasource Activity Feed
 * Real-time connection events and notifications
 */

import { Database, Activity, CheckCircle2, XCircle, AlertTriangle, RefreshCw, Zap } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';

export interface DatasourceEvent {
  id: string;
  datasource_name: string;
  event_type: 'connection' | 'error' | 'test' | 'schema_refresh' | 'enable' | 'disable';
  message: string;
  timestamp: Date;
  status?: 'success' | 'error' | 'warning';
}

export interface DatasourceActivityFeedProps {
  events: DatasourceEvent[];
  showLiveIndicator?: boolean;
  maxEvents?: number;
  className?: string;
}

const eventIcons = {
  connection: Database,
  error: XCircle,
  test: Activity,
  schema_refresh: RefreshCw,
  enable: CheckCircle2,
  disable: AlertTriangle,
};

const eventColors = {
  connection: 'text-blue-500',
  error: 'text-red-500',
  test: 'text-purple-500',
  schema_refresh: 'text-green-500',
  enable: 'text-emerald-500',
  disable: 'text-amber-500',
};

export function DatasourceActivityFeed({
  events,
  showLiveIndicator = true,
  maxEvents = 10,
  className,
}: DatasourceActivityFeedProps) {
  const displayEvents = events.slice(0, maxEvents);

  return (
    <Card className={cn('p-6', className)}>
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <Zap className="w-4 h-4 text-primary" />
          <h3 className="text-sm font-semibold">Data Source Activity</h3>
          {showLiveIndicator && (
            <div className="flex items-center gap-1.5">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-blue-500"></span>
              </span>
              <span className="text-xs text-blue-600 dark:text-blue-400 font-medium">Live</span>
            </div>
          )}
        </div>

        <Badge variant="outline" className="text-xs">
          {events.length} events
        </Badge>
      </div>

      {/* Events List */}
      <div className="space-y-2 max-h-[320px] overflow-y-auto">
        {displayEvents.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground text-sm">
            No recent activity
          </div>
        ) : (
          displayEvents.map((event, index) => {
            const EventIcon = eventIcons[event.event_type];

            return (
              <div
                key={event.id}
                className={cn(
                  'flex items-start gap-3 p-3 rounded-lg border border-border bg-background/50',
                  'transition-all duration-300 hover:bg-accent/50',
                  index === 0 && 'animate-in slide-in-from-top-2 duration-300'
                )}
              >
                <div className={cn('p-2 rounded-md bg-muted', eventColors[event.event_type])}>
                  <EventIcon className="w-3 h-3" />
                </div>

                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between gap-2 mb-1">
                    <p className="text-xs font-medium">{event.datasource_name}</p>
                    {event.status && (
                      <Badge
                        variant="outline"
                        className={cn(
                          'text-[10px] px-1.5 py-0',
                          event.status === 'success' && 'bg-green-50 text-green-700 border-green-300',
                          event.status === 'error' && 'bg-red-50 text-red-700 border-red-300',
                          event.status === 'warning' && 'bg-amber-50 text-amber-700 border-amber-300'
                        )}
                      >
                        {event.status}
                      </Badge>
                    )}
                  </div>

                  <p className="text-xs text-muted-foreground line-clamp-2">{event.message}</p>

                  <div className="flex items-center gap-2 mt-1">
                    <span className="text-xs text-muted-foreground">
                      {formatRelativeTime(event.timestamp)}
                    </span>
                    <span className="text-xs text-muted-foreground">•</span>
                    <span className="text-xs text-muted-foreground capitalize">
                      {event.event_type.replace('_', ' ')}
                    </span>
                  </div>
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
