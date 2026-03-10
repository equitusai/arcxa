/**
 * File Library React Query hooks
 *
 * Provides hooks for file, folder, and tag management in the File Library
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as fileLibraryApi from '@/api/fileLibrary';
import type {
  FileListParams,
  FileUploadRequest,
  BulkImportRequest,
  FolderCreateRequest,
  RegisterFileAsDatasourceRequest,
  RegisterFileAsDatasourceResponse,
} from '@/api/types';

// ============================================================================
// File Query Hooks
// ============================================================================

/**
 * List files query hook
 *
 * Fetches files with optional filters and pagination
 *
 * @param params - List parameters (folder, tags, search, pagination)
 * @example
 * const { data, isLoading } = useFiles({ folder_id: 'abc123', page: 1 });
 */
export function useFiles(params?: FileListParams) {
  return useQuery({
    queryKey: ['file-library', 'files', params],
    queryFn: () => fileLibraryApi.listFiles(params),
    staleTime: 30 * 1000, // 30 seconds
  });
}

/**
 * Get single file metadata query hook
 *
 * Fetches detailed metadata for a specific file
 *
 * @param fileId - File ID
 * @param enabled - Whether to enable the query
 * @example
 * const { data: file } = useFile('file_abc123');
 */
export function useFile(fileId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['file-library', 'files', fileId],
    queryFn: () => fileLibraryApi.getFileMetadata(fileId!),
    enabled: enabled && !!fileId,
    staleTime: 60 * 1000, // 1 minute
  });
}

/**
 * Get file usage statistics query hook
 *
 * @param fileId - File ID
 * @example
 * const { data: stats } = useFileUsageStats('file_abc123');
 */
export function useFileUsageStats(fileId: string | undefined) {
  return useQuery({
    queryKey: ['file-library', 'files', fileId, 'usage-stats'],
    queryFn: () => fileLibraryApi.getFileUsageStats(fileId!),
    enabled: !!fileId,
    staleTime: 60 * 1000, // 1 minute
  });
}

/**
 * Get file lineage query hook
 *
 * @param fileId - File ID
 * @example
 * const { data: lineage } = useFileLineage('file_abc123');
 */
