import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { StatCard } from '@/components/dashboard/StatCard';
import { ActivityFeed } from '@/components/dashboard/ActivityFeed';
import { SystemHealth } from '@/components/dashboard/SystemHealth';
import { ActivityChart } from '@/components/dashboard/ActivityChart';
import { RegisterModelWizard } from '@/components/models/RegisterModelWizard';
import {
  Database,
  GitBranch,
  Brain,
  TrendingUp,
  Plus,
  FileCode,
  GitMerge,
  Sparkles,
  AlertCircle
} from 'lucide-react';
import { motion } from 'framer-motion';
import { useHealth } from '@/hooks/useHealth';
import { useRdfStoreStats } from '@/hooks/useGovernance';
import { useTemporalStats, useCacheStats, useRecentAuditLogs, useAuditActivityHistory } from '@/hooks/useAdmin';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { formatDistanceToNow } from 'date-fns';

const quickActions = [
  { label: 'Create Entity', icon: Plus, variant: 'default' as const, color: 'bg-entity' },
  { label: 'Register Model', icon: Brain, variant: 'outline' as const },
  { label: 'Run SPARQL', icon: FileCode, variant: 'outline' as const },
  { label: 'Start Fusion', icon: GitMerge, variant: 'outline' as const },
];

// Map audit event types to activity types
const auditEventToActivityType = (eventType: string): 'entity_created' | 'model_deployed' | 'fusion_completed' | 'quality_alert' | 'entity_updated' => {
  if (eventType === 'login_success' || eventType === 'login_failure') return 'entity_updated';
  if (eventType === 'user_created') return 'entity_created';
  if (eventType === 'access_denied') return 'quality_alert';
  return 'entity_updated';
};

// Generate activity message from audit event
const auditEventToMessage = (event: any): string => {
  const username = event.username || 'Unknown user';

  switch (event.event_type) {
    case 'login_success':
      return `${username} logged in successfully`;
    case 'login_failure':
      return `Failed login attempt for ${username}`;
    case 'user_created':
      return `New user created: ${event.metadata?.new_username || 'unknown'}`;
    case 'access_denied':
      return `Access denied for ${username} on ${event.resource || 'resource'}`;
    case 'logout':
      return `${username} logged out`;
    default:
      return `${event.action} on ${event.resource || 'system'}`;
  }
};

