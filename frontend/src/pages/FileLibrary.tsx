/**
 * File Library Page
 * Enterprise file management for CSV, Excel, and other tabular data files
 */

import React, { useState, useMemo } from 'react';
import { motion } from 'framer-motion';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  FileText,
  Plus,
  Search,
  Loader2,
  Upload,
  FolderPlus,
  Sparkles,
  Grid3x3,
  List,
  Database,
} from 'lucide-react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useMutation, useQueryClient, useQuery } from '@tanstack/react-query';
import { toast } from 'sonner';
import {
  useFiles,
  useDeleteFile,
  useDeleteFolder,
  useLibraryStats,
  useFolders,
  useCreateFolder,
} from '@/hooks/useFileLibrary';
import { FileGrid } from '@/components/file-library/FileGrid';
import { FileTable } from '@/components/file-library/FileTable';
import { FileUploadDialog } from '@/components/file-library/FileUploadDialog';
import { FileInspector } from '@/components/file-library/FileInspector';
import { Breadcrumb } from '@/components/file-library/Breadcrumb';
import { ScanOptionsDialog } from '@/components/file-library/ScanOptionsDialog';
import { formatFileSize, downloadFile, profileFile, bulkProfileFiles, getProfilingJobStatus, hasFileSchema, getSchemaFieldCount } from '@/api/fileLibrary';
import { listOntologies } from '@/api/ontology';
import { sortFileLibraryItems } from '@/lib/fileLibraryUtils';
import type { FileLibraryItem, FolderItem, BreadcrumbSegment } from '@/lib/fileLibraryTypes';

