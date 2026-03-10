/**
 * File Card Component
 * Displays individual file with metadata, actions, and selection
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
  Sparkles,
  ArrowRight,
  Link2,
} from 'lucide-react';
import { formatFileSize, getFileIcon, hasFileSchema, getSchemaFieldCount } from '@/api/fileLibrary';
import type { FileMetadata } from '@/api/types';
import { cn } from '@/lib/utils';

export interface FileCardProps {
  file: FileMetadata;
  selected: boolean;
  onSelect: (selected: boolean) => void;
  onDelete: () => void;
  onView?: () => void;
  onDownload?: () => void;
  onProfile?: () => void; // QW2: Trigger profiling
  onUseInWorkflow?: () => void; // QW3: Navigate to workflow with this file
}

export function FileCard({
  file,
  selected,
  onSelect,
  onDelete,
  onView,
  onDownload,
  onProfile,
  onUseInWorkflow,
}: FileCardProps) {
  const uploadedDate = new Date(file.uploaded_at || Date.now());
  const icon = getFileIcon(String(file.mime_type || 'application/octet-stream'));

  // QW1: Check if file has schema profiling
  const hasSchema = hasFileSchema(file);
  const schemaColumnCount = getSchemaFieldCount(file);

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
          {/* File Icon & Info */}
          <div className="flex items-start gap-2 flex-1 min-w-0">
            <div className="text-3xl flex-shrink-0">{icon}</div>
            <div className="flex-1 min-w-0">
              <h3
                className="text-sm font-medium text-foreground truncate"
                title={String(file.filename || file.original_filename || 'Unknown')}
              >
                {String(file.original_filename || file.filename || 'Unknown')}
              </h3>
              <p className="text-xs text-muted-foreground mt-0.5">
                {formatFileSize(Number(file.size_bytes) || 0)}
              </p>
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
            {onDownload && (
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
        {/* Tags */}
        {file.tags && Array.isArray(file.tags) && file.tags.length > 0 && (
          <div className="flex items-center gap-1.5 flex-wrap">
            <TagIcon className="h-3 w-3 text-muted-foreground flex-shrink-0" />
            {file.tags.slice(0, 3).map((tag) => (
              <Badge
                key={typeof tag === 'string' ? tag : String(tag)}
                variant="secondary"
                className="text-xs px-1.5 py-0 h-5"
              >
                {typeof tag === 'string' ? tag : String(tag)}
              </Badge>
            ))}
            {file.tags.length > 3 && (
              <Badge variant="secondary" className="text-xs px-1.5 py-0 h-5">
                +{file.tags.length - 3}
              </Badge>
            )}
          </div>
        )}

        {/* Metadata */}
        <div className="space-y-1.5 text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <Calendar className="h-3 w-3 flex-shrink-0" />
            <span title={uploadedDate.toLocaleString()}>
              {uploadedDate.toLocaleDateString()}
            </span>
          </div>

          <div className="flex items-center gap-1.5">
            <User className="h-3 w-3 flex-shrink-0" />
            <span className="truncate" title={String(file.uploaded_by || 'Unknown')}>
              {String(file.uploaded_by || 'Unknown')}
            </span>
          </div>

          {Number(file.access_count) > 0 && (
            <div className="flex items-center gap-1.5">
              <Eye className="h-3 w-3 flex-shrink-0" />
              <span>{Number(file.access_count)} views</span>
            </div>
          )}
        </div>

        {/* Badges Row */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* MIME Type Badge */}
          <Badge variant="outline" className="text-xs font-mono">
            {getMimeTypeLabel(String(file.mime_type || 'application/octet-stream'))}
          </Badge>

          {/* QW1: Schema Status Badge */}
          {hasSchema ? (
            <Badge
              variant="outline"
              className="text-xs bg-green-50 text-green-700 border-green-200 flex items-center gap-1"
            >
              <Database className="h-3 w-3" />
              {schemaColumnCount} field{schemaColumnCount !== 1 ? 's' : ''}
            </Badge>
          ) : (
            <Badge
              variant="outline"
              className="text-xs bg-amber-50 text-amber-700 border-amber-200"
            >
              ⚠️ Not profiled
            </Badge>
          )}

          {/* Ontology Mappings Badge */}
          {file.ontology_mappings && file.ontology_mappings.length > 0 && (
            <Badge
              variant="outline"
              className="text-xs bg-blue-50 text-blue-700 border-blue-200 flex items-center gap-1"
              title={`${file.ontology_mappings.length} field${file.ontology_mappings.length !== 1 ? 's' : ''} mapped to ontology`}
            >
              <Link2 className="h-3 w-3" />
              {file.ontology_mappings.length} mapped
            </Badge>
          )}
        </div>

        {/* QW2 & QW3: Action Buttons */}
        <div className="flex items-center gap-2 pt-1">
          {/* QW2: Profile Button (only show if not profiled and callback provided) */}
          {!hasSchema && onProfile && (
            <Button
              variant="outline"
              size="sm"
              className="h-8 text-xs w-full"
              onClick={(e) => {
                e.stopPropagation();
                onProfile();
              }}
            >
              <Sparkles className="h-3 w-3 mr-1.5" />
              Profile This File
            </Button>
          )}

          {/* QW3: Use in Workflow Button (only show if profiled and callback provided) */}
          {hasSchema && onUseInWorkflow && (
            <Button
              variant="default"
              size="sm"
              className="h-8 text-xs w-full"
              onClick={(e) => {
                e.stopPropagation();
                onUseInWorkflow();
              }}
            >
              Use in Workflows
              <ArrowRight className="h-3 w-3 ml-1.5" />
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

/**
 * Get friendly label for MIME type
 */
function getMimeTypeLabel(mimeType: string): string {
  const typeMap: Record<string, string> = {
    'text/csv': 'CSV',
    'text/tab-separated-values': 'TSV',
    'application/vnd.ms-excel': 'Excel',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': 'Excel',
    'application/json': 'JSON',
    'application/xml': 'XML',
    'text/xml': 'XML',
    'application/pdf': 'PDF',
  };

  return typeMap[mimeType] || mimeType.split('/')[1]?.toUpperCase() || 'File';
}
