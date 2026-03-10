/**
 * Dashboard V2 - Enhanced Control Plane
 * Stunning, professional dashboard with modern UX patterns
 */

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
  Database,
  GitBranch,
  Sparkles,
  TrendingUp,
  FileText,
  AlertTriangle,
  Plus,
  Brain,
  FileCode,
  GitMerge,
  Activity,
} from 'lucide-react';
import { EnhancedStatCard } from '@/components/dashboard/EnhancedStatCard';
import { LiveActivityFeed, ActivityEvent } from '@/components/dashboard/LiveActivityFeed';
import { QuickActionsPanel } from '@/components/dashboard/QuickActionsPanel';
import { SystemHealthWidget } from '@/components/dashboard/SystemHealthWidget';
import { ActivityChart } from '@/components/dashboard/ActivityChart';
import { RegisterModelWizard } from '@/components/models/RegisterModelWizard';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useHealth } from '@/hooks/useHealth';
import { useRdfStoreStats } from '@/hooks/useGovernance';
import { useTemporalStats, useCacheStats, useRecentAuditLogs, useAuditActivityHistory } from '@/hooks/useAdmin';
import { formatDistanceToNow } from 'date-fns';

// Generate sparkline data (24 hours of mock trend data)
const generateSparklineData = (baseValue: number, variance: number = 0.1) => {
  return Array.from({ length: 24 }, (_, i) => {
    const trend = Math.sin(i / 4) * variance;
    const random = (Math.random() - 0.5) * variance;
    return Math.max(0, baseValue * (1 + trend + random));
  });
};

