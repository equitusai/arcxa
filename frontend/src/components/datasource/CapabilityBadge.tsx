/**
 * Capability Badge Component
 *
 * Enhanced badge with tooltip for datasource capabilities.
 * Can be used to replace inline capability rendering in DatasourceTypeSelector.
 *
 * Future enhancement: Add to main selector when Tooltip component is available.
 */

import React from 'react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import {
  Activity,
  Database,
  Search,
  GitBranch,
  Zap,
  Shield,
  LucideIcon,
} from 'lucide-react';

interface CapabilityBadgeProps {
  capability: 'cdc' | 'batch_read' | 'batch_write' | 'profiling' | 'lineage_discovery' | 'schema_evolution' | 'transactions';
  showLabel?: boolean;
  size?: 'sm' | 'md';
}

const CAPABILITY_CONFIG: Record<
  CapabilityBadgeProps['capability'],
  {
    icon: LucideIcon;
    label: string;
    description: string;
    color: string;
    bgColor: string;
  }
> = {
  cdc: {
    icon: Activity,
    label: 'CDC',
    description: 'Change Data Capture – Real-time streaming of data changes',
    color: 'text-blue-600',
    bgColor: 'bg-blue-50',
  },
  batch_read: {
    icon: Database,
    label: 'Batch Read',
    description: 'Large-scale data extraction for analytics and reporting',
    color: 'text-green-600',
    bgColor: 'bg-green-50',
  },
  batch_write: {
    icon: Database,
    label: 'Batch Write',
    description: 'Bulk data loading and import operations',
    color: 'text-green-600',
    bgColor: 'bg-green-50',
  },
  profiling: {
    icon: Search,
    label: 'Profiling',
    description: 'Automatic data quality and schema discovery',
    color: 'text-purple-600',
    bgColor: 'bg-purple-50',
  },
  lineage_discovery: {
    icon: GitBranch,
    label: 'Lineage',
    description: 'Automatic discovery of data lineage and provenance',
    color: 'text-orange-600',
    bgColor: 'bg-orange-50',
  },
  schema_evolution: {
    icon: Zap,
    label: 'Schema Evolution',
    description: 'Handles schema changes without manual intervention',
    color: 'text-yellow-600',
    bgColor: 'bg-yellow-50',
  },
  transactions: {
    icon: Shield,
    label: 'Transactions',
    description: 'ACID-compliant transactional guarantees',
    color: 'text-red-600',
    bgColor: 'bg-red-50',
  },
};

export function CapabilityBadge({ capability, showLabel = true, size = 'sm' }: CapabilityBadgeProps) {
  const config = CAPABILITY_CONFIG[capability];
  const Icon = config.icon;

  const sizeClasses = {
    sm: {
      container: 'px-2 py-1 text-xs',
      icon: 'h-3 w-3',
    },
    md: {
      container: 'px-3 py-1.5 text-sm',
      icon: 'h-4 w-4',
    },
  };

  return (
    <TooltipProvider delayDuration={200}>
      <Tooltip>
        <TooltipTrigger asChild>
          <div
            className={`
              inline-flex items-center gap-1.5 rounded-sm
              ${sizeClasses[size].container}
              ${config.bgColor} ${config.color}
              font-medium cursor-help transition-all
              hover:shadow-sm
            `}
          >
            <Icon className={`${sizeClasses[size].icon} flex-shrink-0`} />
            {showLabel && <span className="truncate">{config.label}</span>}
          </div>
        </TooltipTrigger>
        <TooltipContent side="top" className="max-w-xs">
          <div className="space-y-1">
            <p className="font-semibold text-sm">{config.label}</p>
            <p className="text-xs text-muted-foreground">{config.description}</p>
          </div>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

/**
 * Example Usage in DatasourceTypeSelector:
 *
 * Replace this:
 *   <div className="flex items-center gap-1.5 px-2 py-1 rounded bg-neutral-100/80 text-xs">
 *     <Icon className="h-3 w-3" />
 *     <span>{label}</span>
 *   </div>
 *
 * With this:
 *   <CapabilityBadge capability="cdc" showLabel={true} size="sm" />
 */
