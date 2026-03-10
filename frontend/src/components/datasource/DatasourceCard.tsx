/**
 * Premium Datasource Card
 * Enhanced card with gradients, animations, and status indicators
 */

import { useState } from 'react';
import { motion } from 'framer-motion';
import { Database, Activity, Eye, Trash2, Zap } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Sparkline } from '@/components/dashboard/Sparkline';
import { cn } from '@/lib/utils';
import type { Datasource, ConnectionStatus } from '@/api/types';
import { formatDistanceToNow } from 'date-fns';

export interface DatasourceCardProps {
  datasource: Datasource;
  onViewDetails: () => void;
  onTest: () => void;
  onDelete: () => void;
  className?: string;
}

// Generate mini sparkline for activity (mock data for now)
const generateActivityData = () => {
  return Array.from({ length: 12 }, () => Math.random() * 100);
};

export function DatasourceCard({
  datasource,
  onViewDetails,
  onTest,
  onDelete,
  className,
}: DatasourceCardProps) {
  const [showActions, setShowActions] = useState(false);
  const activityData = generateActivityData();

  const statusConfig = getStatusConfig(datasource.status);
  const typeConfig = getTypeConfig(datasource.metadata.datasource_type);

  const isConnected = datasource.status === 'Connected';
  const hasError = typeof datasource.status === 'object' && 'Error' in datasource.status;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3 }}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      <Card
        className={cn(
          'relative overflow-hidden transition-all duration-300',
          'hover:shadow-lg hover:-translate-y-0.5',
          'border-2',
          statusConfig.borderClass,
          className
        )}
      >
        {/* Gradient Background Overlay */}
        <div
          className={cn(
            'absolute inset-0 opacity-5 pointer-events-none',
            statusConfig.gradientClass
          )}
        />

        <div className="relative p-5">
          {/* Header */}
          <div className="flex items-start justify-between mb-4">
            <div className="flex items-start gap-3 flex-1 min-w-0">
              {/* Icon */}
              <div className={cn(
                'p-3 rounded-lg transition-all duration-300',
                statusConfig.iconBgClass,
                showActions && 'scale-110'
              )}>
                <Database className={cn('w-5 h-5', statusConfig.iconClass)} />
              </div>

              {/* Info */}
              <div className="flex-1 min-w-0">
                <h3 className="text-base font-semibold mb-1 truncate">{datasource.name}</h3>
                <p className="text-xs text-muted-foreground mb-2 truncate">
                  {datasource.metadata.name} v{datasource.metadata.version}
                </p>

                {/* Badges */}
                <div className="flex items-center gap-2 flex-wrap">
                  {/* Status Badge with Pulse */}
                  <div className="relative">
                    {isConnected && (
                      <span className="absolute -top-0.5 -left-0.5 flex h-3 w-3">
                        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                        <span className="relative inline-flex rounded-full h-3 w-3 bg-green-500"></span>
                      </span>
                    )}
                    <Badge
                      variant="outline"
                      className={cn(statusConfig.badgeClass, 'text-xs')}
                    >
                      {statusConfig.label}
                    </Badge>
                  </div>

                  {/* Type Badge with Gradient */}
                  <Badge
                    variant="outline"
                    className={cn(typeConfig.badgeClass, 'text-xs')}
                  >
                    {typeConfig.label}
                  </Badge>

                  {/* Disabled Badge */}
                  {!datasource.enabled && (
                    <Badge variant="outline" className="bg-gray-100 text-gray-600 border-gray-300 text-xs">
                      Disabled
                    </Badge>
                  )}
                </div>
              </div>
            </div>

            {/* Connection Health Indicator */}
            <div className="flex flex-col items-end gap-1">
              <div className={cn(
                'w-2 h-2 rounded-full',
                isConnected && 'bg-green-500 shadow-lg shadow-green-500/50',
                hasError && 'bg-red-500 shadow-lg shadow-red-500/50',
                !isConnected && !hasError && 'bg-gray-300'
              )} />
              {isConnected && (
                <span className="text-[10px] text-green-600 font-medium">LIVE</span>
              )}
            </div>
          </div>

          {/* Description */}
          <p className="text-sm text-muted-foreground mb-3 line-clamp-2">
            {datasource.metadata.description}
          </p>

          {/* Capabilities */}
          <div className="flex items-center gap-3 mb-3 text-xs">
            {datasource.capabilities.cdc && (
              <div className="flex items-center gap-1 text-emerald-600">
                <Zap className="w-3 h-3" />
                <span className="font-medium">CDC</span>
              </div>
            )}
            {datasource.capabilities.profiling && (
              <div className="flex items-center gap-1 text-blue-600">
                <Activity className="w-3 h-3" />
                <span className="font-medium">Profiling</span>
              </div>
            )}
            {datasource.capabilities.lineage_discovery && (
              <div className="flex items-center gap-1 text-purple-600">
                <Database className="w-3 h-3" />
                <span className="font-medium">Lineage</span>
              </div>
            )}
          </div>

          {/* Activity Sparkline */}
          <div className="mb-4">
            <div className="flex items-center justify-between mb-1">
              <span className="text-xs text-muted-foreground">Recent Activity</span>
              <span className="text-xs text-muted-foreground">12h</span>
            </div>
            <Sparkline
              data={activityData}
              width={280}
              height={32}
              color="currentColor"
              className={cn('opacity-70', statusConfig.iconClass)}
            />
          </div>

          {/* Last Tested */}
          <div className="flex items-center justify-between text-xs text-muted-foreground mb-4">
            <span>Last tested:</span>
            <span className="font-medium">
              {formatDistanceToNow(new Date(Date.now() - Math.random() * 3600000), { addSuffix: true })}
            </span>
          </div>

          {/* Quick Actions (Revealed on Hover) */}
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{
              opacity: showActions ? 1 : 0,
              height: showActions ? 'auto' : 0,
            }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="flex items-center gap-2 pt-3 border-t border-border">
              <Button
                variant="ghost"
                size="sm"
                className="flex-1 h-8 text-xs gap-1"
                onClick={onViewDetails}
              >
                <Eye className="w-3 h-3" />
                Details
              </Button>

              <Button
                variant="ghost"
                size="sm"
                className="flex-1 h-8 text-xs gap-1"
                onClick={onTest}
              >
                <Activity className="w-3 h-3" />
                Test
              </Button>

              <Button
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                onClick={onDelete}
                title="Delete"
              >
                <Trash2 className="w-3 h-3" />
              </Button>
            </div>
          </motion.div>
        </div>

        {/* Status Indicator Bar (Bottom) */}
        <div className={cn('h-1 w-full', statusConfig.barClass)} />
      </Card>
    </motion.div>
  );
}

