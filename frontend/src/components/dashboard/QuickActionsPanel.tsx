/**
 * Quick Actions Panel
 * Contextual shortcuts for common tasks
 */

import { Plus, FileText, Database, Settings, Upload, Download, LucideIcon } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';

export interface QuickAction {
  id: string;
  label: string;
  icon: LucideIcon;
  onClick: () => void;
  shortcut?: string;
  variant?: 'default' | 'outline' | 'ghost';
}

export interface QuickActionsPanelProps {
  actions?: QuickAction[];
  recentItems?: Array<{
    id: string;
    label: string;
    type: string;
    onClick: () => void;
  }>;
  className?: string;
}

const defaultActions: QuickAction[] = [
  {
    id: 'new-workflow',
    label: 'New Workflow',
    icon: Plus,
    onClick: () => console.log('New workflow'),
    shortcut: '⌘E',
  },
  {
    id: 'add-datasource',
    label: 'Add Data Source',
    icon: Database,
    onClick: () => console.log('Add datasource'),
    shortcut: '⌘D',
  },
  {
    id: 'import-ontology',
    label: 'Import Ontology',
    icon: Upload,
    onClick: () => console.log('Import ontology'),
    shortcut: '⌘I',
  },
  {
    id: 'export-data',
    label: 'Export Data',
    icon: Download,
    onClick: () => console.log('Export data'),
    shortcut: '⌘X',
  },
  {
    id: 'settings',
    label: 'Settings',
    icon: Settings,
    onClick: () => console.log('Settings'),
    shortcut: '⌘,',
  },
];

export function QuickActionsPanel({
  actions = defaultActions,
  recentItems = [],
  className,
}: QuickActionsPanelProps) {
  return (
    <Card className={cn('p-6', className)}>
      {/* Header */}
      <div className="flex items-center gap-2 mb-4">
        <FileText className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold">Quick Actions</h3>
      </div>

      {/* Action Buttons */}
      <div className="grid grid-cols-2 gap-2 mb-6">
        {actions.map((action, index) => {
          const ActionIcon = action.icon;

          return (
            <Button
              key={action.id}
              variant={action.variant || 'outline'}
              onClick={action.onClick}
              className={cn(
                'h-auto py-3 px-4 flex flex-col items-start gap-2',
                'hover:scale-105 transition-transform duration-200',
                'group'
              )}
              style={{
                animationDelay: `${index * 50}ms`,
              }}
            >
              <div className="flex items-center justify-between w-full">
                <ActionIcon className="w-4 h-4 text-primary" />
                {action.shortcut && (
                  <span className="text-xs text-muted-foreground font-mono">
                    {action.shortcut}
                  </span>
                )}
              </div>
              <span className="text-xs font-medium text-left">{action.label}</span>
            </Button>
          );
        })}
      </div>

      {/* Recent Items */}
      {recentItems.length > 0 && (
        <div>
          <h4 className="text-xs font-medium text-muted-foreground mb-2">Recent Items</h4>
          <div className="space-y-1">
            {recentItems.slice(0, 3).map((item) => (
              <button
                key={item.id}
                onClick={item.onClick}
                className={cn(
                  'w-full flex items-center justify-between p-2 rounded-md',
                  'hover:bg-accent transition-colors text-left'
                )}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <FileText className="w-3 h-3 text-muted-foreground flex-shrink-0" />
                  <span className="text-xs text-foreground truncate">{item.label}</span>
                </div>
                <span className="text-xs text-muted-foreground capitalize flex-shrink-0">
                  {item.type}
                </span>
              </button>
            ))}
          </div>
        </div>
      )}
    </Card>
  );
}
