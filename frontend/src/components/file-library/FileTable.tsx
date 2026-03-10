/**
 * File Table Component
 * Phase 2.2: Table view with schema column for better data visibility
 * Updated: Now displays both folders and files
 */

import React from 'react';
import {
  Eye,
  Download,
  Trash2,
  Database,
  Sparkles,
  ArrowRight,
  Calendar,
  User,
  MoreVertical,
  Folder,
  Link2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import type { FileMetadata } from '@/api/types';
import type { FileLibraryItem } from '@/lib/fileLibraryTypes';
import { formatFileSize, getFileIcon, hasFileSchema, getSchemaFieldCount } from '@/api/fileLibrary';
import { cn } from '@/lib/utils';
import { isFolder, getItemId, getItemName } from '@/lib/fileLibraryUtils';

export interface FileTableProps {
  items: FileLibraryItem[];
  selectedItems: string[];
  onSelectItem: (itemId: string, selected: boolean) => void;
  onSelectAll: (selected: boolean) => void;
  onDelete: (itemId: string) => void;
  onFolderOpen?: (folderId: string) => void;
  onView?: (fileId: string) => void;
  onDownload?: (fileId: string) => void;
  onProfile?: (fileId: string) => void;
  onUseInWorkflow?: (fileId: string) => void;
}

export function FileTable({
  items,
  selectedItems,
  onSelectItem,
  onSelectAll,
  onDelete,
  onFolderOpen,
  onView,
  onDownload,
  onProfile,
  onUseInWorkflow,
}: FileTableProps) {
  const allSelected = items.length > 0 && selectedItems.length === items.length;
  const someSelected = selectedItems.length > 0 && !allSelected;

  return (
    <div className="border rounded-lg overflow-hidden bg-background">
      <Table>
        <TableHeader>
          <TableRow className="bg-muted/50">
            <TableHead className="w-12">
              <Checkbox
                checked={allSelected}
                ref={(el) => {
                  if (el) (el as any).indeterminate = someSelected;
                }}
                onCheckedChange={(checked) => onSelectAll(!!checked)}
              />
            </TableHead>
            <TableHead className="w-12"></TableHead> {/* Icon */}
            <TableHead>Name</TableHead>
            <TableHead className="w-24">Size</TableHead>
            <TableHead className="w-32">Type</TableHead>
            <TableHead className="w-40">Schema</TableHead> {/* Phase 2.2: New column */}
            <TableHead className="w-32">Uploaded By</TableHead>
            <TableHead className="w-32">Date</TableHead>
            <TableHead className="w-32 text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {items.length === 0 ? (
            <TableRow>
              <TableCell colSpan={9} className="text-center py-12 text-muted-foreground">
                No items found
              </TableCell>
            </TableRow>
          ) : (
            items.map((item) => {
              const itemId = getItemId(item);
              const itemName = getItemName(item);
              const isItemFolder = isFolder(item);
              const selected = selectedItems.includes(itemId);

              // Folder-specific rendering
              if (isItemFolder) {
                const createdDate = new Date(item.created_at);
                const totalItems = item.file_count + item.subfolder_count;

                return (
                  <TableRow
                    key={itemId}
                    className={cn(
                      'cursor-pointer hover:bg-muted/50 transition-colors',
                      selected && 'bg-primary/5'
                    )}
                    onClick={() => onSelectItem(itemId, !selected)}
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      onFolderOpen?.(item.id);
                    }}
                  >
                    {/* Checkbox */}
                    <TableCell onClick={(e) => e.stopPropagation()}>
                      <Checkbox
                        checked={selected}
                        onCheckedChange={(checked) => onSelectItem(itemId, !!checked)}
                      />
                    </TableCell>

                    {/* Icon */}
                    <TableCell>
                      <Folder className="h-6 w-6 text-primary" />
                    </TableCell>

                    {/* Name */}
                    <TableCell>
                      <div className="font-semibold text-foreground truncate max-w-xs">
                        {item.name}
                      </div>
                    </TableCell>

                    {/* Size (item count) */}
                    <TableCell className="text-sm text-muted-foreground">
                      {totalItems} {totalItems === 1 ? 'item' : 'items'}
                    </TableCell>

                    {/* Type */}
                    <TableCell>
                      <Badge variant="outline" className="text-xs bg-primary/10 text-primary border-primary/20">
                        Folder
                      </Badge>
                    </TableCell>

                    {/* Schema */}
                    <TableCell>
                      <span className="text-xs text-muted-foreground">—</span>
                    </TableCell>

                    {/* Created By */}
                    <TableCell className="text-sm text-muted-foreground truncate">
                      {item.created_by || 'Unknown'}
                    </TableCell>

                    {/* Date */}
                    <TableCell className="text-sm text-muted-foreground">
                      {createdDate.toLocaleDateString()}
                    </TableCell>

                    {/* Actions */}
                    <TableCell className="text-right" onClick={(e) => e.stopPropagation()}>
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="outline"
                          size="sm"
                          className="h-7 px-2 text-xs"
                          onClick={(e) => {
                            e.stopPropagation();
                            onFolderOpen?.(item.id);
                          }}
                        >
                          Open
                        </Button>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm" className="h-7 w-7 p-0">
                              <MoreVertical className="h-3.5 w-3.5" />
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuItem
                              onClick={() => onDelete(itemId)}
                              className="text-destructive focus:text-destructive"
                            >
                              <Trash2 className="h-3.5 w-3.5 mr-2" />
                              Delete
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              }

              // File-specific rendering
              const uploadedDate = new Date(item.uploaded_at || Date.now());
              const icon = getFileIcon(String(item.mime_type || 'application/octet-stream'));
              const hasSchema = hasFileSchema(item);
              const schemaColumnCount = getSchemaFieldCount(item);

              return (
                <TableRow
                  key={itemId}
                  className={cn(
                    'cursor-pointer hover:bg-muted/50 transition-colors',
                    selected && 'bg-primary/5'
                  )}
                  onClick={() => onSelectItem(itemId, !selected)}
                >
                  {/* Checkbox */}
                  <TableCell onClick={(e) => e.stopPropagation()}>
                    <Checkbox
                      checked={selected}
                      onCheckedChange={(checked) => onSelectItem(itemId, !!checked)}
                    />
                  </TableCell>

                  {/* Icon */}
                  <TableCell>
                    <div className="text-2xl">{icon}</div>
                  </TableCell>

                  {/* Name */}
                  <TableCell>
                    <div className="font-medium text-foreground truncate max-w-xs">
                      {itemName}
                    </div>
                  </TableCell>

                  {/* Size */}
                  <TableCell className="text-sm text-muted-foreground">
                    {formatFileSize(Number(item.size_bytes) || 0)}
                  </TableCell>

                  {/* Type */}
                  <TableCell>
                    <Badge variant="outline" className="text-xs font-mono">
                      {getMimeTypeLabel(String(item.mime_type || 'application/octet-stream'))}
                    </Badge>
                  </TableCell>

                  {/* Phase 2.2: Schema Column */}
                  <TableCell>
                    {hasSchema ? (
                      <div className="flex items-center gap-2 flex-wrap">
                        <Badge
                          variant="outline"
                          className="text-xs bg-green-50 text-green-700 border-green-200 flex items-center gap-1"
                        >
                          <Database className="h-3 w-3" />
                          {schemaColumnCount} field{schemaColumnCount !== 1 ? 's' : ''}
                        </Badge>
                        {item.ontology_mappings && item.ontology_mappings.length > 0 && (
                          <Badge
                            variant="outline"
                            className="text-xs bg-blue-50 text-blue-700 border-blue-200 flex items-center gap-1"
                            title={`${item.ontology_mappings.length} field${item.ontology_mappings.length !== 1 ? 's' : ''} mapped to ontology`}
                          >
                            <Link2 className="h-3 w-3" />
                            {item.ontology_mappings.length}
                          </Badge>
                        )}
                      </div>
                    ) : (
                      <div className="flex items-center gap-2">
                        <Badge
                          variant="outline"
                          className="text-xs bg-amber-50 text-amber-700 border-amber-200"
                        >
                          Not profiled
                        </Badge>
                        {onProfile && (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={(e) => {
                              e.stopPropagation();
                              onProfile(itemId);
                            }}
                          >
                            <Sparkles className="h-3 w-3 mr-1" />
                            Profile
                          </Button>
                        )}
                      </div>
                    )}
                  </TableCell>

                  {/* Uploaded By */}
                  <TableCell className="text-sm text-muted-foreground truncate">
                    {String(item.uploaded_by || 'Unknown')}
                  </TableCell>

                  {/* Date */}
                  <TableCell className="text-sm text-muted-foreground">
                    {uploadedDate.toLocaleDateString()}
                  </TableCell>

                  {/* Actions */}
                  <TableCell className="text-right" onClick={(e) => e.stopPropagation()}>
                    <div className="flex items-center justify-end gap-1">
                      {/* Primary action based on schema status */}
                      {hasSchema && onUseInWorkflow ? (
                        <Button
                          variant="default"
                          size="sm"
                          className="h-7 px-2 text-xs"
                          onClick={(e) => {
                            e.stopPropagation();
                            onUseInWorkflow(itemId);
                          }}
                        >
                          <ArrowRight className="h-3 w-3 mr-1" />
                          Use in Workflow
                        </Button>
                      ) : null}

                      {/* More actions dropdown */}
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button variant="ghost" size="sm" className="h-7 w-7 p-0">
                            <MoreVertical className="h-3.5 w-3.5" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end">
                          {onView && (
                            <DropdownMenuItem onClick={() => onView(itemId)}>
                              <Eye className="h-3.5 w-3.5 mr-2" />
                              View Details
                            </DropdownMenuItem>
                          )}
                          {onDownload && (
                            <DropdownMenuItem onClick={() => onDownload(itemId)}>
                              <Download className="h-3.5 w-3.5 mr-2" />
                              Download
                            </DropdownMenuItem>
                          )}
                          {!hasSchema && onProfile && (
                            <DropdownMenuItem onClick={() => onProfile(itemId)}>
                              <Sparkles className="h-3.5 w-3.5 mr-2" />
                              Profile File
                            </DropdownMenuItem>
                          )}
                          <DropdownMenuItem
                            onClick={() => onDelete(itemId)}
                            className="text-destructive focus:text-destructive"
                          >
                            <Trash2 className="h-3.5 w-3.5 mr-2" />
                            Delete
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </div>
                  </TableCell>
                </TableRow>
              );
            })
          )}
        </TableBody>
      </Table>
    </div>
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