export function Dashboard() {
  const navigate = useNavigate();
  const [wizardOpen, setWizardOpen] = useState(false);

  // Fetch real-time data from backend
  const { data: healthData, isLoading: healthLoading, error: healthError } = useHealth();
  const { data: rdfStats, isLoading: rdfLoading, error: rdfError } = useRdfStoreStats();
  const { data: temporalStats, isLoading: temporalLoading, error: temporalError } = useTemporalStats();
  const { data: cacheStats, isLoading: cacheLoading, error: cacheError } = useCacheStats();
  const { data: auditData } = useRecentAuditLogs(10);
  const { data: auditHistory } = useAuditActivityHistory(24);

  const isLoading = healthLoading || rdfLoading || temporalLoading || cacheLoading;
  const criticalError = healthError; // Only show error if health check fails
  const hasSomeData = healthData || rdfStats || temporalStats || cacheStats;

  // Transform audit events to activities
  const activities = React.useMemo(() => {
    if (!auditData?.events || auditData.events.length === 0) {
      // Return placeholder message when no audit events available
      return [
        {
          id: 'placeholder-1',
          type: 'entity_updated' as const,
          message: 'No recent activity - audit log storage is configured but query not yet implemented',
          time: 'Just now',
          metadata: {},
        },
      ];
    }

    return auditData.events.map((event) => ({
      id: event.id,
      type: auditEventToActivityType(event.event_type),
      message: auditEventToMessage(event),
      time: formatDistanceToNow(new Date(event.timestamp), { addSuffix: true }),
      metadata: event.metadata,
    }));
  }, [auditData]);

  // Build stats from real data
  const stats = [
    {
      title: 'RDF Triples',
      value: rdfStats?.total_triples ?? 0,
      icon: Database,
      color: 'entity' as const,
      trend: undefined
    },
    {
      title: 'Store Type',
      value: rdfStats?.store_type ?? 'N/A',
      icon: GitBranch,
      color: 'model' as const,
      trend: undefined
    },
    {
      title: 'Materialization',
      value: rdfStats?.materialization_enabled ? 'Enabled' : 'Disabled',
      icon: Sparkles,
      color: 'success' as const,
      trend: undefined
    },
    {
      title: 'System Status',
      value: healthData?.status === 'alive' ? 'Healthy' : (healthData?.status ?? 'Unknown'),
      icon: TrendingUp,
      color: healthData?.status === 'alive' ? 'success' as const : 'warning' as const,
      trend: undefined
    },
  ];

  // Build health metrics from real data
  const healthMetrics = [
    {
      label: 'RDF Store',
      value: rdfStats ? 100 : 0,
      unit: rdfStats?.store_type,
      status: rdfStats ? 'healthy' as const : 'critical' as const
    },
    {
      label: 'Materialization',
      value: rdfStats?.materialization_enabled ? 100 : 0,
      status: rdfStats?.materialization_enabled ? 'healthy' as const : 'warning' as const
    },
    {
      label: 'Model Cache',
      value: cacheError ? 0 : 100,
      status: cacheError ? 'critical' as const : 'healthy' as const
    },
    {
      label: 'Temporal Store',
      value: temporalError ? 0 : 100,
      status: temporalError ? 'critical' as const : 'healthy' as const
    },
  ];

  const handleQuickAction = (label: string) => {
    switch (label) {
      case 'Create Entity':
        navigate('/entities');
        break;
      case 'Register Model':
        setWizardOpen(true);
        break;
      case 'Run SPARQL':
        navigate('/sparql');
        break;
      case 'Start Fusion':
        navigate('/fusion-new');
        break;
      default:
        console.warn(`Unhandled quick action: ${label}`);
    }
  };

  return (
    <div className="space-y-5">
      {/* Oracle Redwood Page Header - Toolbar pattern */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15 }}
        className="flex items-start justify-between pb-4 border-b-2 border-border"
      >
        <div className="min-w-0 flex-1">
          <h1 className="text-2xl font-semibold text-foreground mb-1">
            Dashboard
          </h1>
          <p className="text-sm text-muted-foreground">
            Monitor your RDF data governance platform in real-time
          </p>
        </div>

        <div className="flex flex-wrap gap-2 ml-4">
          {quickActions.map((action, index) => (
            <Button
              key={action.label}
              variant={action.variant}
              size="default"
              className="gap-2"
              onClick={() => handleQuickAction(action.label)}
            >
              <action.icon className="h-4 w-4" />
              {action.label}
            </Button>
          ))}
        </div>
      </motion.div>

      {/* Error Alert - only show if critical health check fails */}
      {criticalError && !hasSomeData && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            Unable to connect to the backend. Please check if the server is running.
          </AlertDescription>
        </Alert>
      )}

      {/* Stats Grid - Oracle 4 columns */}
      <div className="grid grid-cols-4 gap-4">
        {stats.map((stat, index) => (
          <StatCard
            key={stat.title}
            {...stat}
            delay={index * 0.03}
          />
        ))}
      </div>

      {/* Main Content Grid - 2/3 + 1/3 layout */}
      <div className="grid grid-cols-3 gap-4">
        <div className="col-span-2">
          <ActivityFeed activities={activities} />
        </div>
        <div className="col-span-1">
          <SystemHealth
            metrics={healthMetrics}
            overallStatus={healthData?.status === 'healthy' ? 'online' : healthData?.status === 'degraded' ? 'degraded' : 'offline'}
          />
        </div>
      </div>

      {/* Activity Trends Chart */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15, delay: 0.2 }}
      >
        <ActivityChart events={auditHistory?.events} hours={24} />
      </motion.div>

      {/* Model Registration Wizard */}
      <RegisterModelWizard open={wizardOpen} onOpenChange={setWizardOpen} />
    </div>
  );
}