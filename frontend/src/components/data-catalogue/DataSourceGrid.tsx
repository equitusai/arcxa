/**
 * Data Source Grid Component
 *
 * Displays a grid of data sources (files and/or datasources)
 * with selection, filtering, and bulk actions.
 */

import React, { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { DataSourceCard } from './DataSourceCard';
import { Loader2, Trash2, Database, FileText } from 'lucide-react';
import type { UnifiedDataSource } from '@/api/dataCatalogue';

export interface DataSourceGridProps {
  sources: UnifiedDataSource[];
  loading?: boolean;
  onDelete?: (sources: UnifiedDataSource[]) => void;
  onView?: (source: UnifiedDataSource) => void;
  onDownload?: (source: UnifiedDataSource) => void;
  emptyMessage?: string;
  emptyDescription?: string;
}

export function DataSourceGrid({
  sources,
  loading = false,
  onDelete,
  onView,
  onDownload,
  emptyMessage = 'No data sources yet',
  emptyDescription = 'Get started by uploading files or connecting external datasources.',
}: DataSourceGridProps) {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // Selection handlers
  const handleSelectAll = (checked: boolean) => {
    if (checked) {
      setSelectedIds(new Set(sources.map(s => s.id)));
    } else {
      setSelectedIds(new Set());
    }
  };

  const handleSelectOne = (id: string, checked: boolean) => {
    const newSelection = new Set(selectedIds);
    if (checked) {
      newSelection.add(id);
    } else {
      newSelection.delete(id);
    }
    setSelectedIds(newSelection);
  };

  // Bulk actions
  const handleBulkDelete = () => {
    if (!onDelete || selectedIds.size === 0) return;

    const selectedSources = sources.filter(s => selectedIds.has(s.id));
    onDelete(selectedSources);
    setSelectedIds(new Set()); // Clear selection after delete
  };

  // Calculate selection state
  const allSelected = sources.length > 0 && selectedIds.size === sources.length;
  const someSelected = selectedIds.size > 0 && selectedIds.size < sources.length;

  // Loading state
  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  // Empty state
  if (sources.length === 0) {
    return (
      <Card>
        <CardContent className="flex flex-col items-center justify-center py-12">
          <div className="flex items-center gap-4 mb-4">
            <FileText className="h-12 w-12 text-muted-foreground" />
            <Database className="h-12 w-12 text-muted-foreground" />
          </div>
          <h3 className="text-lg font-semibold mb-2">{emptyMessage}</h3>
          <p className="text-sm text-muted-foreground text-center max-w-md">
            {emptyDescription}
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      {/* Selection Bar (only show when items are selected) */}
      {selectedIds.size > 0 && (
        <div className="flex items-center justify-between bg-primary/10 border border-primary/20 rounded-md px-4 py-3">
          <div className="flex items-center gap-3">
            <Checkbox
              checked={allSelected}
              onCheckedChange={handleSelectAll}
              aria-label={allSelected ? 'Deselect all' : 'Select all'}
            />
            <span className="text-sm font-medium">
              {selectedIds.size} {selectedIds.size === 1 ? 'item' : 'items'} selected
            </span>
          </div>

          <div className="flex items-center gap-2">
            {onDelete && (
              <Button
                variant="destructive"
                size="sm"
                onClick={handleBulkDelete}
                className="gap-2"
              >
                <Trash2 className="h-4 w-4" />
                Delete Selected
              </Button>
            )}
            <Button
              variant="outline"
              size="sm"
              onClick={() => setSelectedIds(new Set())}
            >
              Clear Selection
            </Button>
          </div>
        </div>
      )}

      {/* Select All Checkbox (when no items selected) */}
      {selectedIds.size === 0 && sources.length > 0 && (
        <div className="flex items-center gap-3 px-1">
          <Checkbox
            checked={allSelected}
            onCheckedChange={handleSelectAll}
            aria-label="Select all"
          />
          <span className="text-sm text-muted-foreground">
            Select all ({sources.length} {sources.length === 1 ? 'item' : 'items'})
          </span>
        </div>
      )}

      {/* Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
        {sources.map((source) => (
          <DataSourceCard
            key={source.id}
            source={source}
            selected={selectedIds.has(source.id)}
            onSelect={(checked) => handleSelectOne(source.id, checked)}
            onDelete={() => onDelete?.([source])}
            onView={() => onView?.(source)}
            onDownload={
              source.type === 'file' && onDownload
                ? () => onDownload(source)
                : undefined
            }
          />
        ))}
      </div>

      {/* Footer summary */}
      <div className="flex items-center justify-between text-xs text-muted-foreground pt-2 border-t">
        <div>
          Showing {sources.length} {sources.length === 1 ? 'source' : 'sources'}
        </div>
        <div className="flex items-center gap-4">
          <span>
            {sources.filter(s => s.type === 'file').length} files
          </span>
          <span>•</span>
          <span>
            {sources.filter(s => s.type === 'datasource').length} datasources
          </span>
        </div>
      </div>
    </div>
  );
}