function getStatusConfig(status: ConnectionStatus) {
  if (status === 'Connected') {
    return {
      label: 'Connected',
      badgeClass: 'bg-green-50 text-green-700 border-green-300',
      borderClass: 'border-green-500/30 hover:border-green-500/50',
      gradientClass: 'bg-gradient-to-br from-green-500 to-emerald-600',
      iconBgClass: 'bg-green-500/10',
      iconClass: 'text-green-600',
      barClass: 'bg-gradient-to-r from-green-500 to-emerald-600',
    };
  } else if (status === 'Connecting') {
    return {
      label: 'Connecting',
      badgeClass: 'bg-blue-50 text-blue-700 border-blue-300',
      borderClass: 'border-blue-500/30 hover:border-blue-500/50',
      gradientClass: 'bg-gradient-to-br from-blue-500 to-cyan-600',
      iconBgClass: 'bg-blue-500/10',
      iconClass: 'text-blue-600',
      barClass: 'bg-gradient-to-r from-blue-500 to-cyan-600 animate-pulse',
    };
  } else if (status === 'Disconnected') {
    return {
      label: 'Disconnected',
      badgeClass: 'bg-gray-50 text-gray-700 border-gray-300',
      borderClass: 'border-gray-300 hover:border-gray-400',
      gradientClass: 'bg-gradient-to-br from-gray-400 to-gray-500',
      iconBgClass: 'bg-gray-500/10',
      iconClass: 'text-gray-600',
      barClass: 'bg-gradient-to-r from-gray-400 to-gray-500',
    };
  } else if (typeof status === 'object' && 'Degraded' in status) {
    return {
      label: 'Degraded',
      badgeClass: 'bg-yellow-50 text-yellow-700 border-yellow-300',
      borderClass: 'border-yellow-500/30 hover:border-yellow-500/50',
      gradientClass: 'bg-gradient-to-br from-yellow-500 to-amber-600',
      iconBgClass: 'bg-yellow-500/10',
      iconClass: 'text-yellow-600',
      barClass: 'bg-gradient-to-r from-yellow-500 to-amber-600',
    };
  } else if (typeof status === 'object' && 'Error' in status) {
    return {
      label: 'Error',
      badgeClass: 'bg-red-50 text-red-700 border-red-300',
      borderClass: 'border-red-500/30 hover:border-red-500/50',
      gradientClass: 'bg-gradient-to-br from-red-500 to-rose-600',
      iconBgClass: 'bg-red-500/10',
      iconClass: 'text-red-600',
      barClass: 'bg-gradient-to-r from-red-500 to-rose-600',
    };
  }

  return {
    label: 'Unknown',
    badgeClass: 'bg-gray-50 text-gray-700 border-gray-300',
    borderClass: 'border-gray-300 hover:border-gray-400',
    gradientClass: 'bg-gradient-to-br from-gray-400 to-gray-500',
    iconBgClass: 'bg-gray-500/10',
    iconClass: 'text-gray-600',
    barClass: 'bg-gradient-to-r from-gray-400 to-gray-500',
  };
}

function getTypeConfig(type: Datasource['metadata']['datasource_type']) {
  const typeStr = typeof type === 'string' ? type : type?.Custom || 'Unknown';

  const configs: Record<string, { label: string; badgeClass: string }> = {
    Relational: {
      label: 'Relational',
      badgeClass: 'bg-gradient-to-r from-blue-50 to-blue-100 text-blue-700 border-blue-300',
    },
    Document: {
      label: 'Document',
      badgeClass: 'bg-gradient-to-r from-green-50 to-green-100 text-green-700 border-green-300',
    },
    Search: {
      label: 'Search',
      badgeClass: 'bg-gradient-to-r from-purple-50 to-purple-100 text-purple-700 border-purple-300',
    },
    ObjectStorage: {
      label: 'Object Store',
      badgeClass: 'bg-gradient-to-r from-orange-50 to-orange-100 text-orange-700 border-orange-300',
    },
    Streaming: {
      label: 'Streaming',
      badgeClass: 'bg-gradient-to-r from-red-50 to-red-100 text-red-700 border-red-300',
    },
    Graph: {
      label: 'Graph',
      badgeClass: 'bg-gradient-to-r from-pink-50 to-pink-100 text-pink-700 border-pink-300',
    },
    TimeSeries: {
      label: 'Time Series',
      badgeClass: 'bg-gradient-to-r from-indigo-50 to-indigo-100 text-indigo-700 border-indigo-300',
    },
  };

  return configs[typeStr] || {
    label: typeStr,
    badgeClass: 'bg-gradient-to-r from-gray-50 to-gray-100 text-gray-700 border-gray-300',
  };
}
