import React from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Sparkles } from 'lucide-react';
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import { format, subHours, startOfHour, parseISO } from 'date-fns';
import type { AuditLogEntry } from '@/api/types';

interface ActivityChartProps {
  events?: AuditLogEntry[];
  hours?: number;
}

interface ChartDataPoint {
  hour: string;
  time: Date;
  authentication: number;
  dataAccess: number;
  administration: number;
  security: number;
  total: number;
}

// Categorize audit event types
const categorizeEvent = (eventType: string): keyof Omit<ChartDataPoint, 'hour' | 'time' | 'total'> => {
  if (eventType.includes('login') || eventType.includes('logout') || eventType.includes('token')) {
    return 'authentication';
  }
  if (eventType.includes('data') || eventType.includes('query')) {
    return 'dataAccess';
  }
  if (eventType.includes('user') || eventType.includes('admin') || eventType.includes('configuration')) {
    return 'administration';
  }
  return 'security';
};

export function ActivityChart({ events = [], hours = 24 }: ActivityChartProps) {
  const chartData = React.useMemo(() => {
    // Create hourly buckets
    const now = new Date();
    const buckets = new Map<string, ChartDataPoint>();

    // Initialize all hourly buckets
    for (let i = hours - 1; i >= 0; i--) {
      const hourStart = startOfHour(subHours(now, i));
      const key = hourStart.toISOString();
      buckets.set(key, {
        hour: format(hourStart, 'HH:mm'),
        time: hourStart,
        authentication: 0,
        dataAccess: 0,
        administration: 0,
        security: 0,
        total: 0,
      });
    }

    // Populate buckets with event data
    events.forEach((event) => {
      const eventTime = parseISO(event.timestamp);
      const hourStart = startOfHour(eventTime);
      const key = hourStart.toISOString();

      const bucket = buckets.get(key);
      if (bucket) {
        const category = categorizeEvent(event.event_type);
        bucket[category]++;
        bucket.total++;
      }
    });

    return Array.from(buckets.values()).sort((a, b) => a.time.getTime() - b.time.getTime());
  }, [events, hours]);

  const maxValue = React.useMemo(() => {
    return Math.max(...chartData.map(d => d.total), 1);
  }, [chartData]);

  const hasData = events.length > 0;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-sm bg-entity/10">
            <Sparkles className="h-5 w-5 text-entity" />
          </div>
          <div>
            <CardTitle>Activity Trends</CardTitle>
            <CardDescription>
              {hasData
                ? `System activity over the last ${hours} hours`
                : 'No activity data available - audit logging is active'}
            </CardDescription>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <div className="h-80">
          {hasData ? (
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart
                data={chartData}
                margin={{ top: 10, right: 30, left: 0, bottom: 0 }}
              >
                <defs>
                  <linearGradient id="colorAuth" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="hsl(var(--entity))" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="hsl(var(--entity))" stopOpacity={0} />
                  </linearGradient>
                  <linearGradient id="colorData" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="hsl(var(--model))" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="hsl(var(--model))" stopOpacity={0} />
                  </linearGradient>
                  <linearGradient id="colorAdmin" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="hsl(var(--warning))" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="hsl(var(--warning))" stopOpacity={0} />
                  </linearGradient>
                  <linearGradient id="colorSecurity" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="hsl(var(--error))" stopOpacity={0.3} />
                    <stop offset="95%" stopColor="hsl(var(--error))" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <CartesianGrid
                  strokeDasharray="3 3"
                  stroke="hsl(var(--border))"
                  strokeOpacity={0.3}
                />
                <XAxis
                  dataKey="hour"
                  stroke="hsl(var(--muted-foreground))"
                  fontSize={12}
                  tickLine={false}
                  axisLine={false}
                  interval="preserveStartEnd"
                />
                <YAxis
                  stroke="hsl(var(--muted-foreground))"
                  fontSize={12}
                  tickLine={false}
                  axisLine={false}
                  allowDecimals={false}
                  domain={[0, maxValue + 2]}
                />
                <Tooltip
                  content={({ active, payload }) => {
                    if (!active || !payload?.length) return null;

                    return (
                      <div className="rounded-sm border border-border bg-background p-3 shadow-lg">
                        <p className="text-sm font-semibold text-foreground mb-2">
                          {payload[0].payload.hour}
                        </p>
                        <div className="space-y-1">
                          {payload.reverse().map((entry: any) => (
                            <div key={entry.name} className="flex items-center gap-2 text-xs">
                              <div
                                className="w-3 h-3 rounded-sm"
                                style={{ backgroundColor: entry.color }}
                              />
                              <span className="text-muted-foreground">{entry.name}:</span>
                              <span className="font-semibold text-foreground">{entry.value}</span>
                            </div>
                          ))}
                        </div>
                      </div>
                    );
                  }}
                />
                <Legend
                  wrapperStyle={{
                    paddingTop: '20px',
                  }}
                  iconType="circle"
                  formatter={(value) => (
                    <span className="text-sm text-foreground">{value}</span>
                  )}
                />
                <Area
                  type="monotone"
                  dataKey="authentication"
                  name="Authentication"
                  stackId="1"
                  stroke="hsl(var(--entity))"
                  fill="url(#colorAuth)"
                  strokeWidth={2}
                />
                <Area
                  type="monotone"
                  dataKey="dataAccess"
                  name="Data Access"
                  stackId="1"
                  stroke="hsl(var(--model))"
                  fill="url(#colorData)"
                  strokeWidth={2}
                />
                <Area
                  type="monotone"
                  dataKey="administration"
                  name="Administration"
                  stackId="1"
                  stroke="hsl(var(--warning))"
                  fill="url(#colorAdmin)"
                  strokeWidth={2}
                />
                <Area
                  type="monotone"
                  dataKey="security"
                  name="Security"
                  stackId="1"
                  stroke="hsl(var(--error))"
                  fill="url(#colorSecurity)"
                  strokeWidth={2}
                />
              </AreaChart>
            </ResponsiveContainer>
          ) : (
            <div className="h-full flex flex-col items-center justify-center border border-dashed border-border rounded-sm bg-background-secondary">
              <Sparkles className="h-12 w-12 text-muted-foreground mb-4 opacity-50" />
              <p className="text-sm text-muted-foreground font-semibold mb-1">
                No activity data yet
              </p>
              <p className="text-xs text-muted-foreground">
                Events will appear here as users interact with the system
              </p>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
