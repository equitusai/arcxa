/**
 * Data Joiner Node Body
 * JOIN two datasets (INNER, LEFT, RIGHT, FULL)
 */

import { ArrowRightLeft } from 'lucide-react';
import { InlineBadgeToggle } from '../widgets';
import type { DataJoinerConfig } from '@/lib/workflow-etl-config';

export interface DataJoinerNodeBodyProps {
  config?: DataJoinerConfig;
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
  onConfigureJoin?: () => void;
}

export function DataJoinerNodeBody({
  config,
  status = 'idle',
  progress,
  metrics,
  error,
  onConfigureJoin,
}: DataJoinerNodeBodyProps) {
  // Running state
  if (status === 'running' && progress !== undefined) {
    return (
      <div className="px-3 py-3">
        <div className="mb-3">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-muted-foreground">Joining datasets...</span>
            <span className="font-semibold text-foreground">{progress}%</span>
          </div>
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-green-500 to-green-400 transition-all duration-300"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      </div>
    );
  }

  // Success/Configured state
  if (config) {
    return (
      <div className="px-3 py-3 space-y-3">
        {/* Join type */}
        <div>
          <div className="text-xs font-medium text-muted-foreground mb-1">Join Type</div>
          <InlineBadgeToggle
            value={config.join_type}
            options={[
              { value: 'inner', label: 'Inner', color: 'success' },
              { value: 'left', label: 'Left', color: 'warning' },
              { value: 'right', label: 'Right', color: 'warning' },
              { value: 'full', label: 'Full', color: 'secondary' },
            ]}
            onChange={(value) => {
              // TODO: Implement join type toggle callback
            }}
          />
        </div>

        {/* Join keys */}
        <div className="p-2 bg-muted border border-neutral-200 rounded text-xs space-y-2">
          <div className="font-medium text-foreground">Join Keys</div>

          <div>
            <div className="text-muted-foreground mb-0.5">Left:</div>
            <div className="text-foreground font-mono">
              {config.left_key && config.left_key.length > 0 ? config.left_key.join(', ') : 'Not configured'}
            </div>
          </div>

          <div className="flex items-center justify-center text-neutral-400">
            <ArrowRightLeft className="w-3 h-3" />
          </div>

          <div>
            <div className="text-muted-foreground mb-0.5">Right:</div>
            <div className="text-foreground font-mono">
              {config.right_key && config.right_key.length > 0 ? config.right_key.join(', ') : 'Not configured'}
            </div>
          </div>
        </div>

        {/* Output columns */}
        {config.output_columns && config.output_columns.length > 0 && (
          <div>
            <div className="text-xs font-medium text-muted-foreground mb-1">
              Output Columns ({config.output_columns.length})
            </div>
            <div className="text-xs text-muted-foreground truncate">
              {config.output_columns.slice(0, 3).join(', ')}
              {config.output_columns.length > 3 && ` +${config.output_columns.length - 3} more`}
            </div>
          </div>
        )}

        {/* Metrics */}
        {metrics && status === 'success' && (
          <div className="space-y-1 pt-2 border-t border-border">
            <div className="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-500">
              <div className="w-1 h-1 rounded-full bg-green-500" />
              Complete ({metrics.duration}ms)
            </div>
            {metrics.rowsProcessed && (
              <div className="text-xs text-muted-foreground pl-2.5">
                {metrics.rowsProcessed.toLocaleString()} rows output
              </div>
            )}
          </div>
        )}

        {/* Configure button */}
        {onConfigureJoin && (
          <button
            onClick={onConfigureJoin}
            className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
          >
            Configure Join
          </button>
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

  // Unconfigured state
  return (
    <div className="px-3 py-3">
      <div className="text-xs text-amber-600 flex items-center gap-1.5 mb-2">
        <div className="w-1 h-1 rounded-full bg-amber-500" />
        Click to configure join
      </div>
      {onConfigureJoin && (
        <button
          onClick={onConfigureJoin}
          className="w-full px-2 py-1.5 text-xs font-medium text-blue-700 bg-blue-50 hover:bg-blue-100 border border-blue-200 rounded transition-colors"
        >
          Configure Join Keys
        </button>
      )}
    </div>
  );
}
