/**
 * File Library Types
 * Unified types for displaying both folders and files together
 */

import type { FileMetadata } from '@/api/types';

/**
 * Folder item type
 */
export interface FolderItem {
  type: 'folder';
  id: string;
  name: string;
  path: string;
  parent_id: string | null;
  file_count: number;
  subfolder_count: number;
  created_at: string;
  updated_at: string;
  created_by?: string;
  default_ontology_id?: string; // Default ontology for files in this folder
}

/**
 * File item type (extends existing FileMetadata)
 */
export interface FileItem extends FileMetadata {
  type: 'file';
}

/**
 * Union type for displaying both folders and files
 */
export type FileLibraryItem = FolderItem | FileItem;

/**
 * Breadcrumb segment for navigation
 */
export interface BreadcrumbSegment {
  id: string;
  name: string;
  path: string;
}

/**
 * Folder contents response from API
 */
export interface FolderContentsResponse {
  folders: FolderItem[];
  files: FileItem[];
  total_items: number;
  current_path: BreadcrumbSegment[];
}
