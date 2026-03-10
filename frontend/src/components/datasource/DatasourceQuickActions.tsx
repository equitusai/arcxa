/**
 * Datasource Quick Actions Panel
 * Contextual shortcuts for datasource management
 */

import { Plus, Database, RefreshCw, Activity, FileText, Settings, Zap } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { Datasource } from '@/api/types';

export interface DatasourceQuickActionsProps {
  onAddDatasource?: () => void;
  onTestAll?: () => void;
  onRefreshAll?: () => void;
  onViewHealth?: () => void;
  recentDatasources?: Datasource[];
  className?: string;
}

export function DatasourceQuickActions({
  onAddDatasource,
  onTestAll,
  onRefreshAll,
  onViewHealth,
  recentDatasources = [],
  className,
}: DatasourceQuickActionsProps) {
  const actions = [
    {
      id: 'add',
      label: 'Add Data Source',
      icon: Plus,
      onClick: onAddDatasource,
      variant: 'default' as const,
      shortcut: '⌘N',
    },
    {
      id: 'test-all',
      label: 'Test All',
      icon: Activity,
      onClick: onTestAll,
      variant: 'outline' as const,
      shortcut: '⌘T',
    },
    {
      id: 'refresh-all',
      label: 'Refresh All',
      icon: RefreshCw,
      onClick: onRefreshAll,
      variant: 'outline' as const,
      shortcut: '⌘R',
    },
    {
      id: 'health',
      label: 'View Health',
      icon: Zap,
      onClick: onViewHealth,
      variant: 'outline' as const,
      shortcut: '⌘H',
    },
  ];

  return (
    <Card className={cn('p-6', className)}>
      {/* Header */}
      <div className="flex items-center gap-2 mb-4">
        <Settings className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold">Quick Actions</h3>
      </div>

      {/* Action Buttons */}
      <div className="grid grid-cols-2 gap-2 mb-6">
        {actions.map((action, index) => {
          const ActionIcon = action.icon;

          return (
            <Button
              key={action.id}
              variant={action.variant}
              onClick={action.onClick}
              disabled={!action.onClick}
              className={cn(
                'h-auto py-3 px-3 flex flex-col items-start gap-2',
                'hover:scale-105 transition-transform duration-200',
                'group',
                action.variant === 'default' && 'col-span-2'
              )}
              style={{
                animationDelay: `${index * 50}ms`,
              }}
            >
              <div className="flex items-center justify-between w-full">
                <ActionIcon className={cn(
                  'w-4 h-4',
                  action.variant === 'default' ? 'text-primary-foreground' : 'text-primary'
                )} />
                {action.shortcut && (
                  <span className={cn(
                    'text-xs font-mono',
                    action.variant === 'default' ? 'text-primary-foreground/70' : 'text-muted-foreground'
                  )}>
                    {action.shortcut}
                  </span>
                )}
              </div>
              <span className="text-xs font-medium text-left">{action.label}</span>
            </Button>
          );
        })}
      </div>

      {/* Recent Data Sources */}
      {recentDatasources.length > 0 && (
        <div>
          <h4 className="text-xs font-medium text-muted-foreground mb-2">Recent Data Sources</h4>
          <div className="space-y-1">
            {recentDatasources.slice(0, 3).map((ds) => {
              const isConnected = ds.status === 'Connected';
              const hasError = typeof ds.status === 'object' && 'Error' in ds.status;

              return (
                <button
                  key={ds.id}
                  className={cn(
                    'w-full flex items-center justify-between p-2 rounded-md',
                    'hover:bg-accent transition-colors text-left group'
                  )}
                >
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    <div className={cn(
                      'w-1.5 h-1.5 rounded-full',
                      isConnected && 'bg-green-500',
                      hasError && 'bg-red-500',
                      !isConnected && !hasError && 'bg-gray-400'
                    )} />
                    <Database className="w-3 h-3 text-muted-foreground flex-shrink-0" />
                    <span className="text-xs text-foreground truncate">{ds.name}</span>
                  </div>
                  <span className="text-xs text-muted-foreground capitalize flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                    {typeof ds.metadata.datasource_type === 'string'
                      ? ds.metadata.datasource_type
                      : ds.metadata.datasource_type?.Custom || 'Unknown'}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Stats Summary */}
      <div className="mt-6 pt-4 border-t border-border">
        <div className="grid grid-cols-3 gap-2 text-center">
          <div>
            <p className="text-lg font-bold text-foreground">{recentDatasources.length}</p>
            <p className="text-xs text-muted-foreground">Total</p>
          </div>
          <div>
            <p className="text-lg font-bold text-green-600">
              {recentDatasources.filter((ds) => ds.status === 'Connected').length}
            </p>
            <p className="text-xs text-muted-foreground">Active</p>
          </div>
          <div>
            <p className="text-lg font-bold text-red-600">
              {recentDatasources.filter((ds) =>
                typeof ds.status === 'object' && 'Error' in ds.status
              ).length}
            </p>
            <p className="text-xs text-muted-foreground">Errors</p>
          </div>
        </div>
      </div>
    </Card>
  );
}
