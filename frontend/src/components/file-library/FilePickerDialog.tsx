/**
 * File Picker Dialog Component
 * Visual file browser for selecting files from the library
 * Replaces error-prone manual path entry in workflows
 */

import { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Card } from '@/components/ui/card';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Search,
  FileText,
  Clock,
  Database,
  CheckCircle,
  Folder,
  Filter,
  FileSpreadsheet,
  FileCode,
  X,
} from 'lucide-react';
import { useFiles, useFolders, useTags } from '@/hooks/useFileLibrary';
import { formatFileSize, getFileIcon, getFileSchema, hasFileSchema, getSchemaFieldCount } from '@/api/fileLibrary';
import type { FileMetadata } from '@/api/types';
import { cn } from '@/lib/utils';

interface FilePickerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectFile: (file: FileMetadata) => void;

  // Filtering options
  allowedMimeTypes?: string[]; // e.g., ['text/csv', 'text/tab-separated-values']
  allowedExtensions?: string[]; // e.g., ['.csv', '.tsv']
  title?: string;
  description?: string;
}

export function FilePickerDialog({
  open,
  onOpenChange,
  onSelectFile,
  allowedMimeTypes,
  allowedExtensions,
  title = 'Select File',
  description = 'Choose a file from the library',
}: FilePickerDialogProps) {
  // State
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedFolderId, setSelectedFolderId] = useState<string | undefined>();
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [selectedFile, setSelectedFile] = useState<FileMetadata | null>(null);
  const [activeTab, setActiveTab] = useState<'recent' | 'all' | 'registered'>('recent');

  // Data hooks
  const { data: filesData, isLoading: filesLoading } = useFiles({
    search: searchQuery || undefined,
    folder_id: selectedFolderId,
    tags: selectedTags.length > 0 ? selectedTags : undefined,
    sort_by: activeTab === 'recent' ? 'uploaded_at' : 'filename',
    sort_order: activeTab === 'recent' ? 'desc' : 'asc',
    limit: 50,
  });

  const { data: foldersData } = useFolders();
  const { data: tagsData } = useTags();

  // Reset state when dialog opens/closes
  useEffect(() => {
    if (!open) {
      setSelectedFile(null);
      setSearchQuery('');
      setSelectedFolderId(undefined);
      setSelectedTags([]);
      setActiveTab('recent');
    }
  }, [open]);

  // Filter files by MIME type and extension
  const filteredFiles = filesData?.files.filter((file) => {
    // Filter by MIME type
    if (allowedMimeTypes && allowedMimeTypes.length > 0) {
      const mimeMatch = allowedMimeTypes.some((mimeType) =>
        file.mime_type.includes(mimeType) || mimeType.includes(file.mime_type)
      );
      if (!mimeMatch) return false;
    }

    // Filter by extension
    if (allowedExtensions && allowedExtensions.length > 0) {
      const extMatch = allowedExtensions.some((ext) =>
        file.original_filename.toLowerCase().endsWith(ext.toLowerCase())
      );
      if (!extMatch) return false;
    }

    // Filter by registration status
    if (activeTab === 'registered' && file.registration_status !== 'registered') {
      return false;
    }

    return true;
  }) || [];

  // Group recent files (last 7 days)
  const recentFiles = filteredFiles.filter((file) => {
    const uploadedDate = new Date(file.uploaded_at);
    const now = new Date();
    const daysDiff = (now.getTime() - uploadedDate.getTime()) / (1000 * 60 * 60 * 24);
    return daysDiff <= 7;
  });

  const handleSelectFile = () => {
    if (selectedFile) {
      onSelectFile(selectedFile);
      onOpenChange(false);
    }
  };

  const handleToggleTag = (tag: string) => {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag]
    );
  };

  const formatRelativeTime = (date: Date) => {
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffHours < 1) return 'Just now';
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  };

  const getMimeTypeLabel = (mimeType: string): string => {
    const typeMap: Record<string, string> = {
      'text/csv': 'CSV',
      'text/tab-separated-values': 'TSV',
      'application/vnd.ms-excel': 'Excel',
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': 'Excel',
      'application/json': 'JSON',
      'application/xml': 'XML',
      'text/xml': 'XML',
    };
    return typeMap[mimeType] || mimeType.split('/')[1]?.toUpperCase() || 'File';
  };

  const renderFileCard = (file: FileMetadata) => {
    const isSelected = selectedFile?.file_id === file.file_id;
    const uploadedDate = new Date(file.uploaded_at);
    const icon = getFileIcon(file.mime_type);

    return (
      <Card
        key={file.file_id}
        className={cn(
          'p-3 cursor-pointer transition-all hover:border-primary hover:shadow-sm',
          isSelected && 'border-primary border-2 bg-primary/5'
        )}
        onClick={() => setSelectedFile(file)}
      >
        <div className="flex items-start gap-3">
          {/* File Icon */}
          <div className="text-2xl flex-shrink-0">{icon}</div>

          {/* File Info */}
          <div className="flex-1 min-w-0">
            <div className="flex items-start justify-between gap-2">
              <h4 className="text-sm font-medium truncate" title={file.original_filename}>
                {file.original_filename}
              </h4>
              {isSelected && <CheckCircle className="h-4 w-4 text-primary flex-shrink-0" />}
            </div>

            {/* Metadata Row */}
            <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground flex-wrap">
              <Badge variant="outline" className="text-xs">
                {getMimeTypeLabel(file.mime_type)}
              </Badge>
              <span>•</span>
              <span>{formatFileSize(file.size_bytes)}</span>
              <span>•</span>
              <div className="flex items-center gap-1">
                <Clock className="h-3 w-3" />
                <span>{formatRelativeTime(uploadedDate)}</span>
              </div>
            </div>

            {/* Registration Status */}
            {file.registration_status === 'registered' && (
              <div className="flex items-center gap-1 mt-1.5">
                <Database className="h-3 w-3 text-green-600" />
                <span className="text-xs text-green-600 font-medium">Registered datasource</span>
              </div>
            )}

            {/* Tags */}
            {file.tags && file.tags.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-2">
                {file.tags.slice(0, 3).map((tag) => (
                  <Badge key={tag} variant="secondary" className="text-xs">
                    {tag}
                  </Badge>
                ))}
                {file.tags.length > 3 && (
                  <Badge variant="secondary" className="text-xs">
                    +{file.tags.length - 3}
                  </Badge>
                )}
              </div>
            )}

            {/* Schema Preview */}
            {hasFileSchema(file) && (() => {
              const schema = getFileSchema(file);
              const fieldCount = getSchemaFieldCount(file);
              return (
                <div className="mt-2 p-2 bg-muted rounded-sm text-xs">
                  <div className="flex items-center gap-3">
                    <span className="text-muted-foreground">
                      {schema?.total_rows?.toLocaleString() || 'Unknown'} rows
                    </span>
                    <span>•</span>
                    <span className="text-muted-foreground">
                      {fieldCount} {fieldCount === 1 ? 'column' : 'columns'}
                    </span>
                  </div>
                </div>
              );
            })()}
          </div>
        </div>
      </Card>
    );
  };

  const displayFiles = activeTab === 'recent' ? recentFiles : filteredFiles;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[85vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>

        {/* Search and Filters */}
        <div className="space-y-3">
          {/* Search Bar */}
          <div className="relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search files by name..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery('')}
                className="absolute right-3 top-1/2 transform -translate-y-1/2"
              >
                <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
              </button>
            )}
          </div>

          {/* Filters Row */}
          <div className="flex gap-2 items-center flex-wrap">
            {/* Folder Filter */}
            {foldersData && foldersData.folders.length > 0 && (
              <Select value={selectedFolderId || 'all'} onValueChange={(value) => setSelectedFolderId(value === 'all' ? undefined : value)}>
                <SelectTrigger className="w-[180px] h-8 text-xs">
                  <Folder className="h-3 w-3 mr-1" />
                  <SelectValue placeholder="All folders" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All folders</SelectItem>
                  {foldersData.folders.map((folder) => (
                    <SelectItem key={folder.folder_id} value={folder.folder_id}>
                      {folder.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}

            {/* Tag Filters */}
            {tagsData && tagsData.tags.length > 0 && (
              <div className="flex items-center gap-1 flex-wrap">
                <Filter className="h-3 w-3 text-muted-foreground" />
                {tagsData.tags.slice(0, 5).map((tagStat) => (
                  <Badge
                    key={tagStat.tag}
                    variant={selectedTags.includes(tagStat.tag) ? 'default' : 'outline'}
                    className="text-xs cursor-pointer"
                    onClick={() => handleToggleTag(tagStat.tag)}
                  >
                    {tagStat.tag} ({tagStat.file_count})
                  </Badge>
                ))}
              </div>
            )}

            {/* Clear Filters */}
            {(selectedFolderId || selectedTags.length > 0 || searchQuery) && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setSearchQuery('');
                  setSelectedFolderId(undefined);
                  setSelectedTags([]);
                }}
                className="h-8 text-xs"
              >
                Clear filters
              </Button>
            )}
          </div>
        </div>

        {/* Tabs */}
        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as any)} className="flex-1 flex flex-col overflow-hidden">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="recent" className="text-xs">
              <Clock className="h-3 w-3 mr-1" />
              Recent
              {recentFiles.length > 0 && (
                <Badge variant="secondary" className="ml-1 text-xs">
                  {recentFiles.length}
                </Badge>
              )}
            </TabsTrigger>
            <TabsTrigger value="all" className="text-xs">
              <FileText className="h-3 w-3 mr-1" />
              All Files
              {filteredFiles.length > 0 && (
                <Badge variant="secondary" className="ml-1 text-xs">
                  {filteredFiles.length}
                </Badge>
              )}
            </TabsTrigger>
            <TabsTrigger value="registered" className="text-xs">
              <Database className="h-3 w-3 mr-1" />
              Registered
              {filteredFiles.filter(f => f.registration_status === 'registered').length > 0 && (
                <Badge variant="secondary" className="ml-1 text-xs">
                  {filteredFiles.filter(f => f.registration_status === 'registered').length}
                </Badge>
              )}
            </TabsTrigger>
          </TabsList>

          {/* Tab Content */}
          <ScrollArea className="flex-1 mt-4">
            <TabsContent value="recent" className="space-y-2 mt-0">
              {filesLoading ? (
                <div className="text-center py-8 text-sm text-muted-foreground">
                  Loading files...
                </div>
              ) : recentFiles.length === 0 ? (
                <div className="text-center py-8">
                  <Clock className="h-8 w-8 mx-auto mb-2 text-muted-foreground opacity-50" />
                  <p className="text-sm text-muted-foreground">
                    No recent files found
                  </p>
                  <p className="text-xs text-muted-foreground mt-1">
                    Files uploaded in the last 7 days will appear here
                  </p>
                </div>
              ) : (
                recentFiles.map((file) => renderFileCard(file))
              )}
            </TabsContent>

            <TabsContent value="all" className="space-y-2 mt-0">
              {filesLoading ? (
                <div className="text-center py-8 text-sm text-muted-foreground">
                  Loading files...
                </div>
              ) : filteredFiles.length === 0 ? (
                <div className="text-center py-8">
                  <FileText className="h-8 w-8 mx-auto mb-2 text-muted-foreground opacity-50" />
                  <p className="text-sm text-muted-foreground">
                    No files found
                  </p>
                  {(searchQuery || selectedTags.length > 0 || selectedFolderId) && (
                    <p className="text-xs text-muted-foreground mt-1">
                      Try adjusting your search or filters
                    </p>
                  )}
                </div>
              ) : (
                filteredFiles.map((file) => renderFileCard(file))
              )}
            </TabsContent>

            <TabsContent value="registered" className="space-y-2 mt-0">
              {filesLoading ? (
                <div className="text-center py-8 text-sm text-muted-foreground">
                  Loading files...
                </div>
              ) : filteredFiles.filter(f => f.registration_status === 'registered').length === 0 ? (
                <div className="text-center py-8">
                  <Database className="h-8 w-8 mx-auto mb-2 text-muted-foreground opacity-50" />
                  <p className="text-sm text-muted-foreground">
                    No registered datasources found
                  </p>
                  <p className="text-xs text-muted-foreground mt-1">
                    Register files as datasources to use them in workflows
                  </p>
                </div>
              ) : (
                filteredFiles.filter(f => f.registration_status === 'registered').map((file) => renderFileCard(file))
              )}
            </TabsContent>
          </ScrollArea>
        </Tabs>

        {/* Footer Actions */}
        <div className="flex items-center justify-between gap-3 pt-4 border-t">
          <div className="text-sm text-muted-foreground">
            {selectedFile ? (
              <div className="flex items-center gap-2">
                <CheckCircle className="h-4 w-4 text-green-600" />
                <span>Selected: <span className="font-medium">{selectedFile.original_filename}</span></span>
              </div>
            ) : (
              <span>Select a file to continue</span>
            )}
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button onClick={handleSelectFile} disabled={!selectedFile}>
              Use File
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
