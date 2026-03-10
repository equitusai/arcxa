/**
 * Cluster Overview Tab - Professional Redesign
 *
 * Shows cluster health, key metrics, and configuration summary
 * Features: skeleton states, smooth transitions, optimistic updates, zero jarring refreshes
 */

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { CheckCircle, AlertCircle, XCircle, Database, Activity } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { cn } from '@/lib/utils';
import { useClusterHealth, useClusterStats, useClusterConfig, useClusterTopology } from '@/hooks/useCluster';
import { formatDistanceToNow } from 'date-fns';
import { RefreshIndicator } from './RefreshIndicator';
import { CountUpNumber } from './CountUpNumber';
import {
  ClusterHealthSkeleton,
  MetricsSkeleton,
  ShardListSkeleton,
  ConfigSkeleton,
} from './ClusterSkeletons';

interface ClusterOverviewProps {
  onNavigateToTopology?: () => void;
}

export function ClusterOverview({ onNavigateToTopology }: ClusterOverviewProps) {
  // Use improved React Query hooks with optimistic updates
  const {
    data: health,
    isLoading: healthLoading,
    isFetching: healthFetching,
    dataUpdatedAt: healthUpdatedAt,
  } = useClusterHealth();

  const {
    data: stats,
    isLoading: statsLoading,
    isFetching: statsFetching,
  } = useClusterStats();

  const { data: config, isLoading: configLoading } = useClusterConfig();
  const { data: topology, isLoading: topologyLoading } = useClusterTopology();

  const isInitialLoading = healthLoading || statsLoading;
  const isSingleNode = config?.mode === 'single-node';

  // Determine status icon and colors
  const getStatusConfig = () => {
    if (!health) return { icon: AlertCircle, color: 'text-muted-foreground', bg: 'bg-muted', label: 'Unknown' };

    switch (health.status) {
      case 'healthy':
        return { icon: CheckCircle, color: 'text-success', bg: 'bg-success/10', label: 'Healthy' };
      case 'degraded':
        return { icon: AlertCircle, color: 'text-warning', bg: 'bg-warning/10', label: 'Degraded' };
      case 'critical':
        return { icon: XCircle, color: 'text-error', bg: 'bg-error/10', label: 'Critical' };
      default:
        return { icon: AlertCircle, color: 'text-muted-foreground', bg: 'bg-muted', label: 'Unknown' };
    }
  };

  const statusConfig = getStatusConfig();
  const StatusIcon = statusConfig.icon;

  // Format uptime
  const formatUptime = (seconds: number) => {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    return `${days}d ${hours}h`;
  };

  // Format bytes to GB
  const formatBytes = (bytes: number) => {
    return (bytes / 1_073_741_824).toFixed(2);
  };

  // Format large numbers
  const formatNumber = (num: number) => {
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
    return num.toString();
  };

  // Show skeleton on initial load only
  if (isInitialLoading) {
    return (
      <div className="space-y-4">
        <ClusterHealthSkeleton />
        <MetricsSkeleton />
        <ShardListSkeleton />
        <ConfigSkeleton />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Cluster Health Status */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2, ease: 'easeOut' }}
      >
        <Card className="glass-morphism border-border relative">
          {/* Show refresh indicator during background refresh */}
          <AnimatePresence>
            {healthFetching && !healthLoading && <RefreshIndicator />}
          </AnimatePresence>

          <CardContent className="p-6">
            <div className="flex items-start justify-between mb-3">
              <div className="flex items-center gap-3">
                <motion.div
                  className={cn('p-2 rounded-lg', statusConfig.bg)}
                  initial={{ scale: 0.8, opacity: 0 }}
                  animate={{ scale: 1, opacity: 1 }}
                  transition={{ duration: 0.2 }}
                >
                  <StatusIcon className={cn('h-6 w-6', statusConfig.color)} />
                </motion.div>
                <div>
                  <motion.h3
                    className="text-lg font-semibold"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ duration: 0.3, delay: 0.1 }}
                  >
                    {statusConfig.label}
                  </motion.h3>
                  <motion.p
                    className="text-sm text-muted-foreground"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ duration: 0.3, delay: 0.15 }}
                  >
                    <CountUpNumber value={health?.healthy_shards ?? 0} />/
                    <CountUpNumber value={health?.total_shards ?? 0} /> shards operational
                    {health && health.uptime_seconds && ` • Uptime: ${formatUptime(health.uptime_seconds)}`}
                  </motion.p>
                </div>
              </div>
              <Badge variant={statusConfig.label === 'Healthy' ? 'success' : statusConfig.label === 'Degraded' ? 'warning' : 'destructive'}>
                {config?.mode === 'single-node' && health?.total_shards === 1
                  ? 'Single-Node'
                  : `${health?.total_shards ?? topology?.total_shards ?? 0} Shards`}
              </Badge>
            </div>

            {/* Last updated timestamp */}
            {healthUpdatedAt && (
              <motion.p
                className="text-xs text-muted-foreground"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ duration: 0.3, delay: 0.2 }}
              >
                Last updated: {formatDistanceToNow(new Date(healthUpdatedAt), { addSuffix: true })}
              </motion.p>
            )}

            {/* Show issues if any */}
            <AnimatePresence mode="wait">
              {health && health.issues.length > 0 && (
                <motion.div
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: 'auto' }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.2 }}
                  className="mt-4 p-3 bg-warning/5 border border-warning/20 rounded-md overflow-hidden"
                >
                  <p className="text-sm font-medium text-warning mb-2">Active Issues ({health.issues.length})</p>
                  <ul className="space-y-1">
                    {health.issues.slice(0, 3).map((issue, idx) => (
                      <motion.li
                        key={idx}
                        initial={{ opacity: 0, x: -8 }}
                        animate={{ opacity: 1, x: 0 }}
                        transition={{ duration: 0.2, delay: idx * 0.05 }}
                        className="text-xs text-muted-foreground"
                      >
                        • {issue.message}
                      </motion.li>
                    ))}
                  </ul>
                </motion.div>
              )}
            </AnimatePresence>
          </CardContent>
        </Card>
      </motion.div>

      {/* Metrics Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Performance Metrics */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: 'easeOut', delay: 0.05 }}
        >
          <Card className="glass-morphism border-border relative">
            <AnimatePresence>
              {statsFetching && !statsLoading && <RefreshIndicator />}
            </AnimatePresence>

            <CardHeader className="pb-3">
              <div className="flex items-center gap-2">
                <Activity className="h-4 w-4 text-info" />
                <CardTitle className="text-sm">Performance Metrics</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Queries/sec</span>
                <motion.span
                  className="text-lg font-semibold"
                  key={`qps-${stats?.queries_per_second}`}
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  {stats?.queries_per_second !== undefined ? (
                    <CountUpNumber value={stats.queries_per_second} decimals={1} />
                  ) : (
                    '0.0'
                  )}
                </motion.span>
              </div>
              <Separator className="bg-border" />
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Writes/sec</span>
                <motion.span
                  className="text-lg font-semibold"
                  key={`wps-${stats?.writes_per_second}`}
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  {stats?.writes_per_second !== undefined ? (
                    <CountUpNumber value={stats.writes_per_second} decimals={1} />
                  ) : (
                    '0.0'
                  )}
                </motion.span>
              </div>
              <Separator className="bg-border" />
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">P99 Latency</span>
                <motion.span
                  className="text-lg font-semibold"
                  key={`p99-${stats?.p99_query_latency_ms}`}
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  {stats?.p99_query_latency_ms !== undefined ? (
                    <>
                      <CountUpNumber value={stats.p99_query_latency_ms} decimals={0} />ms
                    </>
                  ) : (
                    '0ms'
                  )}
                </motion.span>
              </div>
            </CardContent>
          </Card>
        </motion.div>

        {/* Data Overview */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: 'easeOut', delay: 0.1 }}
        >
          <Card className="glass-morphism border-border relative">
            <AnimatePresence>
              {statsFetching && !statsLoading && <RefreshIndicator />}
            </AnimatePresence>

            <CardHeader className="pb-3">
              <div className="flex items-center gap-2">
                <Database className="h-4 w-4 text-entity" />
                <CardTitle className="text-sm">Data Overview</CardTitle>
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Total Triples</span>
                <motion.span
                  className="text-lg font-semibold"
                  key={`triples-${stats?.total_triples}`}
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  {formatNumber(stats?.total_triples || 0)}
                </motion.span>
              </div>
              <Separator className="bg-border" />
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Database Size</span>
                <motion.span
                  className="text-lg font-semibold"
                  key={`size-${stats?.total_size_gb}`}
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  {stats?.total_size_gb !== undefined ? (
                    <>
                      <CountUpNumber value={stats.total_size_gb} decimals={2} /> GB
                    </>
                  ) : (
                    '0.00 GB'
                  )}
                </motion.span>
              </div>
              <Separator className="bg-border" />
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Avg Utilization</span>
                <motion.span
                  className="text-lg font-semibold"
                  key={`util-${stats?.average_shard_utilization}`}
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  {stats?.average_shard_utilization !== undefined ? (
                    <>
                      <CountUpNumber value={stats.average_shard_utilization * 100} decimals={0} />%
                    </>
                  ) : (
                    '0%'
                  )}
                </motion.span>
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>

      {/* Shard Health Summary */}
      {topology && topology.shards && topology.shards.length > 0 && (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: 'easeOut', delay: 0.15 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm">Shard Health Summary</CardTitle>
                <Button variant="ghost" size="sm" onClick={onNavigateToTopology}>
                  View all {topology.shards.length} shards →
                </Button>
              </div>
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                <AnimatePresence mode="popLayout">
                  {topology.shards.slice(0, 5).map((shard, index) => (
                    <motion.div
                      key={shard.shard_id}
                      initial={{ opacity: 0, x: -8 }}
                      animate={{ opacity: 1, x: 0 }}
                      exit={{ opacity: 0, x: 8 }}
                      transition={{ duration: 0.2, delay: index * 0.03 }}
                      className="flex items-center justify-between p-2 rounded border border-border hover:bg-background-secondary/50 transition-colors"
                    >
                      <div className="flex items-center gap-3">
                        <span className="text-sm font-mono font-medium">Shard {shard.shard_id}</span>
                        <Badge variant="outline" className="text-xs">
                          {shard.status}
                        </Badge>
                      </div>
                      <div className="flex items-center gap-4 text-xs text-muted-foreground">
                        <span>{formatNumber(shard.triple_count)} triples</span>
                        <span>{formatBytes(shard.size_bytes)} GB</span>
                      </div>
                    </motion.div>
                  ))}
                </AnimatePresence>
              </div>
            </CardContent>
          </Card>
        </motion.div>
      )}

      {/* Configuration Summary */}
      {configLoading ? (
        <ConfigSkeleton />
      ) : (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.2, ease: 'easeOut', delay: 0.2 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <CardTitle className="text-sm">Configuration Summary</CardTitle>
              <CardDescription>Current cluster settings</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Cluster Mode</span>
                <Badge variant="outline">{config?.mode || 'Unknown'}</Badge>
              </div>
              <Separator className="bg-border" />
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Auto-save Interval</span>
                <span className="text-sm font-medium">
                  {config?.data_retention.auto_save_interval_seconds || 300}s
                </span>
              </div>
              <Separator className="bg-border" />
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">Backup Enabled</span>
                <Badge variant={config?.data_retention.backup_enabled ? 'success' : 'outline'}>
                  {config?.data_retention.backup_enabled ? 'Yes' : 'No'}
                </Badge>
              </div>
              {!isSingleNode && config?.auto_scaling && (
                <>
                  <Separator className="bg-border" />
                  <div className="flex items-center justify-between">
                    <span className="text-sm text-muted-foreground">Auto-Scaling</span>
                    <Badge variant={config.auto_scaling.enabled ? 'success' : 'outline'}>
                      {config.auto_scaling.enabled ? 'Enabled' : 'Disabled'}
                    </Badge>
                  </div>
                </>
              )}
            </CardContent>
          </Card>
        </motion.div>
      )}
    </div>
  );
}
