/**
 * FolderCard Component
 * Displays a folder in grid view with visual hierarchy and interaction
 */

import React, { useState } from 'react';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Folder, FolderOpen, Trash2, Calendar, User } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { FolderItem } from '@/lib/fileLibraryTypes';

interface FolderCardProps {
  folder: FolderItem;
  selected: boolean;
  onSelect: (selected: boolean) => void;
  onOpen: () => void;
  onDelete?: () => void;
}

export function FolderCard({
  folder,
  selected,
  onSelect,
  onOpen,
  onDelete
}: FolderCardProps) {
  const [isHovered, setIsHovered] = useState(false);
  const createdDate = new Date(folder.created_at);

  const totalItems = folder.file_count + folder.subfolder_count;

  return (
    <Card
      className={cn(
        'hover:border-primary transition-all cursor-pointer relative group h-60',
        selected && 'border-primary bg-primary/5'
      )}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('button')) return;
        onSelect(!selected);
      }}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onDoubleClick={(e) => {
        e.stopPropagation();
        onOpen();
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
          className="h-4 w-4 rounded border-gray-300 cursor-pointer"
        />
      </div>

      {/* Quick Actions (visible on hover) */}
      <div className="absolute top-3 right-3 z-10 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        {onDelete && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 text-destructive hover:text-destructive"
            title="Delete folder"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>

      <CardHeader className="pb-3">
        <div className="flex flex-col items-center justify-center pt-4">
          {/* Large Folder Icon (changes on hover) */}
          {isHovered ? (
            <FolderOpen className="h-20 w-20 text-primary mb-3 transition-all" />
          ) : (
            <Folder className="h-20 w-20 text-primary mb-3 transition-all" />
          )}

          {/* Folder Name */}
          <h3
            className="text-sm font-semibold text-foreground text-center truncate w-full px-6"
            title={folder.name}
          >
            {folder.name}
          </h3>

          {/* Item Count */}
          <p className="text-xs text-muted-foreground mt-1">
            {folder.file_count} {folder.file_count === 1 ? 'file' : 'files'}
            {folder.subfolder_count > 0 && (
              <> • {folder.subfolder_count} {folder.subfolder_count === 1 ? 'subfolder' : 'subfolders'}</>
            )}
          </p>
        </div>
      </CardHeader>

      <CardContent className="space-y-2">
        {/* Metadata */}
        <div className="space-y-1.5 text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <Calendar className="h-3 w-3 flex-shrink-0" />
            <span title={createdDate.toLocaleString()}>
              {createdDate.toLocaleDateString()}
            </span>
          </div>

          {folder.created_by && (
            <div className="flex items-center gap-1.5">
              <User className="h-3 w-3 flex-shrink-0" />
              <span className="truncate" title={folder.created_by}>
                {folder.created_by}
              </span>
            </div>
          )}
        </div>

        {/* Open Button */}
        <Button
          variant="outline"
          size="sm"
          className="w-full h-8 text-xs mt-2"
          onClick={(e) => {
            e.stopPropagation();
            onOpen();
          }}
        >
          Open Folder
        </Button>
      </CardContent>
    </Card>
  );
}
