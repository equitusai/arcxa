/**
 * Keyboard Shortcuts Dialog
 * Displays available keyboard shortcuts to users
 */

import React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Keyboard } from 'lucide-react';
import { Badge } from '@/components/ui/badge';

interface ShortcutRowProps {
  keys: string[];
  action: string;
}

function ShortcutRow({ keys, action }: ShortcutRowProps) {
  return (
    <div className="flex items-center justify-between py-1.5">
      <span className="text-sm text-muted-foreground">{action}</span>
      <div className="flex items-center gap-1">
        {keys.map((key, idx) => (
          <React.Fragment key={idx}>
            <kbd className="px-2 py-1 text-xs font-semibold bg-muted border border-border rounded">
              {key}
            </kbd>
            {idx < keys.length - 1 && (
              <span className="text-xs text-muted-foreground">+</span>
            )}
          </React.Fragment>
        ))}
      </div>
    </div>
  );
}

export function KeyboardShortcutsDialog() {
  const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
  const modKey = isMac ? '⌘' : 'Ctrl';

  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" className="gap-2">
          <Keyboard className="h-4 w-4" />
          Shortcuts
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Keyboard Shortcuts</DialogTitle>
          <DialogDescription>
            Speed up your workflow with these keyboard shortcuts
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-2 gap-6 mt-4">
          {/* Canvas Navigation */}
          <div>
            <h4 className="text-sm font-semibold mb-3 flex items-center gap-2">
              Canvas Navigation
              <Badge variant="outline" className="text-xs">Essential</Badge>
            </h4>
            <div className="space-y-1">
              <ShortcutRow keys={['F']} action="Fit view" />
              <ShortcutRow keys={['Space', 'Drag']} action="Pan canvas" />
              <ShortcutRow keys={[modKey, 'Scroll']} action="Zoom in/out" />
            </div>
          </div>

          {/* Editing */}
          <div>
            <h4 className="text-sm font-semibold mb-3">Editing</h4>
            <div className="space-y-1">
              <ShortcutRow keys={[modKey, 'Z']} action="Undo" />
              <ShortcutRow keys={[modKey, 'Shift', 'Z']} action="Redo" />
              <ShortcutRow keys={[modKey, 'D']} action="Duplicate" />
              <ShortcutRow keys={['Delete']} action="Delete" />
            </div>
          </div>

          {/* Selection */}
          <div>
            <h4 className="text-sm font-semibold mb-3">Selection</h4>
            <div className="space-y-1">
              <ShortcutRow keys={[modKey, 'A']} action="Select all" />
              <ShortcutRow keys={['Click', 'Drag']} action="Multi-select" />
              <ShortcutRow keys={['↑', '↓', '←', '→']} action="Nudge 8px" />
            </div>
          </div>

          {/* File Operations */}
          <div>
            <h4 className="text-sm font-semibold mb-3">File Operations</h4>
            <div className="space-y-1">
              <ShortcutRow keys={[modKey, 'S']} action="Save workflow" />
              <ShortcutRow keys={[modKey, '↵']} action="Execute workflow" />
              <ShortcutRow keys={[modKey, 'Shift', 'L']} action="Auto-layout" />
            </div>
          </div>
        </div>

        <div className="mt-6 p-3 bg-muted rounded-md">
          <p className="text-xs text-muted-foreground">
            <strong>Tip:</strong> Hold <kbd className="px-1.5 py-0.5 mx-1 text-xs bg-background border border-border rounded">Shift</kbd> while dragging to create a selection box. Click an empty area to deselect all.
          </p>
        </div>
      </DialogContent>
    </Dialog>
  );
}
