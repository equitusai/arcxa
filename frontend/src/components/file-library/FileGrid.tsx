/**
 * File Grid Component
 * Displays files and folders in a responsive grid layout with selection support
 */

import React from 'react';
import { motion } from 'framer-motion';
import { FileCard } from './FileCard';
import { FolderCard } from './FolderCard';
import type { FileMetadata } from '@/api/types';
import type { FileLibraryItem } from '@/lib/fileLibraryTypes';
import { isFolder, getItemId } from '@/lib/fileLibraryUtils';

export interface FileGridProps {
  items: FileLibraryItem[];
  selectedItems: string[];
  onSelectItem: (itemId: string, selected: boolean) => void;
  onSelectAll: (selected: boolean) => void;
  onDelete: (itemId: string) => void;
  onFolderOpen?: (folderId: string) => void;
  onView?: (fileId: string) => void;
  onDownload?: (fileId: string) => void;
  onProfile?: (fileId: string) => void; // QW2: Profile file callback
  onUseInWorkflow?: (fileId: string) => void; // QW3: Use in workflow callback
}

export function FileGrid({
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
}: FileGridProps) {
  const allSelected = items.length > 0 && selectedItems.length === items.length;
  const someSelected = selectedItems.length > 0 && !allSelected;

  return (
    <div className="space-y-4">
      {/* Selection Header */}
      {items.length > 0 && (
        <div className="flex items-center gap-2 px-1">
          <label className="flex items-center gap-2 text-sm text-muted-foreground cursor-pointer">
            <input
              type="checkbox"
              checked={allSelected}
              ref={(el) => {
                if (el) el.indeterminate = someSelected;
              }}
              onChange={(e) => onSelectAll(e.target.checked)}
              className="h-4 w-4 rounded border-gray-300"
            />
            {allSelected ? 'Deselect all' : someSelected ? `${selectedItems.length} selected` : 'Select all'}
          </label>
        </div>
      )}

      {/* Items Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
        {items.map((item, idx) => {
          const itemId = getItemId(item);

          return (
            <motion.div
              key={itemId}
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              transition={{ duration: 0.15, delay: idx * 0.02 }}
            >
              {isFolder(item) ? (
                <FolderCard
                  folder={item}
                  selected={selectedItems.includes(itemId)}
                  onSelect={(selected) => onSelectItem(itemId, selected)}
                  onOpen={() => onFolderOpen?.(item.id)}
                  onDelete={() => onDelete(itemId)}
                />
              ) : (
                <FileCard
                  file={item}
                  selected={selectedItems.includes(itemId)}
                  onSelect={(selected) => onSelectItem(itemId, selected)}
                  onDelete={() => onDelete(itemId)}
                  onView={onView ? () => onView(itemId) : undefined}
                  onDownload={onDownload ? () => onDownload(itemId) : undefined}
                  onProfile={onProfile ? () => onProfile(itemId) : undefined}
                  onUseInWorkflow={onUseInWorkflow ? () => onUseInWorkflow(itemId) : undefined}
                />
              )}
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}
