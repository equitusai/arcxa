/**
 * Breadcrumb Component
 * Navigation breadcrumb for folder hierarchy
 */

import React from 'react';
import { ChevronRight, Home, ArrowUp } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { BreadcrumbSegment } from '@/lib/fileLibraryTypes';

interface BreadcrumbProps {
  path: BreadcrumbSegment[];
  onNavigate: (folderId: string | null) => void;
}

export function Breadcrumb({ path, onNavigate }: BreadcrumbProps) {
  const hasParent = path.length > 0;

  return (
    <div className="flex items-center gap-2 py-3 px-1 border-b border-border">
      {/* Home */}
      <button
        onClick={() => onNavigate(null)}
        className="flex items-center gap-1.5 text-sm text-primary hover:underline focus:outline-none focus:ring-2 focus:ring-primary rounded px-2 py-1"
        aria-label="Navigate to root folder"
      >
        <Home className="h-4 w-4" />
        <span>Home</span>
      </button>

      {/* Path Segments */}
      {path.map((segment, idx) => {
        const isLast = idx === path.length - 1;
        return (
          <React.Fragment key={segment.id}>
            <ChevronRight className="h-4 w-4 text-muted-foreground flex-shrink-0" />
            {isLast ? (
              <span className="text-sm font-semibold text-foreground px-2 py-1">
                {segment.name}
              </span>
            ) : (
              <button
                onClick={() => onNavigate(segment.id)}
                className="text-sm text-primary hover:underline focus:outline-none focus:ring-2 focus:ring-primary rounded px-2 py-1"
              >
                {segment.name}
              </button>
            )}
          </React.Fragment>
        );
      })}

      {/* Up Level Button */}
      {hasParent && (
        <Button
          variant="outline"
          size="sm"
          className="ml-auto h-7 gap-1.5 text-xs"
          onClick={() => {
            const parentId = path.length > 1 ? path[path.length - 2].id : null;
            onNavigate(parentId);
          }}
        >
          <ArrowUp className="h-3.5 w-3.5" />
          Up Level
        </Button>
      )}
    </div>
  );
}
