/**
 * Data Source Card Component
 *
 * Unified card component that displays either a file or datasource
 * with appropriate metadata, icons, and actions.
 */

import React from 'react';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Eye,
  Download,
  Trash2,
  Tag as TagIcon,
  Calendar,
  User,
  Database,
  HardDrive,
  CheckCircle2,
  XCircle,
  AlertCircle,
  Clock,
  FileText,
} from 'lucide-react';
import { formatSize, getSourceIcon, getStatusColor, type UnifiedDataSource } from '@/api/dataCatalogue';
import { cn } from '@/lib/utils';

export interface DataSourceCardProps {
  source: UnifiedDataSource;
  selected: boolean;
  onSelect: (selected: boolean) => void;
  onDelete: () => void;
  onView?: () => void;
  onDownload?: () => void; // Only for files
}

export function DataSourceCard({
  source,
  selected,
  onSelect,
  onDelete,
  onView,
  onDownload,
}: DataSourceCardProps) {
  const createdDate = new Date(source.created_at);
  const icon = getSourceIcon(source);
  const statusColor = getStatusColor(source.status);

  // Determine status badge
  const getStatusBadge = () => {
    const iconMap = {
      active: CheckCircle2,
      registered: CheckCircle2,
      inactive: XCircle,
      error: AlertCircle,
      unregistered: Clock,
    };

    const Icon = iconMap[source.status] || AlertCircle;
    const label = source.status.charAt(0).toUpperCase() + source.status.slice(1);

    const colorMap: Record<string, string> = {
      green: 'bg-green-100 text-green-700 border-green-200',
      yellow: 'bg-yellow-100 text-yellow-700 border-yellow-200',
      red: 'bg-red-100 text-red-700 border-red-200',
      gray: 'bg-gray-100 text-gray-700 border-gray-200',
    };

    return (
      <Badge variant="outline" className={cn('text-xs gap-1', colorMap[statusColor] || colorMap.gray)}>
        <Icon className="h-3 w-3" />
        {label}
      </Badge>
    );
  };

  // Get type label with icon
  const getTypeLabel = () => {
    if (source.type === 'file') {
      return (
        <div className="flex items-center gap-1 text-xs text-muted-foreground">
          <FileText className="h-3 w-3" />
          <span>{source.file_type || 'File'}</span>
        </div>
      );
    } else {
      return (
        <div className="flex items-center gap-1 text-xs text-muted-foreground">
          <Database className="h-3 w-3" />
          <span>{source.datasource_category || 'Datasource'}</span>
        </div>
      );
    }
  };

  return (
    <Card
      className={cn(
        'hover:border-primary transition-all cursor-pointer relative group',
        selected && 'border-primary bg-primary/5'
      )}
      onClick={(e) => {
        // Don't trigger selection when clicking buttons
        if ((e.target as HTMLElement).closest('button')) return;
        onSelect(!selected);
      }}
    >
      {/* Selection Checkbox */}
      <div className="absolute top-3 left-3 z-10">
        <input
          type="checkbox"
          checked={selected}
          onChange={(e) => {
            e.stopPropagation();
            onSelect(e.target.checked);
          }}
          className="h-4 w-4 rounded border-gray-300"
        />
      </div>

      <CardHeader className="pb-3">
        <div className="flex items-start justify-between pl-6">
          {/* Icon & Info */}
          <div className="flex items-start gap-2 flex-1 min-w-0">
            <div className="text-3xl flex-shrink-0">{icon}</div>
            <div className="flex-1 min-w-0">
              <h3
                className="text-sm font-medium text-foreground truncate"
                title={source.name}
              >
                {source.name}
              </h3>
              <div className="flex items-center gap-2 mt-1">
                {getTypeLabel()}
                {source.size_bytes !== undefined && (
                  <span className="text-xs text-muted-foreground">
                    • {formatSize(source.size_bytes)}
                  </span>
                )}
              </div>
            </div>
          </div>

          {/* Quick Actions (visible on hover) */}
          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
            {onView && (
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0"
                title="View details"
                onClick={(e) => {
                  e.stopPropagation();
                  onView();
                }}
              >
                <Eye className="h-3.5 w-3.5" />
              </Button>
            )}
            {onDownload && source.type === 'file' && (
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0"
                title="Download"
                onClick={(e) => {
                  e.stopPropagation();
                  onDownload();
                }}
              >
                <Download className="h-3.5 w-3.5" />
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              className="h-7 w-7 p-0 text-destructive hover:text-destructive"
              title="Delete"
              onClick={(e) => {
                e.stopPropagation();
                onDelete();
              }}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-3">
        {/* Description (for datasources) */}
        {source.description && (
          <p className="text-xs text-muted-foreground line-clamp-2">
            {source.description}
          </p>
        )}

        {/* Status and Connection Status */}
        <div className="flex items-center gap-2 flex-wrap">
          {getStatusBadge()}
          {source.connection_status && source.type === 'datasource' && (
            <Badge variant="outline" className="text-xs gap-1">
              <HardDrive className="h-3 w-3" />
              {source.connection_status}
            </Badge>
          )}
          {source.datasource_id && source.type === 'file' && (
            <Badge variant="outline" className="text-xs gap-1 bg-blue-50 text-blue-700 border-blue-200">
              <Database className="h-3 w-3" />
              Registered
            </Badge>
          )}
        </div>

        {/* Tags */}
        {source.tags && source.tags.length > 0 && (
          <div className="flex items-center gap-1.5 flex-wrap">
            <TagIcon className="h-3 w-3 text-muted-foreground flex-shrink-0" />
            {source.tags.slice(0, 3).map((tag) => (
              <Badge
                key={tag}
                variant="secondary"
                className="text-xs px-1.5 py-0 h-5"
              >
                {tag}
              </Badge>
            ))}
            {source.tags.length > 3 && (
              <Badge variant="secondary" className="text-xs px-1.5 py-0 h-5">
                +{source.tags.length - 3}
              </Badge>
            )}
          </div>
        )}

        {/* Schema Information */}
        {source.has_schema && source.schema_info && (
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            {source.schema_info.row_count !== undefined && (
              <div className="flex items-center gap-1">
                <span className="font-medium">{source.schema_info.row_count.toLocaleString()}</span>
                <span>rows</span>
              </div>
            )}
            {source.schema_info.column_count !== undefined && (
              <div className="flex items-center gap-1">
                <span className="font-medium">{source.schema_info.column_count}</span>
                <span>columns</span>
              </div>
            )}
            {source.schema_info.table_count !== undefined && (
              <div className="flex items-center gap-1">
                <span className="font-medium">{source.schema_info.table_count}</span>
                <span>tables</span>
              </div>
            )}
          </div>
        )}

        {/* Metadata */}
        <div className="space-y-1.5 text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <Calendar className="h-3 w-3 flex-shrink-0" />
            <span title={createdDate.toLocaleString()}>
              {source.type === 'file' ? 'Uploaded' : 'Created'} {createdDate.toLocaleDateString()}
            </span>
          </div>

          {source.uploaded_by && source.type === 'file' && (
            <div className="flex items-center gap-1.5">
              <User className="h-3 w-3 flex-shrink-0" />
              <span className="truncate" title={source.uploaded_by}>
                {source.uploaded_by}
              </span>
            </div>
          )}

          {source.plugin_name && source.type === 'datasource' && (
            <div className="flex items-center gap-1.5">
              <Database className="h-3 w-3 flex-shrink-0" />
              <span className="truncate" title={source.plugin_name}>
                {source.plugin_name}
              </span>
            </div>
          )}

          {source.access_count !== undefined && source.access_count > 0 && (
            <div className="flex items-center gap-1.5">
              <Eye className="h-3 w-3 flex-shrink-0" />
              <span>{source.access_count} views</span>
            </div>
          )}

          {source.last_accessed_at && (
            <div className="flex items-center gap-1.5">
              <Clock className="h-3 w-3 flex-shrink-0" />
              <span title={new Date(source.last_accessed_at).toLocaleString()}>
                Last accessed {new Date(source.last_accessed_at).toLocaleDateString()}
              </span>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
