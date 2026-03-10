/**
 * Semantic Mapper Node Body
 * Displays ontology target, auto-approve threshold, and field mapping status
 */

import React from 'react';
import { Layers, Settings } from 'lucide-react';
import { StatusPills, InlineBadgeToggle } from '../widgets';
import type { StatusPill } from '../widgets';
import type { SemanticMapperConfig } from '@/lib/workflow-etl-config';

export interface SemanticMapperNodeBodyProps {
  config?: SemanticMapperConfig;
  status?: 'idle' | 'running' | 'success' | 'error';
  progress?: number;
  metrics?: {
    rowsProcessed?: number;
    duration?: number;
    size?: number;
  };
  error?: {
    message: string;
    details?: string;
  };
  onOntologyClick?: () => void;
  onThresholdClick?: () => void;
  onReviewPending?: () => void;
  onStatusPillClick?: (status: 'approved' | 'pending' | 'rejected') => void;
}

export function SemanticMapperNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onOntologyClick,
  onThresholdClick,
  onReviewPending,
  onStatusPillClick,
}: SemanticMapperNodeBodyProps) {
  // Calculate mapping counts
  const mappingCounts = config?.field_mappings?.reduce(
    (acc, mapping) => {
      if (mapping.confidence >= (config.auto_approve_threshold ?? 0.9)) {
        acc.approved++;
      } else if (mapping.confidence >= 0.5) {
        acc.pending++;
      } else {
        acc.rejected++;
      }
      return acc;
    },
    { approved: 0, pending: 0, rejected: 0 }
  ) || { approved: 0, pending: 0, rejected: 0 };

  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Mapping fields...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-green-500 to-green-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
          {metrics?.rowsProcessed && (
            <div className="text-xs text-muted-foreground mt-1">
              {metrics.rowsProcessed.toLocaleString()} fields analyzed
            </div>
          )}
        </div>
      </div>
    );
  }

  // Success state
  if (status === 'success' && config) {
    const totalMappings = config.field_mappings?.length || 0;
    const hasPending = mappingCounts.pending > 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Ontology target */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Layers className="w-3 h-3" />
            <span className="font-medium">Target Ontology</span>
          </div>
          <button
            onClick={onOntologyClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.target_ontology && config.target_ontology.length > 0 ? config.target_ontology.join(', ') : 'Not configured'}
          </button>
        </div>

        {/* Mapping status pills */}
        {totalMappings > 0 && (
          <div>
            <div className="text-xs font-medium text-foreground mb-1.5">
              Mapping Status ({totalMappings} fields)
            </div>
            <StatusPills
              pills={([
                {
                  label: 'Approved',
                  count: mappingCounts.approved,
                  color: 'success' as const,
                  icon: 'check' as const,
                  onClick: onStatusPillClick ? () => onStatusPillClick('approved') : undefined,
                },
                {
                  label: 'Pending',
                  count: mappingCounts.pending,
                  color: 'warning' as const,
                  icon: 'clock' as const,
                  onClick: onStatusPillClick ? () => onStatusPillClick('pending') : undefined,
                },
                {
                  label: 'Rejected',
                  count: mappingCounts.rejected,
                  color: 'danger' as const,
                  icon: 'x' as const,
                  onClick: onStatusPillClick ? () => onStatusPillClick('rejected') : undefined,
                },
              ] as StatusPill[]).filter(pill => pill.count > 0)}
            />
          </div>
        )}

        {/* Auto-approve threshold */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Settings className="w-3 h-3" />
            <span className="font-medium">Auto-approve threshold</span>
          </div>
          <button
            onClick={onThresholdClick}
            className="text-xs text-foreground hover:text-blue-600 transition-colors"
          >
            {((config.auto_approve_threshold ?? 0.9) * 100).toFixed(0)}% confidence
          </button>
        </div>

        {/* Review pending action */}
        {hasPending && onReviewPending && (
          <button
            onClick={onReviewPending}
            className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
          >
            Review {mappingCounts.pending} Pending Mapping{mappingCounts.pending !== 1 ? 's' : ''}
          </button>
        )}

        {/* Metrics */}
        {metrics && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {metrics.rowsProcessed.toLocaleString()} fields mapped
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  // Error state
  if (status === 'error' && error) {
    return (
      <div className="px-3 py-3">
        <div className="p-2 bg-red-50 border border-red-200 rounded text-xs">
          <div className="font-semibold text-red-700 mb-1">{error.message}</div>
          {error.details && <div className="text-red-600">{error.details}</div>}
        </div>
      </div>
    );
  }

  // Configured state (idle)
  if (config && status === 'idle') {
    const totalMappings = config.field_mappings?.length || 0;
    const hasPending = mappingCounts.pending > 0;

    return (
      <div className="px-3 py-3 space-y-3">
        {/* Ontology target */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Layers className="w-3 h-3" />
            <span className="font-medium">Target Ontology</span>
          </div>
          <button
            onClick={onOntologyClick}
            className="w-full text-left text-xs text-foreground hover:text-blue-600 transition-colors truncate"
          >
            {config.target_ontology && config.target_ontology.length > 0 ? config.target_ontology.join(', ') : 'Not configured'}
          </button>
        </div>

        {/* Mapping mode badge */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Mapping Mode</div>
          <InlineBadgeToggle
            value={config.mapping_mode}
            options={[
              { value: 'auto', label: 'Auto', color: 'success' },
              { value: 'manual', label: 'Manual', color: 'secondary' },
              { value: 'hybrid', label: 'Hybrid', color: 'warning' },
            ]}
            onChange={(value) => {
              // TODO: Implement mapping mode toggle callback
            }}
          />
        </div>

        {/* Mapping status pills */}
        {totalMappings > 0 && (
          <div>
            <div className="text-xs font-medium text-foreground mb-1.5">
              Mapping Status ({totalMappings} fields)
            </div>
            <StatusPills
              pills={([
                {
                  label: 'Approved',
                  count: mappingCounts.approved,
                  color: 'success' as const,
                  icon: 'check' as const,
                  onClick: onStatusPillClick ? () => onStatusPillClick('approved') : undefined,
                },
                {
                  label: 'Pending',
                  count: mappingCounts.pending,
                  color: 'warning' as const,
                  icon: 'clock' as const,
                  onClick: onStatusPillClick ? () => onStatusPillClick('pending') : undefined,
                },
                {
                  label: 'Rejected',
                  count: mappingCounts.rejected,
                  color: 'danger' as const,
                  icon: 'x' as const,
                  onClick: onStatusPillClick ? () => onStatusPillClick('rejected') : undefined,
                },
              ] as StatusPill[]).filter(pill => pill.count > 0)}
            />
          </div>
        )}

        {/* Auto-approve threshold */}
        <div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
            <Settings className="w-3 h-3" />
            <span className="font-medium">Auto-approve threshold</span>
          </div>
          <button
            onClick={onThresholdClick}
            className="text-xs text-foreground hover:text-blue-600 transition-colors"
          >
            {((config.auto_approve_threshold ?? 0.9) * 100).toFixed(0)}% confidence
          </button>
        </div>

        {/* Review pending action */}
        {hasPending && onReviewPending && (
          <button
            onClick={onReviewPending}
            className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
          >
            Review {mappingCounts.pending} Pending Mapping{mappingCounts.pending !== 1 ? 's' : ''}
          </button>
        )}
      </div>
    );
  }

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure ontology mapping
      </div>
    </div>
  );
}