export function useFileLineage(fileId: string | undefined) {
  return useQuery({
    queryKey: ['file-library', 'files', fileId, 'lineage'],
    queryFn: () => fileLibraryApi.getFileLineage(fileId!),
    enabled: !!fileId,
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Get file impact analysis query hook
 *
 * @param fileId - File ID
 * @example
 * const { data: impact } = useFileImpactAnalysis('file_abc123');
 */
export function useFileImpactAnalysis(fileId: string | undefined) {
  return useQuery({
    queryKey: ['file-library', 'files', fileId, 'impact'],
    queryFn: () => fileLibraryApi.getFileImpactAnalysis(fileId!),
    enabled: !!fileId,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}

// ============================================================================
// File Mutation Hooks
// ============================================================================

/**
 * Upload single file mutation hook
 *
 * @example
 * const uploadFile = useUploadFile();
 * uploadFile.mutate({
 *   file: myFile,
 *   folder_id: 'folder_123',
 *   tags: ['import', 'csv']
 * });
 */
export function useUploadFile() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: FileUploadRequest) => fileLibraryApi.uploadFile(request),
    onSuccess: (data) => {
      // Invalidate files list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      // Invalidate stats
      queryClient.invalidateQueries({ queryKey: ['file-library', 'stats'] });

      toast.success('File uploaded successfully', {
        description: `${data.filename} (${fileLibraryApi.formatFileSize(data.size_bytes)})`,
      });
    },
    onError: (error: any) => {
      console.error('File upload failed:', error);
      toast.error('Failed to upload file', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Bulk upload files mutation hook
 *
 * @example
 * const bulkUpload = useBulkUploadFiles();
 * bulkUpload.mutate({
 *   files: [{ file: file1 }, { file: file2 }],
 *   folder_id: 'folder_123',
 *   common_tags: ['import']
 * });
 */
export function useBulkUploadFiles() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: BulkImportRequest) => fileLibraryApi.bulkUploadFiles(request),
    onSuccess: (data) => {
      // Invalidate files list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      // Invalidate stats
      queryClient.invalidateQueries({ queryKey: ['file-library', 'stats'] });

      toast.success('Bulk upload started', {
        description: `Uploading ${data.total_files} files (Job ID: ${data.job_id})`,
      });
    },
    onError: (error: any) => {
      console.error('Bulk upload failed:', error);
      toast.error('Failed to start bulk upload', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Get bulk import job status query hook
 *
 * @param jobId - Job ID
 * @param enabled - Whether to enable the query
 * @example
 * const { data: jobStatus } = useBulkImportStatus('job_123');
 */
export function useBulkImportStatus(jobId: string | undefined, enabled = true) {
  return useQuery({
    queryKey: ['file-library', 'jobs', jobId],
    queryFn: () => fileLibraryApi.getBulkImportStatus(jobId!),
    enabled: enabled && !!jobId,
    refetchInterval: (query) => {
      const data = query.state.data;
      // Poll every 2 seconds while processing
      if (data?.status === 'pending' || data?.status === 'processing') {
        return 2000;
      }
      // Stop polling when complete or failed
      return false;
    },
    staleTime: 0, // Always fresh for job monitoring
  });
}

/**
 * Update file metadata mutation hook
 *
 * @example
 * const updateFile = useUpdateFile();
 * updateFile.mutate({
 *   fileId: 'file_123',
 *   updates: { tags: ['processed'], folder_id: 'folder_456' }
 * });
 */
export function useUpdateFile() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ fileId, updates }: { fileId: string; updates: any }) =>
      fileLibraryApi.updateFileMetadata(fileId, updates),
    onSuccess: (data, variables) => {
      // Update file in cache
      queryClient.setQueryData(['file-library', 'files', variables.fileId], data);

      // Invalidate files list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      toast.success('File updated successfully');
    },
    onError: (error: any) => {
      console.error('File update failed:', error);
      toast.error('Failed to update file', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Delete file mutation hook
 *
 * @example
 * const deleteFile = useDeleteFile();
 * deleteFile.mutate('file_123');
 */
export function useDeleteFile() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (fileId: string) => fileLibraryApi.deleteFile(fileId),
    onSuccess: (_, fileId) => {
      // Remove file from cache
      queryClient.removeQueries({ queryKey: ['file-library', 'files', fileId] });

      // Invalidate files list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      // Invalidate stats
      queryClient.invalidateQueries({ queryKey: ['file-library', 'stats'] });

      toast.success('File deleted successfully');
    },
    onError: (error: any) => {
      console.error('File deletion failed:', error);
      toast.error('Failed to delete file', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Bulk update files mutation hook
 *
 * @example
 * const bulkUpdate = useBulkUpdateFiles();
 * bulkUpdate.mutate({
 *   fileIds: ['file_1', 'file_2'],
 *   updates: { folder_id: 'folder_123' }
 * });
 */
export function useBulkUpdateFiles() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ fileIds, updates }: { fileIds: string[]; updates: any }) =>
      fileLibraryApi.bulkUpdateFiles(fileIds, updates),
    onSuccess: (data) => {
      // Invalidate files list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      toast.success(`${data.updated_count} files updated successfully`);
    },
    onError: (error: any) => {
      console.error('Bulk update failed:', error);
      toast.error('Failed to update files', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Bulk delete files mutation hook
 *
 * @example
 * const bulkDelete = useBulkDeleteFiles();
 * bulkDelete.mutate(['file_1', 'file_2', 'file_3']);
 */
export function useBulkDeleteFiles() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (fileIds: string[]) => fileLibraryApi.bulkDeleteFiles(fileIds),
    onSuccess: (data) => {
      // Invalidate files list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      // Invalidate stats
      queryClient.invalidateQueries({ queryKey: ['file-library', 'stats'] });

      toast.success(`${data.deleted_count} files deleted successfully`);
    },
    onError: (error: any) => {
      console.error('Bulk deletion failed:', error);
      toast.error('Failed to delete files', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Folder Hooks
// ============================================================================

/**
 * List folders query hook
 *
 * @example
 * const { data: folders } = useFolders();
 */
export function useFolders() {
  return useQuery({
    queryKey: ['file-library', 'folders'],
    queryFn: () => fileLibraryApi.listFolders(),
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

/**
 * Create folder mutation hook
 *
 * @example
 * const createFolder = useCreateFolder();
 * createFolder.mutate({ name: 'Sales Data', parent_folder_id: 'folder_123' });
 */
export function useCreateFolder() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: FolderCreateRequest) => fileLibraryApi.createFolder(request),
    onSuccess: (data) => {
      // Invalidate folders list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'folders'] });

      toast.success('Folder created successfully', {
        description: data.name,
      });
    },
    onError: (error: any) => {
      console.error('Folder creation failed:', error);
      toast.error('Failed to create folder', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Update folder mutation hook
 *
 * @example
 * const updateFolder = useUpdateFolder();
 * updateFolder.mutate({
 *   folderId: 'folder_123',
 *   updates: { name: 'Updated Name' }
 * });
 */
export function useUpdateFolder() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ folderId, updates }: { folderId: string; updates: any }) =>
      fileLibraryApi.updateFolder(folderId, updates),
    onSuccess: (data) => {
      // Invalidate folders list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'folders'] });

      toast.success('Folder updated successfully');
    },
    onError: (error: any) => {
      console.error('Folder update failed:', error);
      toast.error('Failed to update folder', {
        description: error.message || 'Server error',
      });
    },
  });
}

/**
 * Delete folder mutation hook
 *
 * @example
 * const deleteFolder = useDeleteFolder();
 * deleteFolder.mutate({ folderId: 'folder_123', force: false });
 */
export function useDeleteFolder() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ folderId, force }: { folderId: string; force?: boolean }) =>
      fileLibraryApi.deleteFolder(folderId, force),
    onSuccess: (_, variables) => {
      // Invalidate folders list
      queryClient.invalidateQueries({ queryKey: ['file-library', 'folders'] });

      // Invalidate files list (folder deletion may affect files)
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      toast.success('Folder deleted successfully');
    },
    onError: (error: any) => {
      console.error('Folder deletion failed:', error);
      toast.error('Failed to delete folder', {
        description: error.message || 'Contains files (use force delete)',
      });
    },
  });
}

// ============================================================================
// Tag Hooks
// ============================================================================

/**
 * List tags query hook
 *
 * @example
 * const { data: tags } = useTags();
 */
export function useTags() {
  return useQuery({
    queryKey: ['file-library', 'tags'],
    queryFn: () => fileLibraryApi.listTags(),
    staleTime: 2 * 60 * 1000, // 2 minutes
  });
}

// ============================================================================
// Search Hooks
// ============================================================================

/**
 * Search files mutation hook
 *
 * @example
 * const searchFiles = useSearchFiles();
 * searchFiles.mutate({
 *   query: 'customer',
 *   filters: { tags: ['csv'], mime_types: ['text/csv'] }
 * });
 */
export function useSearchFiles() {
  return useMutation({
    mutationFn: (query: any) => fileLibraryApi.searchFiles(query),
    onError: (error: any) => {
      console.error('File search failed:', error);
      toast.error('Search failed', {
        description: error.message || 'Server error',
      });
    },
  });
}

// ============================================================================
// Statistics Hooks
// ============================================================================

/**
 * Get library statistics query hook
 *
 * @example
 * const { data: stats } = useLibraryStats();
 */
export function useLibraryStats() {
  return useQuery({
    queryKey: ['file-library', 'stats'],
    queryFn: () => fileLibraryApi.getLibraryStats(),
    staleTime: 60 * 1000, // 1 minute
  });
}

// ============================================================================
// File-to-Datasource Integration Hooks
// ============================================================================

/**
 * Validate file for datasource registration query hook
 *
 * Performs schema inference and checks if the file can be registered
 *
 * @param fileId - File ID to validate
 * @example
 * const { data: validation } = useValidateFileForRegistration('file_123');
 */
export function useValidateFileForRegistration(fileId: string | undefined) {
  return useQuery({
    queryKey: ['file-library', 'files', fileId, 'validate-registration'],
    queryFn: () => fileLibraryApi.validateFileForRegistration(fileId!),
    // TODO: Re-enable once backend implements /validate-registration endpoint
    enabled: false, // Disabled: Backend endpoint not yet implemented
    staleTime: 30 * 1000, // 30 seconds
  });
}

/**
 * Register file as datasource mutation hook
 *
 * Creates a datasource entry from a file and optionally imports to catalogue
 *
 * @example
 * const registerFile = useRegisterFileAsDatasource();
 * registerFile.mutate({
 *   fileId: 'file_123',
 *   request: {
 *     datasource_name: 'Customers CSV',
 *     connector_type: 'CSVFile',
 *     parsing_config: { delimiter: ',', has_header: true },
 *     import_to_catalogue: true
 *   }
 * });
 */
export function useRegisterFileAsDatasource() {
  const queryClient = useQueryClient();

  return useMutation<
    RegisterFileAsDatasourceResponse,
    Error,
    { fileId: string; request: RegisterFileAsDatasourceRequest }
  >({
    mutationFn: ({ fileId, request }) =>
      fileLibraryApi.registerFileAsDatasource(fileId, request),
    onSuccess: (data, variables) => {
      // Invalidate file queries to refresh registration status
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files', variables.fileId] });
      queryClient.invalidateQueries({ queryKey: ['file-library', 'files'] });

      // Invalidate datasources to show new file-based source
      queryClient.invalidateQueries({ queryKey: ['datasources'] });

      // If dataset created, invalidate catalogue
      if (data.dataset_id) {
        queryClient.invalidateQueries({ queryKey: ['datasets'] });
      }

      toast.success('File registered as datasource', {
        description: data.dataset_id
          ? 'Datasource created and dataset imported'
          : 'Datasource created successfully',
      });
    },
    onError: (error: any) => {
      console.error('File registration failed:', error);
      toast.error('Failed to register file as datasource', {
        description: error.message || 'Server error',
      });
    },
  });
}
