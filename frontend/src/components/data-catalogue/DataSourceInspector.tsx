/**
 * Data Source Inspector Component
 *
 * Slide-out panel showing detailed information about a file or datasource
 * Displays different metadata based on source type
 */

import React from 'react';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import {
  Calendar,
  User,
  Database,
  FileText,
  HardDrive,
  Tag as TagIcon,
  Download,
  Trash2,
  CheckCircle2,
  XCircle,
  AlertCircle,
  Clock,
  Activity,
  Eye,
} from 'lucide-react';
import { formatSize, getSourceIcon, getStatusColor, type UnifiedDataSource } from '@/api/dataCatalogue';
import { cn } from '@/lib/utils';

export interface DataSourceInspectorProps {
  source: UnifiedDataSource | null;
  open: boolean;
  onClose: () => void;
  onDelete?: () => void;
  onDownload?: () => void;
}

export function DataSourceInspector({
  source,
  open,
  onClose,
  onDelete,
  onDownload,
}: DataSourceInspectorProps) {
  if (!source) return null;

  const createdDate = new Date(source.created_at);
  const icon = getSourceIcon(source);
  const statusColor = getStatusColor(source.status);

  // Status badge
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
      <Badge variant="outline" className={cn('gap-1.5', colorMap[statusColor] || colorMap.gray)}>
        <Icon className="h-3.5 w-3.5" />
        {label}
      </Badge>
    );
  };

  return (
    <Sheet open={open} onOpenChange={(isOpen) => !isOpen && onClose()}>
      <SheetContent className="w-full sm:max-w-lg overflow-y-auto">
        <SheetHeader className="space-y-4">
          {/* Header with icon and title */}
          <div className="flex items-start gap-3">
            <div className="text-4xl flex-shrink-0">{icon}</div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-lg break-words">{source.name}</SheetTitle>
              <SheetDescription className="flex items-center gap-2 mt-1">
                {source.type === 'file' ? (
                  <>
                    <FileText className="h-3.5 w-3.5" />
                    <span>{source.file_type || 'File'}</span>
                  </>
                ) : (
                  <>
                    <Database className="h-3.5 w-3.5" />
                    <span>{source.datasource_category || 'Datasource'}</span>
                  </>
                )}
                {source.size_bytes !== undefined && (
                  <>
                    <span>•</span>
                    <span>{formatSize(source.size_bytes)}</span>
                  </>
                )}
              </SheetDescription>
            </div>
          </div>

          {/* Status and actions */}
          <div className="flex items-center gap-2 flex-wrap">
            {getStatusBadge()}
            {source.connection_status && source.type === 'datasource' && (
              <Badge variant="outline" className="gap-1.5">
                <HardDrive className="h-3.5 w-3.5" />
                {source.connection_status}
              </Badge>
            )}
            {source.datasource_id && source.type === 'file' && (
              <Badge variant="outline" className="gap-1.5 bg-blue-50 text-blue-700 border-blue-200">
                <Database className="h-3.5 w-3.5" />
                Registered as Datasource
              </Badge>
            )}
          </div>

          {/* Action buttons */}
          <div className="flex items-center gap-2">
            {onDownload && source.type === 'file' && (
              <Button onClick={onDownload} size="sm" variant="outline" className="gap-2">
                <Download className="h-4 w-4" />
                Download
              </Button>
            )}
            {onDelete && (
              <Button
                onClick={onDelete}
                size="sm"
                variant="destructive"
                className="gap-2"
              >
                <Trash2 className="h-4 w-4" />
                Delete
              </Button>
            )}
          </div>
        </SheetHeader>

        <Separator className="my-6" />

        {/* Details Section */}
        <div className="space-y-6">
          {/* Description (for datasources) */}
          {source.description && (
            <div>
              <h3 className="text-sm font-semibold mb-2">Description</h3>
              <p className="text-sm text-muted-foreground">{source.description}</p>
            </div>
          )}

          {/* Metadata */}
          <div>
            <h3 className="text-sm font-semibold mb-3">Details</h3>
            <div className="space-y-3">
              <div className="flex items-start gap-3">
                <Calendar className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                <div className="flex-1">
                  <div className="text-xs text-muted-foreground">
                    {source.type === 'file' ? 'Uploaded' : 'Created'}
                  </div>
                  <div className="text-sm font-medium">
                    {createdDate.toLocaleString()}
                  </div>
                </div>
              </div>

              {source.updated_at && (
                <div className="flex items-start gap-3">
                  <Clock className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Last Updated</div>
                    <div className="text-sm font-medium">
                      {new Date(source.updated_at).toLocaleString()}
                    </div>
                  </div>
                </div>
              )}

              {source.uploaded_by && source.type === 'file' && (
                <div className="flex items-start gap-3">
                  <User className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Uploaded By</div>
                    <div className="text-sm font-medium">{source.uploaded_by}</div>
                  </div>
                </div>
              )}

              {source.plugin_name && source.type === 'datasource' && (
                <div className="flex items-start gap-3">
                  <Database className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Plugin</div>
                    <div className="text-sm font-medium">{source.plugin_name}</div>
                  </div>
                </div>
              )}

              {source.access_count !== undefined && source.access_count > 0 && (
                <div className="flex items-start gap-3">
                  <Eye className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Access Count</div>
                    <div className="text-sm font-medium">{source.access_count} views</div>
                  </div>
                </div>
              )}

              {source.last_accessed_at && (
                <div className="flex items-start gap-3">
                  <Activity className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Last Accessed</div>
                    <div className="text-sm font-medium">
                      {new Date(source.last_accessed_at).toLocaleString()}
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Schema Information */}
          {source.has_schema && source.schema_info && (
            <div>
              <h3 className="text-sm font-semibold mb-3">Schema</h3>
              <div className="grid grid-cols-2 gap-3">
                {source.schema_info.row_count !== undefined && (
                  <div className="bg-muted/50 rounded-md p-3">
                    <div className="text-xs text-muted-foreground">Rows</div>
                    <div className="text-lg font-semibold">
                      {source.schema_info.row_count.toLocaleString()}
                    </div>
                  </div>
                )}
                {source.schema_info.column_count !== undefined && (
                  <div className="bg-muted/50 rounded-md p-3">
                    <div className="text-xs text-muted-foreground">Columns</div>
                    <div className="text-lg font-semibold">
                      {source.schema_info.column_count}
                    </div>
                  </div>
                )}
                {source.schema_info.table_count !== undefined && (
                  <div className="bg-muted/50 rounded-md p-3">
                    <div className="text-xs text-muted-foreground">Tables</div>
                    <div className="text-lg font-semibold">
                      {source.schema_info.table_count}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Tags */}
          {source.tags && source.tags.length > 0 && (
            <div>
              <h3 className="text-sm font-semibold mb-3 flex items-center gap-2">
                <TagIcon className="h-4 w-4" />
                Tags
              </h3>
              <div className="flex flex-wrap gap-2">
                {source.tags.map((tag) => (
                  <Badge key={tag} variant="secondary">
                    {tag}
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Custom Metadata */}
          {source.custom_metadata && Object.keys(source.custom_metadata).length > 0 && (
            <div>
              <h3 className="text-sm font-semibold mb-3">Custom Metadata</h3>
              <div className="bg-muted/50 rounded-md p-3 font-mono text-xs">
                <pre className="overflow-x-auto">
                  {JSON.stringify(source.custom_metadata, null, 2)}
                </pre>
              </div>
            </div>
          )}

          {/* File-specific: MIME type */}
          {source.type === 'file' && source.mime_type && (
            <div>
              <h3 className="text-sm font-semibold mb-3">Technical Details</h3>
              <div className="space-y-2 text-sm">
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">MIME Type</span>
                  <Badge variant="outline" className="font-mono text-xs">
                    {source.mime_type}
                  </Badge>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Source ID</span>
                  <code className="text-xs bg-muted px-2 py-1 rounded">
                    {source.id}
                  </code>
                </div>
              </div>
            </div>
          )}

          {/* Datasource-specific: Connection info */}
          {source.type === 'datasource' && (
            <div>
              <h3 className="text-sm font-semibold mb-3">Connection</h3>
              <div className="space-y-2 text-sm">
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Status</span>
                  <Badge variant="outline">
                    {source.connection_status || 'Unknown'}
                  </Badge>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Category</span>
                  <Badge variant="outline">
                    {source.datasource_category || 'Unknown'}
                  </Badge>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Source ID</span>
                  <code className="text-xs bg-muted px-2 py-1 rounded">
                    {source.id}
                  </code>
                </div>
              </div>
            </div>
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}