export function DashboardV2() {
  const navigate = useNavigate();
  const [wizardOpen, setWizardOpen] = useState(false);

  // Fetch real-time data from backend
  const { data: healthData } = useHealth();
  const { data: rdfStats } = useRdfStoreStats();
  const { data: temporalStats } = useTemporalStats();
  const { data: cacheStats } = useCacheStats();
  const { data: auditData } = useRecentAuditLogs(20);
  const { data: auditHistory } = useAuditActivityHistory(24);

  // Transform audit events to activity feed format
  const activities: ActivityEvent[] = (auditData?.events || []).map((event: any) => ({
    id: event.id,
    type: event.event_type?.includes('login') ? 'system' :
          event.event_type?.includes('workflow') ? 'workflow' :
          event.event_type?.includes('datasource') ? 'datasource' : 'ontology',
    message: event.event_type === 'login_success' ? `${event.username} logged in successfully` :
             event.event_type === 'login_failure' ? `Failed login attempt for ${event.username}` :
             event.event_type === 'user_created' ? `New user created: ${event.metadata?.new_username}` :
             `${event.action} on ${event.resource || 'system'}`,
    timestamp: new Date(event.timestamp),
    status: event.event_type?.includes('failure') || event.event_type?.includes('denied') ? 'error' :
            event.event_type?.includes('warning') ? 'warning' : 'success',
    metadata: event.metadata,
  }));

  // Enhanced stats with sparklines
  const triplesCount = rdfStats?.total_triples ?? 0;
  const workflowsCount = temporalStats?.temporal_chains ?? 42; // Use temporal chains or fallback to mock
  const datasourcesCount = 8; // Mock data
  const qualityScore = 94; // Mock quality score

  const stats = [
    {
      id: 'system-status',
      title: 'System Status',
      value: healthData?.status === 'alive' ? 'Healthy' : 'Degraded',
      icon: TrendingUp,
      status: healthData?.status === 'alive' ? 'success' as const : 'warning' as const,
      sparklineData: generateSparklineData(100, 0.05),
      trend: { value: 2.5, isPositive: true },
      action: {
        label: 'View Details',
        onClick: () => navigate('/admin'),
      },
    },
    {
      id: 'rdf-store',
      title: 'RDF Store',
      value: triplesCount,
      icon: Database,
      status: 'info' as const,
      sparklineData: generateSparklineData(triplesCount, 0.15),
      trend: { value: 12.3, isPositive: true },
      action: {
        label: 'Browse Triples',
        onClick: () => navigate('/sparql'),
      },
    },
    {
      id: 'workflows',
      title: 'Workflows',
      value: workflowsCount,
      icon: GitBranch,
      status: 'info' as const,
      sparklineData: generateSparklineData(workflowsCount, 0.2),
      trend: { value: 8.1, isPositive: true },
      action: {
        label: 'View Workflows',
        onClick: () => navigate('/workflows'),
      },
    },
    {
      id: 'datasources',
      title: 'Data Sources',
      value: datasourcesCount,
      icon: FileText,
      status: 'info' as const,
      sparklineData: generateSparklineData(datasourcesCount, 0.1),
      trend: { value: 3.2, isPositive: false },
      action: {
        label: 'Manage Sources',
        onClick: () => navigate('/datasources'),
      },
    },
    {
      id: 'quality',
      title: 'Data Quality',
      value: `${qualityScore}%`,
      icon: AlertTriangle,
      status: qualityScore >= 90 ? 'success' as const : 'warning' as const,
      sparklineData: generateSparklineData(qualityScore, 0.03),
      trend: { value: 1.5, isPositive: true },
      action: {
        label: 'Quality Report',
        onClick: () => navigate('/quality'),
      },
    },
  ];

  // System health components
  const systemComponents = [
    { name: 'RDF Store', status: rdfStats ? 'healthy' as const : 'down' as const },
    { name: 'Cache Layer', status: cacheStats ? 'healthy' as const : 'degraded' as const },
    { name: 'Temporal Store', status: temporalStats ? 'healthy' as const : 'degraded' as const },
    { name: 'API Gateway', status: healthData?.status === 'alive' ? 'healthy' as const : 'down' as const },
  ];

  const overallHealth = Math.round(
    (systemComponents.filter((c) => c.status === 'healthy').length / systemComponents.length) * 100
  );

  // Quick actions
  const quickActions = [
    {
      id: 'new-workflow',
      label: 'New Workflow',
      icon: Plus,
      onClick: () => navigate('/workflows/new'),
      shortcut: '⌘E',
    },
    {
      id: 'register-model',
      label: 'Register Model',
      icon: Brain,
      onClick: () => setWizardOpen(true),
      shortcut: '⌘M',
    },
    {
      id: 'run-sparql',
      label: 'Run SPARQL',
      icon: FileCode,
      onClick: () => navigate('/sparql'),
      shortcut: '⌘Q',
    },
    {
      id: 'start-fusion',
      label: 'Start Fusion',
      icon: GitMerge,
      onClick: () => navigate('/fusion-new'),
      shortcut: '⌘F',
    },
    {
      id: 'add-datasource',
      label: 'Add Data Source',
      icon: Database,
      onClick: () => navigate('/datasources'),
      shortcut: '⌘D',
    },
    {
      id: 'view-activity',
      label: 'View Activity',
      icon: Activity,
      onClick: () => navigate('/admin'),
      shortcut: '⌘A',
    },
  ];

  // Recent items (mock data for now)
  const recentItems = [
    { id: '1', label: 'Customer Pipeline', type: 'workflow', onClick: () => navigate('/workflows/1') },
    { id: '2', label: 'Product Ontology', type: 'ontology', onClick: () => navigate('/ontologies/2') },
    { id: '3', label: 'Sales Database', type: 'datasource', onClick: () => navigate('/datasources/3') },
  ];

  return (
    <div className="space-y-6 pb-8">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3 }}
        className="flex items-start justify-between pb-6 border-b border-border"
      >
        <div>
          <h1 className="text-3xl font-bold bg-gradient-to-r from-primary to-primary/60 bg-clip-text text-transparent">
            Control Plane
          </h1>
          <p className="text-sm text-muted-foreground mt-1">
            Real-time monitoring and management for your data governance platform
          </p>
        </div>
      </motion.div>

      {/* Enhanced Stats Grid */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.4, delay: 0.1 }}
        className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-5 gap-4"
      >
        {stats.map((stat, index) => (
          <motion.div
            key={stat.id}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3, delay: 0.1 + index * 0.05 }}
          >
            <EnhancedStatCard {...stat} className="group" />
          </motion.div>
        ))}
      </motion.div>

      {/* Main Content - 70/30 Split */}
      <div className="grid grid-cols-1 lg:grid-cols-10 gap-6">
        {/* Left Column - Activity Feed + Chart */}
        <div className="lg:col-span-7 space-y-6">
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, delay: 0.3 }}
          >
            <LiveActivityFeed events={activities} showLiveIndicator maxEvents={10} />
          </motion.div>

          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.4, delay: 0.4 }}
          >
            <ActivityChart events={auditHistory?.events} hours={24} />
          </motion.div>
        </div>

        {/* Right Column - Quick Actions + System Health */}
        <div className="lg:col-span-3 space-y-6">
          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, delay: 0.3 }}
          >
            <QuickActionsPanel actions={quickActions} recentItems={recentItems} />
          </motion.div>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, delay: 0.4 }}
          >
            <SystemHealthWidget
              overallHealth={overallHealth}
              components={systemComponents}
              onDiagnose={() => navigate('/admin')}
              onRefresh={() => window.location.reload()}
            />
          </motion.div>
        </div>
      </div>

      {/* Model Registration Wizard */}
      <RegisterModelWizard open={wizardOpen} onOpenChange={setWizardOpen} />
    </div>
  );
}
