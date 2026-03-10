/**
 * File Upload Dialog Component
 * Dialog for uploading single or multiple files with metadata
 */

import React, { useState, useRef } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Upload,
  X,
  FileText,
  Loader2,
  CheckCircle,
  AlertCircle,
  Sparkles,
  Info,
} from 'lucide-react';
import { useUploadFile, useFolders } from '@/hooks/useFileLibrary';
import { formatFileSize } from '@/api/fileLibrary';
import { cn } from '@/lib/utils';

interface FileUploadDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function FileUploadDialog({ open, onOpenChange }: FileUploadDialogProps) {
  const [selectedFiles, setSelectedFiles] = useState<File[]>([]);
  const [folderId, setFolderId] = useState<string>('__none__');
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState('');
  const [isDragging, setIsDragging] = useState(false);
  const [autoProfile, setAutoProfile] = useState(true); // Phase 2.1: Auto-profile toggle
  const fileInputRef = useRef<HTMLInputElement>(null);

  const uploadFile = useUploadFile();
  const { data: foldersResponse } = useFolders();
  const folders = foldersResponse?.folders || [];

  const handleFileSelect = (files: FileList | null) => {
    if (!files) return;

    const filesArray = Array.from(files);
    // Filter for supported file types
    const supportedFiles = filesArray.filter((file) => {
      const type = file.type;
      return (
        type.includes('csv') ||
        type.includes('tab-separated') ||
        type.includes('excel') ||
        type.includes('spreadsheet') ||
        type.includes('json') ||
        type.includes('xml')
      );
    });

    if (supportedFiles.length < filesArray.length) {
      const unsupported = filesArray.length - supportedFiles.length;
      console.warn(`${unsupported} unsupported file(s) filtered out`);
    }

    setSelectedFiles((prev) => [...prev, ...supportedFiles]);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    handleFileSelect(e.dataTransfer.files);
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  const handleDragLeave = () => {
    setIsDragging(false);
  };

  const removeFile = (index: number) => {
    setSelectedFiles((prev) => prev.filter((_, i) => i !== index));
  };

  const addTag = () => {
    const tag = tagInput.trim();
    if (tag && !tags.includes(tag)) {
      setTags((prev) => [...prev, tag]);
      setTagInput('');
    }
  };

  const removeTag = (tag: string) => {
    setTags((prev) => prev.filter((t) => t !== tag));
  };

  const handleUpload = async () => {
    if (selectedFiles.length === 0) return;

    try {
      // Upload files sequentially
      for (const file of selectedFiles) {
        await uploadFile.mutateAsync({
          file,
          folder_id: folderId === '__none__' ? undefined : folderId,
          tags: tags.length > 0 ? tags : undefined,
          auto_profile: autoProfile, // Phase 2.1: Auto-profile toggle
        });
      }

      // Reset and close
      setSelectedFiles([]);
      setTags([]);
      setFolderId('__none__');
      setAutoProfile(true); // Reset to default
      onOpenChange(false);
    } catch (error) {
      console.error('Upload failed:', error);
    }
  };

  const totalSize = selectedFiles.reduce((sum, file) => sum + file.size, 0);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Upload Files</DialogTitle>
          <DialogDescription>
            Upload CSV, TSV, Excel, or other tabular data files to your library
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 mt-4">
          {/* File Drop Zone */}
          <div
            onDrop={handleDrop}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            className={cn(
              'border-2 border-dashed rounded-lg p-8 text-center cursor-pointer transition-colors',
              isDragging
                ? 'border-primary bg-primary/5'
                : 'border-border hover:border-primary/50 hover:bg-muted/50'
            )}
            onClick={() => fileInputRef.current?.click()}
          >
            <input
              ref={fileInputRef}
              type="file"
              multiple
              accept=".csv,.tsv,.txt,.xlsx,.xls,.json,.xml"
              onChange={(e) => handleFileSelect(e.target.files)}
              className="hidden"
            />
            <Upload className="h-10 w-10 mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm font-medium mb-1">
              Drop files here or click to browse
            </p>
            <p className="text-xs text-muted-foreground">
              Supports CSV, TSV, Excel, JSON, XML (max 100 MB per file)
            </p>
          </div>

          {/* Selected Files List */}
          {selectedFiles.length > 0 && (
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-sm font-semibold">
                  Selected Files ({selectedFiles.length})
                </Label>
                <span className="text-xs text-muted-foreground">
                  Total: {formatFileSize(totalSize)}
                </span>
              </div>
              <div className="space-y-2 max-h-48 overflow-y-auto border rounded-md p-2">
                {selectedFiles.map((file, index) => (
                  <div
                    key={index}
                    className="flex items-center justify-between p-2 bg-muted rounded-sm group"
                  >
                    <div className="flex items-center gap-2 flex-1 min-w-0">
                      <FileText className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                      <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium truncate">{file.name}</p>
                        <p className="text-xs text-muted-foreground">
                          {formatFileSize(file.size)}
                        </p>
                      </div>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 w-7 p-0 opacity-0 group-hover:opacity-100"
                      onClick={() => removeFile(index)}
                    >
                      <X className="h-4 w-4" />
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Folder Selection */}
          <div className="space-y-2">
            <Label>Folder (Optional)</Label>
            <Select value={folderId} onValueChange={setFolderId}>
              <SelectTrigger>
                <SelectValue placeholder="Select a folder or leave empty" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">No folder (root)</SelectItem>
                {folders.map((folder) => (
                  <SelectItem key={folder.folder_id} value={folder.folder_id}>
                    {folder.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Organize files into folders for better management
            </p>
          </div>

          {/* Phase 2.1: Auto-Profile Toggle */}
          <div className="space-y-2">
            <div className="flex items-center justify-between p-4 bg-gradient-to-r from-purple-50 to-blue-50 border border-purple-200 rounded-lg">
              <div className="flex items-start gap-3 flex-1">
                <Sparkles className="h-5 w-5 text-purple-600 flex-shrink-0 mt-0.5" />
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <Label htmlFor="auto-profile" className="text-sm font-semibold text-foreground cursor-pointer">
                      Auto-Profile Files
                    </Label>
                    <Badge variant="outline" className="text-xs bg-green-50 text-green-700 border-green-200">
                      Recommended
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground mt-1">
                    Automatically infer schema (column names, types) after upload. Files will be immediately ready for use in workflows.
                  </p>
                </div>
              </div>
              <Switch
                id="auto-profile"
                checked={autoProfile}
                onCheckedChange={setAutoProfile}
                className="ml-3"
              />
            </div>
            {!autoProfile && (
              <div className="flex items-start gap-2 p-2 bg-amber-50 border border-amber-200 rounded text-xs">
                <Info className="h-3.5 w-3.5 text-amber-600 flex-shrink-0 mt-0.5" />
                <p className="text-amber-800">
                  Files without schema profiling cannot be used in workflows until manually profiled later.
                </p>
              </div>
            )}
          </div>

          {/* Tags */}
          <div className="space-y-2">
            <Label>Tags (Optional)</Label>
            <div className="flex gap-2">
              <Input
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    addTag();
                  }
                }}
                placeholder="Add tags..."
                className="flex-1"
              />
              <Button type="button" variant="outline" onClick={addTag}>
                Add
              </Button>
            </div>
            {tags.length > 0 && (
              <div className="flex flex-wrap gap-1.5 mt-2">
                {tags.map((tag) => (
                  <Badge
                    key={tag}
                    variant="secondary"
                    className="gap-1 cursor-pointer hover:bg-secondary/80"
                    onClick={() => removeTag(tag)}
                  >
                    {tag}
                    <X className="h-3 w-3" />
                  </Badge>
                ))}
              </div>
            )}
            <p className="text-xs text-muted-foreground">
              Press Enter or click Add to create tags (e.g., csv, import, staging)
            </p>
          </div>

          {/* Upload Status */}
          {uploadFile.isPending && (
            <div className="flex items-center gap-2 p-3 bg-blue-50 border border-blue-200 rounded-sm">
              <Loader2 className="h-4 w-4 animate-spin text-blue-600" />
              <p className="text-sm text-blue-900">Uploading files...</p>
            </div>
          )}

          {uploadFile.isSuccess && (
            <div className="flex items-center gap-2 p-3 bg-green-50 border border-green-200 rounded-sm">
              <CheckCircle className="h-4 w-4 text-green-600" />
              <p className="text-sm text-green-900">Upload successful!</p>
            </div>
          )}

          {uploadFile.isError && (
            <div className="flex items-center gap-2 p-3 bg-red-50 border border-red-200 rounded-sm">
              <AlertCircle className="h-4 w-4 text-red-600" />
              <p className="text-sm text-red-900">
                Upload failed: {uploadFile.error?.message}
              </p>
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2 pt-4 border-t">
            <Button
              onClick={handleUpload}
              disabled={selectedFiles.length === 0 || uploadFile.isPending}
              className="flex-1"
            >
              {uploadFile.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Uploading...
                </>
              ) : (
                <>
                  <Upload className="h-4 w-4 mr-2" />
                  Upload {selectedFiles.length} {selectedFiles.length === 1 ? 'File' : 'Files'}
                </>
              )}
            </Button>
            <Button
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={uploadFile.isPending}
            >
              Cancel
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