export function FileLibrary() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedItems, setSelectedItems] = useState<string[]>([]);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [itemToDelete, setItemToDelete] = useState<string | null>(null);
  const [showUploadDialog, setShowUploadDialog] = useState(false);
  const [inspectorFileId, setInspectorFileId] = useState<string | null>(null);
  const [profilingJobId, setProfilingJobId] = useState<string | null>(null);
  const [profilingProgress, setProfilingProgress] = useState<number>(0);
  const [viewMode, setViewMode] = useState<'grid' | 'table'>('grid'); // Phase 2.2: View mode toggle
  const [filterMode, setFilterMode] = useState<'all' | 'ready' | 'unprofiled'>('all'); // Phase 2.4: Filter mode
  const [showNewFolderDialog, setShowNewFolderDialog] = useState(false);
  const [newFolderName, setNewFolderName] = useState('');
  const [newFolderOntologyId, setNewFolderOntologyId] = useState<string>('none');
  const [showScanDialog, setShowScanDialog] = useState(false);
  const [filesToScan, setFilesToScan] = useState<string[]>([]);

  const navigate = useNavigate();
  const queryClient = useQueryClient();

  // Get current folder from URL
  const currentFolderId = searchParams.get('folder') || undefined;

  // Query files and folders
  const { data: filesResponse, isLoading } = useFiles({
    folder_id: currentFolderId,
    search: searchQuery || undefined
  });
  const { data: foldersResponse } = useFolders();
  const { data: stats } = useLibraryStats();
  const deleteFile = useDeleteFile();
  const deleteFolder = useDeleteFolder();
  const createFolderMutation = useCreateFolder();

  // Fetch active ontologies for folder configuration
  const { data: ontologies } = useQuery({
    queryKey: ['ontologies', 'active'],
    queryFn: () => listOntologies(true),
  });

  // QW2: Scan file mutation
  const profileFileMutation = useMutation({
    mutationFn: ({ fileId, params }: { fileId: string; params?: any }) => profileFile(fileId, params),
    onMutate: () => {
      // Phase 2.3: Show progress toast
      toast.loading('Scanning file...', {
        id: 'profile-single',
        description: 'Detecting schema and inferring field types',
      });
    },
    onSuccess: (data) => {
      // Invalidate files query to refresh the list with updated schema
      queryClient.invalidateQueries({ queryKey: ['file-library'] });

      // Phase 2.3: Show success toast with schema info
      const fieldCount = getSchemaFieldCount(data);
      toast.success('File scanned successfully', {
        id: 'profile-single',
        description: fieldCount > 0
          ? `Found ${fieldCount} field${fieldCount !== 1 ? 's' : ''}`
          : 'Schema analysis complete',
      });
    },
    onError: (error: any) => {
      // Phase 2.3: Show error toast
      toast.error('Failed to scan file', {
        id: 'profile-single',
        description: error.message || 'Please try again',
      });
    },
  });

  // QW5: Bulk scan files mutation
  const bulkProfileMutation = useMutation({
    mutationFn: ({ fileIds, params }: { fileIds: string[]; params?: any }) => bulkProfileFiles(fileIds, params),
    onMutate: (fileIds) => {
      // Phase 2.3: Show initial progress toast
      toast.loading(`Scanning ${fileIds.fileIds.length} files...`, {
        id: 'bulk-profile',
        description: 'Starting batch schema detection with auto-save',
      });
    },
    onSuccess: (data) => {
      // Start polling for job status
      setProfilingJobId(data.job_id);
      pollProfilingStatus(data.job_id);
    },
    onError: (error: any) => {
      // Phase 2.3: Show error toast
      toast.error('Failed to start bulk scanning', {
        id: 'bulk-profile',
        description: error.message || 'Please try again',
      });
    },
  });

  // Poll profiling job status
  const pollProfilingStatus = async (jobId: string) => {
    const interval = setInterval(async () => {
      try {
        const status = await getProfilingJobStatus(jobId);
        setProfilingProgress(status.progress_percent);

        // Phase 2.3: Update toast with real-time progress
        if (status.status === 'processing') {
          toast.loading(`Scanning files... ${Math.round(status.progress_percent)}%`, {
            id: 'bulk-profile',
            description: `${status.processed_files} of ${status.total_files} files processed`,
          });
        }

        // Job is complete (completed, partial, or failed)
        if (status.status === 'completed' || status.status === 'partial' || status.status === 'failed') {
          clearInterval(interval);
          setProfilingJobId(null);
          setProfilingProgress(0);

          // Phase 2.3: Show completion toast based on status
          if (status.status === 'completed') {
            toast.success('Bulk scanning complete', {
              id: 'bulk-profile',
              description: `Successfully scanned ${status.successful_files} of ${status.total_files} files`,
            });
          } else if (status.status === 'partial') {
            toast.warning('Bulk scanning partially complete', {
              id: 'bulk-profile',
              description: `${status.successful_files} succeeded, ${status.failed_files} failed`,
            });
          } else {
            toast.error('Bulk scanning failed', {
              id: 'bulk-profile',
              description: `${status.failed_files} of ${status.total_files} files failed to scan`,
            });
          }

          // Refresh files list with updated schemas
          queryClient.invalidateQueries({ queryKey: ['file-library'] });
          // Clear selection after bulk profiling
          setSelectedItems([]);
        }
      } catch (error) {
        console.error('Failed to poll profiling status:', error);
        clearInterval(interval);
        setProfilingJobId(null);
        setProfilingProgress(0);

        // Phase 2.3: Show error toast
        toast.error('Failed to check scanning status', {
          id: 'bulk-profile',
          description: 'Progress tracking interrupted',
        });
      }
    }, 2000); // Poll every 2 seconds
  };

  // Build combined items list (folders + files)
  const allFiles = filesResponse?.files || [];
  const allFolders = foldersResponse?.folders || [];
  const totalFiles = filesResponse?.total || 0;

  // Get subfolders for current folder
  const currentFolderSubfolders = useMemo(() => {
    return allFolders
      .filter(folder => folder.parent_folder_id === (currentFolderId || null))
      .map((folder): FolderItem => ({
        type: 'folder',
        id: folder.folder_id,
        name: folder.name,
        path: folder.name,
        parent_id: folder.parent_folder_id ?? null,
        file_count: folder.file_count || 0,
        subfolder_count: 0 /* subfolder_count not available */ || 0,
        created_at: folder.created_at,
        updated_at: folder.created_at,
        // created_by: not available,
      }));
  }, [allFolders, currentFolderId]);

  // Get current folder and its default ontology
  const currentFolder = useMemo(() => {
    if (!currentFolderId) return null;
    return allFolders.find(f => f.folder_id === currentFolderId) || null;
  }, [allFolders, currentFolderId]);

  const currentFolderOntologyId = currentFolder?.default_ontology_id;

  // Build breadcrumb path
  const breadcrumbPath = useMemo((): BreadcrumbSegment[] => {
    if (!currentFolderId) return [];

    const path: BreadcrumbSegment[] = [];
    let folderId: string | null = currentFolderId;

    while (folderId) {
      const folder = allFolders.find(f => f.folder_id === folderId);
      if (!folder) break;

      path.unshift({
        id: folder.folder_id,
        name: folder.name,
        path: folder.name,
      });

      folderId = folder.parent_folder_id ?? null;
    }

    return path;
  }, [allFolders, currentFolderId]);

  // Phase 2.4: Filter files based on schema status
  const filteredFiles = allFiles.filter((file) => {
    if (filterMode === 'all') return true;

    const hasSchema = hasFileSchema(file);

    if (filterMode === 'ready') return hasSchema;
    if (filterMode === 'unprofiled') return !hasSchema;

    return true;
  });

  // Combine folders and filtered files, then sort (folders-first)
  const items = useMemo((): FileLibraryItem[] => {
    const fileItems: FileLibraryItem[] = filteredFiles.map(file => ({
      ...file,
      type: 'file' as const,
    }));

    const combined = [...currentFolderSubfolders, ...fileItems];
    return sortFileLibraryItems(combined);
  }, [currentFolderSubfolders, filteredFiles]);

  // Phase 2.4: Calculate stats for filters
  const workflowReadyCount = allFiles.filter(file => hasFileSchema(file)).length;
  const unprofiledCount = allFiles.filter(file => !hasFileSchema(file)).length;

  // Folder navigation handlers
  const handleFolderOpen = (folderId: string) => {
    setSearchParams({ folder: folderId });
  };

  const handleNavigateToFolder = (folderId: string | null) => {
    if (folderId) {
      setSearchParams({ folder: folderId });
    } else {
      setSearchParams({});
    }
  };

  const handleDelete = (itemId: string) => {
    setItemToDelete(itemId);
    setShowDeleteDialog(true);
  };

  const confirmDelete = () => {
    if (!itemToDelete) return;

    // Find the item to determine if it's a folder or file
    const item = items.find(i =>
      i.type === 'folder' ? i.id === itemToDelete : (i.file_id === itemToDelete || (i as any).id === itemToDelete)
    );

    if (!item) {
      console.error('Item not found:', itemToDelete);
      setShowDeleteDialog(false);
      setItemToDelete(null);
      return;
    }

    // Delete folder or file based on type
    if (item.type === 'folder') {
      deleteFolder.mutate(
        { folderId: item.id, force: false },
        {
          onSuccess: () => {
            setShowDeleteDialog(false);
            setItemToDelete(null);
            setSelectedItems(prev => prev.filter(id => id !== itemToDelete));
          },
          onError: (error: any) => {
            // Keep dialog open on error so user can see the error message
            // The error toast is already handled by useDeleteFolder
            console.error('Folder deletion failed:', error);
          }
        }
      );
    } else {
      deleteFile.mutate(itemToDelete, {
        onSuccess: () => {
          setShowDeleteDialog(false);
          setItemToDelete(null);
          setSelectedItems(prev => prev.filter(id => id !== itemToDelete));
        },
        onError: (error: any) => {
          console.error('File deletion failed:', error);
        }
      });
    }
  };

  const handleSelectItem = (itemId: string, selected: boolean) => {
    if (selected) {
      setSelectedItems(prev => [...prev, itemId]);
    } else {
      setSelectedItems(prev => prev.filter(id => id !== itemId));
    }
  };

  const handleSelectAll = (selected: boolean) => {
    if (selected) {
      setSelectedItems(items.map(item => item.type === 'folder' ? item.id : (item.file_id || '')));
    } else {
      setSelectedItems([]);
    }
  };

  const handleView = (fileId: string) => {
    setInspectorFileId(fileId);
  };

  const handleDownload = async (fileId: string) => {
    try {
      const url = await downloadFile(fileId);
      window.open(url, '_blank');
    } catch (error) {
      console.error('Download failed:', error);
    }
  };

  // QW2: Profile file handler - Show scan options dialog
  const handleProfile = (fileId: string) => {
    setFilesToScan([fileId]);
    setShowScanDialog(true);
  };

  // Handle scan confirmation from dialog
  const handleScanConfirm = (params: any) => {
    if (filesToScan.length === 1) {
      // Single file scan
      profileFileMutation.mutate({ fileId: filesToScan[0], params });
    } else if (filesToScan.length > 1) {
      // Bulk scan
      bulkProfileMutation.mutate({ fileIds: filesToScan, params });
    }
    setFilesToScan([]);
  };

  // QW3: Navigate to workflow designer with file pre-selected
  const handleUseInWorkflow = (fileId: string) => {
    // Navigate to workflow designer and pass file context
    navigate('/workflows', {
      state: {
        addFileToWorkflow: fileId
      }
    });
  };

  // QW5: Bulk profile selected files - Show scan options dialog
  const handleBulkProfile = () => {
    if (selectedItems.length > 0) {
      setFilesToScan(selectedItems);
      setShowScanDialog(true);
    }
  };

  // Handle new folder creation
  const handleCreateFolder = () => {
    if (!newFolderName.trim()) {
      toast.error('Please enter a folder name');
      return;
    }

    createFolderMutation.mutate(
      {
        name: newFolderName,
        parent_folder_id: currentFolderId, // Create in current folder
        default_ontology_id: newFolderOntologyId !== 'none' ? newFolderOntologyId : undefined,
      },
      {
        onSuccess: () => {
          // Close dialog and reset form
          setShowNewFolderDialog(false);
          setNewFolderName('');
          setNewFolderOntologyId('none');
        },
      }
    );
  };

  return (
    <div className="container mx-auto py-6 space-y-6">
      {/* Page Header */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15 }}
        className="flex items-start justify-between pb-4 border-b-2 border-border"
      >
        <div>
          <h1 className="text-2xl font-semibold text-foreground mb-1">File Library</h1>
          <p className="text-sm text-muted-foreground">
            Centralized repository for CSV, Excel, and tabular data files
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" className="gap-2" onClick={() => setShowNewFolderDialog(true)}>
            <FolderPlus className="h-4 w-4" />
            New Folder
          </Button>
          <Button className="gap-2" onClick={() => setShowUploadDialog(true)}>
            <Upload className="h-4 w-4" />
            Upload Files
          </Button>
        </div>
      </motion.div>

      {/* Stats Cards */}
      {stats && typeof stats === 'object' && (
        <motion.div
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.05 }}
          className="grid grid-cols-1 md:grid-cols-4 gap-4"
        >
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Total Files
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{Number(stats.total_files || 0).toLocaleString()}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Total Size
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{formatFileSize(Number(stats.total_size_bytes || 0))}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Folders
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{Number(stats.folder_count || 0)}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Recent Uploads
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold text-green-600">{Number(stats.recent_uploads || 0)}</div>
              <p className="text-xs text-muted-foreground mt-1">Last 24 hours</p>
            </CardContent>
          </Card>
        </motion.div>
      )}

      {/* Search & Actions Bar */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.1 }}
        className="flex items-center gap-3"
      >
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search files by name, tags, or type..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>

        {/* Phase 2.2: View mode toggle */}
        <div className="flex items-center gap-1 border rounded-md p-1">
          <Button
            variant={viewMode === 'grid' ? 'default' : 'ghost'}
            size="sm"
            className="h-7 px-2"
            onClick={() => setViewMode('grid')}
          >
            <Grid3x3 className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant={viewMode === 'table' ? 'default' : 'ghost'}
            size="sm"
            className="h-7 px-2"
            onClick={() => setViewMode('table')}
          >
            <List className="h-3.5 w-3.5" />
          </Button>
        </div>

        {selectedItems.length > 0 && (
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            className="flex items-center gap-2"
          >
            <Badge variant="secondary" className="text-sm">
              {selectedItems.length} selected
            </Badge>
            {/* QW5: Bulk Scan Button */}
            <Button
              variant="outline"
              size="sm"
              className="gap-1.5"
              onClick={handleBulkProfile}
              disabled={bulkProfileMutation.isPending || !!profilingJobId}
            >
              {bulkProfileMutation.isPending || profilingJobId ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  Scanning... {profilingProgress > 0 && `${Math.round(profilingProgress)}%`}
                </>
              ) : (
                <>
                  <Sparkles className="h-3.5 w-3.5" />
                  Scan Selected ({selectedItems.length})
                </>
              )}
            </Button>
            <Button variant="outline" size="sm">
              Move
            </Button>
            <Button variant="outline" size="sm">
              Tag
            </Button>
            <Button variant="destructive" size="sm">
              Delete
            </Button>
          </motion.div>
        )}
      </motion.div>

      {/* Phase 2.4: Filter Tabs */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.12 }}
        className="flex items-center gap-2 border-b border-border pb-0"
      >
        <Button
          variant={filterMode === 'all' ? 'default' : 'ghost'}
          size="sm"
          className="h-9 rounded-b-none border-b-2"
          onClick={() => setFilterMode('all')}
          style={{
            borderBottomColor: filterMode === 'all' ? 'hsl(var(--primary))' : 'transparent',
          }}
        >
          All Files
          <Badge variant="secondary" className="ml-2 text-xs">
            {allFiles.length}
          </Badge>
        </Button>

        <Button
          variant={filterMode === 'ready' ? 'default' : 'ghost'}
          size="sm"
          className="h-9 rounded-b-none border-b-2 gap-2"
          onClick={() => setFilterMode('ready')}
          style={{
            borderBottomColor: filterMode === 'ready' ? 'hsl(var(--primary))' : 'transparent',
          }}
        >
          <Database className="h-3.5 w-3.5" />
          Workflow Ready
          <Badge variant="outline" className="ml-1 text-xs bg-green-50 text-green-700 border-green-200">
            {workflowReadyCount}
          </Badge>
        </Button>

        <Button
          variant={filterMode === 'unprofiled' ? 'default' : 'ghost'}
          size="sm"
          className="h-9 rounded-b-none border-b-2"
          onClick={() => setFilterMode('unprofiled')}
          style={{
            borderBottomColor: filterMode === 'unprofiled' ? 'hsl(var(--primary))' : 'transparent',
          }}
        >
          <Sparkles className="h-3.5 w-3.5 mr-1.5" />
          Needs Profiling
          <Badge variant="outline" className="ml-2 text-xs bg-amber-50 text-amber-700 border-amber-200">
            {unprofiledCount}
          </Badge>
        </Button>
      </motion.div>

      {/* Breadcrumb Navigation */}
      {breadcrumbPath.length > 0 && (
        <motion.div
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.14 }}
        >
          <Breadcrumb
            path={breadcrumbPath}
            onNavigate={handleNavigateToFolder}
          />
        </motion.div>
      )}

      {/* File Grid */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.15 }}
      >
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        ) : items.length > 0 ? (
          <>
            {/* Phase 2.2: Conditional rendering based on view mode */}
            {viewMode === 'grid' ? (
              <FileGrid
                items={items}
                selectedItems={selectedItems}
                onSelectItem={handleSelectItem}
                onSelectAll={handleSelectAll}
                onDelete={handleDelete}
                onFolderOpen={handleFolderOpen}
                onView={handleView}
                onDownload={handleDownload}
                onProfile={handleProfile}
                onUseInWorkflow={handleUseInWorkflow}
              />
            ) : (
              <FileTable
                items={items}
                selectedItems={selectedItems}
                onSelectItem={handleSelectItem}
                onSelectAll={handleSelectAll}
                onDelete={handleDelete}
                onFolderOpen={handleFolderOpen}
                onView={handleView}
                onDownload={handleDownload}
                onProfile={handleProfile}
                onUseInWorkflow={handleUseInWorkflow}
              />
            )}

            {totalFiles > filteredFiles.length && (
              <div className="text-center py-4">
                <p className="text-sm text-muted-foreground">
                  Showing {items.length} items ({filteredFiles.length} files, {currentFolderSubfolders.length} folders)
                </p>
                <Button variant="outline" size="sm" className="mt-2">
                  Load More
                </Button>
              </div>
            )}
          </>
        ) : (
          <Card>
            <CardContent className="flex flex-col items-center justify-center py-12">
              <FileText className="h-12 w-12 text-muted-foreground mb-4" />
              <p className="text-sm text-muted-foreground text-center">
                {searchQuery
                  ? 'No files found matching your search'
                  : 'No files uploaded yet'}
              </p>
              {!searchQuery && (
                <Button className="mt-4 gap-2">
                  <Plus className="h-4 w-4" />
                  Upload Your First File
                </Button>
              )}
            </CardContent>
          </Card>
        )}
      </motion.div>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {items.find(i => i.type === 'folder' ? i.id === itemToDelete : (i.file_id === itemToDelete))?.type === 'folder'
                ? 'Delete Folder'
                : 'Delete File'}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {items.find(i => i.type === 'folder' ? i.id === itemToDelete : (i.file_id === itemToDelete))?.type === 'folder'
                ? 'Are you sure you want to delete this folder? This action cannot be undone. If the folder contains files, deletion will fail unless you use force delete.'
                : 'Are you sure you want to delete this file? This action cannot be undone and may affect workflows that use this file.'}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* File Upload Dialog */}
      <FileUploadDialog
        open={showUploadDialog}
        onOpenChange={setShowUploadDialog}
      />

      {/* File Inspector */}
      <FileInspector
        fileId={inspectorFileId}
        open={!!inspectorFileId}
        onOpenChange={(open) => !open && setInspectorFileId(null)}
        onDownload={handleDownload}
        onDelete={handleDelete}
      />

      {/* New Folder Dialog */}
      <AlertDialog open={showNewFolderDialog} onOpenChange={setShowNewFolderDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Create New Folder</AlertDialogTitle>
            <AlertDialogDescription>
              Enter a name for the new folder and optionally set a default ontology for automatic field mapping.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="py-4 space-y-4">
            <div className="space-y-2">
              <Label htmlFor="folder-name">Folder Name</Label>
              <Input
                id="folder-name"
                placeholder="e.g., Customer Data"
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    handleCreateFolder();
                  }
                }}
                autoFocus
              />
            </div>

            <div className="space-y-2">
              <Label htmlFor="folder-ontology">Default Ontology (Optional)</Label>
              <Select value={newFolderOntologyId} onValueChange={setNewFolderOntologyId}>
                <SelectTrigger id="folder-ontology">
                  <SelectValue placeholder="None - Files scanned without ontology mapping" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">None</SelectItem>
                  {ontologies && ontologies.length > 0 ? (
                    ontologies.map((ontology) => (
                      <SelectItem key={ontology.id} value={ontology.id}>
                        {ontology.name || ontology.id}
                      </SelectItem>
                    ))
                  ) : (
                    <SelectItem value="__no_ontologies__" disabled>
                      No ontologies available
                    </SelectItem>
                  )}
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">
                Files scanned in this folder will automatically map to this ontology
              </p>
            </div>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => {
              setNewFolderName('');
              setNewFolderOntologyId('');
            }}>
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction onClick={handleCreateFolder}>
              Create Folder
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Scan Options Dialog */}
      <ScanOptionsDialog
        open={showScanDialog}
        onOpenChange={setShowScanDialog}
        onConfirm={handleScanConfirm}
        fileCount={filesToScan.length}
        defaultOntologyId={currentFolderOntologyId}
      />
    </div>
  );
}
