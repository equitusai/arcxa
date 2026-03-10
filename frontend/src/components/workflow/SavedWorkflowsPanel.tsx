/**
 * Saved Workflows Panel
 * Gradio-style always-visible panel for browsing and loading workflows
 */

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  FolderOpen,
  Search,
  Calendar,
  PlayCircle,
  MoreVertical,
  Trash2,
  Copy,
  ChevronLeft,
  ChevronRight,
  Loader2,
} from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import type { Workflow } from '@/api/types';
import { formatDistanceToNow } from 'date-fns';

interface SavedWorkflowsPanelProps {
  workflows?: Workflow[];
  isLoading?: boolean;
  onLoadWorkflow: (workflow: Workflow) => void;
  onDeleteWorkflow?: (workflowId: string) => void;
  onDuplicateWorkflow?: (workflow: Workflow) => void;
  selectedWorkflowId?: string | null;
  isCollapsed?: boolean;
  onToggleCollapse?: () => void;
}

export function SavedWorkflowsPanel({
  workflows = [],
  isLoading = false,
  onLoadWorkflow,
  onDeleteWorkflow,
  onDuplicateWorkflow,
  selectedWorkflowId,
  isCollapsed = false,
  onToggleCollapse,
}: SavedWorkflowsPanelProps) {
  const [searchQuery, setSearchQuery] = useState('');

  // Ensure workflows is always an array (handle undefined case during refetch)
  const workflowList = Array.isArray(workflows) ? workflows : [];

  const filteredWorkflows = workflowList.filter((workflow) =>
    workflow.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    workflow.id.toLowerCase().includes(searchQuery.toLowerCase())
  );

  return (
    <motion.aside
      initial={false}
      animate={{
        width: isCollapsed ? 48 : 320,
      }}
      transition={{ duration: 0.2, ease: 'easeInOut' }}
      className="border-r border-border bg-background flex flex-col h-full relative"
    >
      {/* Collapse/Expand Button */}
      <Button
        variant="ghost"
        size="sm"
        className="absolute top-2 right-2 z-10 h-7 w-7 p-0"
        onClick={onToggleCollapse}
        title={isCollapsed ? 'Expand Workflows Panel' : 'Collapse Workflows Panel'}
      >
        {isCollapsed ? (
          <ChevronRight className="h-4 w-4" />
        ) : (
          <ChevronLeft className="h-4 w-4" />
        )}
      </Button>

      {!isCollapsed && (
        <>
          {/* Header */}
          <div className="p-4 border-b border-border">
            <div className="flex items-center gap-2 mb-3">
              <FolderOpen className="h-5 w-5 text-primary" />
              <h3 className="font-semibold text-sm">Saved Workflows</h3>
            </div>

            {/* Search */}
            <div className="relative">
              <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
              <Input
                type="search"
                placeholder="Search workflows..."
                className="pl-9 h-9"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
            </div>
          </div>

          {/* Workflow List */}
          <ScrollArea className="flex-1">
            <div className="p-3 space-y-2">
              {isLoading ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                </div>
              ) : filteredWorkflows.length === 0 ? (
                <div className="text-center py-8 px-4">
                  <FolderOpen className="h-12 w-12 mx-auto mb-3 text-muted-foreground opacity-50" />
                  <p className="text-sm text-muted-foreground">
                    {searchQuery ? 'No workflows found' : 'No workflows yet'}
                  </p>
                  {!searchQuery && (
                    <p className="text-xs text-muted-foreground mt-1">
                      Create your first workflow to get started
                    </p>
                  )}
                </div>
              ) : (
                <AnimatePresence>
                  {filteredWorkflows.map((workflow) => (
                    <WorkflowCard
                      key={workflow.id}
                      workflow={workflow}
                      isSelected={workflow.id === selectedWorkflowId}
                      onLoad={() => onLoadWorkflow(workflow)}
                      onDelete={onDeleteWorkflow ? () => onDeleteWorkflow(workflow.id) : undefined}
                      onDuplicate={onDuplicateWorkflow ? () => onDuplicateWorkflow(workflow) : undefined}
                    />
                  ))}
                </AnimatePresence>
              )}
            </div>
          </ScrollArea>

          {/* Footer Stats */}
          <div className="border-t border-border p-3">
            <div className="flex items-center justify-between text-xs text-muted-foreground">
              <span>{filteredWorkflows.length} workflow{filteredWorkflows.length !== 1 ? 's' : ''}</span>
              {searchQuery && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setSearchQuery('')}
                  className="h-6 text-xs"
                >
                  Clear search
                </Button>
              )}
            </div>
          </div>
        </>
      )}
    </motion.aside>
  );
}

interface WorkflowCardProps {
  workflow: Workflow;
  isSelected: boolean;
  onLoad: () => void;
  onDelete?: () => void;
  onDuplicate?: () => void;
}

function WorkflowCard({ workflow, isSelected, onLoad, onDelete, onDuplicate }: WorkflowCardProps) {
  const stepCount = workflow.definition?.steps?.length || 0;
  const createdAt = workflow.created_at ? new Date(workflow.created_at) : null;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.15 }}
    >
      <Card
        className={cn(
          'group cursor-pointer transition-all hover:shadow-sm',
          isSelected ? 'border-primary bg-primary/5 shadow-sm' : 'hover:border-primary/50'
        )}
        onClick={onLoad}
      >
        <CardContent className="p-3">
          <div className="flex items-start justify-between mb-2">
            <div className="flex-1 min-w-0">
              <h4 className={cn(
                'text-sm font-medium truncate',
                isSelected && 'text-primary'
              )}>
                {workflow.name}
              </h4>
              <p className="text-xs text-muted-foreground truncate font-mono mt-0.5">
                {workflow.id}
              </p>
            </div>

            {(onDelete || onDuplicate) && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100"
                  >
                    <MoreVertical className="h-3 w-3" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={(e) => {
                    e.stopPropagation();
                    onLoad();
                  }}>
                    <PlayCircle className="h-4 w-4 mr-2" />
                    Load Workflow
                  </DropdownMenuItem>
                  {onDuplicate && (
                    <DropdownMenuItem onClick={(e) => {
                      e.stopPropagation();
                      onDuplicate();
                    }}>
                      <Copy className="h-4 w-4 mr-2" />
                      Duplicate
                    </DropdownMenuItem>
                  )}
                  {onDelete && (
                    <>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        className="text-destructive"
                        onClick={(e) => {
                          e.stopPropagation();
                          onDelete();
                        }}
                      >
                        <Trash2 className="h-4 w-4 mr-2" />
                        Delete
                      </DropdownMenuItem>
                    </>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>

          <div className="flex items-center gap-2 flex-wrap">
            {stepCount > 0 && (
              <Badge variant="secondary" className="text-xs">
                {stepCount} step{stepCount !== 1 ? 's' : ''}
              </Badge>
            )}

            {createdAt && (
              <div className="flex items-center gap-1 text-xs text-muted-foreground">
                <Calendar className="h-3 w-3" />
                <span>{formatDistanceToNow(createdAt, { addSuffix: true })}</span>
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </motion.div>
  );
}
