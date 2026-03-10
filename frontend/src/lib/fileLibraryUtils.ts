/**
 * File Library Utilities
 * Helper functions for working with folders and files
 */

import type { FileLibraryItem, FolderItem, FileItem } from './fileLibraryTypes';

/**
 * Sort file library items: folders first (alphabetically), then files (alphabetically)
 * This is the standard file manager convention.
 */
export function sortFileLibraryItems(items: FileLibraryItem[]): FileLibraryItem[] {
  return items.sort((a, b) => {
    // 1. Folders always come before files
    if (a.type === 'folder' && b.type === 'file') return -1;
    if (a.type === 'file' && b.type === 'folder') return 1;

    // 2. Within same type, sort alphabetically by name (case-insensitive)
    const nameA = a.type === 'folder' ? a.name : (a.original_filename || a.filename);
    const nameB = b.type === 'folder' ? b.name : (b.original_filename || b.filename);

    return nameA.toLowerCase().localeCompare(nameB.toLowerCase());
  });
}

/**
 * Type guard to check if item is a folder
 */
export function isFolder(item: FileLibraryItem): item is FolderItem {
  return item.type === 'folder';
}

/**
 * Type guard to check if item is a file
 */
export function isFile(item: FileLibraryItem): item is FileItem {
  return item.type === 'file';
}

/**
 * Get item display name
 */
export function getItemName(item: FileLibraryItem): string {
  return item.type === 'folder' ? item.name : (item.original_filename || item.filename);
}

/**
 * Get item unique ID
 */
export function getItemId(item: FileLibraryItem): string {
  return item.type === 'folder' ? item.id : (item.file_id || (item as any).id);
}
