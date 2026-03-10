/**
 * Keyboard Shortcuts Hook
 * Provides power user keyboard navigation and commands
 */

import { useEffect, useCallback } from 'react';
import { Node, Edge, useReactFlow } from 'reactflow';

export interface KeyboardShortcutsHandlers {
  onSave?: () => void;
  onUndo?: () => void;
  onRedo?: () => void;
  onDuplicate?: () => void;
  onDelete?: () => void;
  onSelectAll?: () => void;
  onAutoLayout?: () => void;
  onExecute?: () => void;
}

export function useKeyboardShortcuts(
  nodes: Node[],
  edges: Edge[],
  setNodes: (nodes: Node[] | ((nodes: Node[]) => Node[])) => void,
  setEdges: (edges: Edge[] | ((edges: Edge[]) => Edge[])) => void,
  handlers: KeyboardShortcutsHandlers = {}
) {
  const reactFlowInstance = useReactFlow();

  // Get selected nodes
  const getSelectedNodes = useCallback(() => {
    return nodes.filter(node => node.selected);
  }, [nodes]);

  // Get selected edges
  const getSelectedEdges = useCallback(() => {
    return edges.filter(edge => edge.selected);
  }, [edges]);

  // Duplicate selected nodes
  const duplicateSelectedNodes = useCallback(() => {
    if (handlers.onDuplicate) {
      handlers.onDuplicate();
      return;
    }

    const selectedNodes = getSelectedNodes();
    if (selectedNodes.length === 0) return;

    const newNodes = selectedNodes.map((node, index) => ({
      ...node,
      id: `${node.id}_copy_${Date.now()}_${index}`,
      position: {
        x: node.position.x + 50,
        y: node.position.y + 50,
      },
      selected: true,
    }));

    // Deselect old nodes and add new ones
    setNodes((nds) =>
      nds.map(n => ({ ...n, selected: false })).concat(newNodes)
    );
  }, [getSelectedNodes, setNodes, handlers]);

  // Delete selected nodes and edges
  const deleteSelected = useCallback(() => {
    if (handlers.onDelete) {
      handlers.onDelete();
      return;
    }

    const selectedNodeIds = getSelectedNodes().map(n => n.id);
    const selectedEdgeIds = getSelectedEdges().map(e => e.id);

    if (selectedNodeIds.length === 0 && selectedEdgeIds.length === 0) return;

    // Remove selected nodes
    setNodes((nds) => nds.filter(n => !selectedNodeIds.includes(n.id)));

    // Remove selected edges and edges connected to deleted nodes
    setEdges((eds) =>
      eds.filter(
        e =>
          !selectedEdgeIds.includes(e.id) &&
          !selectedNodeIds.includes(e.source) &&
          !selectedNodeIds.includes(e.target)
      )
    );
  }, [getSelectedNodes, getSelectedEdges, setNodes, setEdges, handlers]);

  // Select all nodes
  const selectAll = useCallback(() => {
    if (handlers.onSelectAll) {
      handlers.onSelectAll();
      return;
    }

    setNodes((nds) => nds.map(n => ({ ...n, selected: true })));
  }, [setNodes, handlers]);

  // Nudge selected nodes
  const nudgeSelectedNodes = useCallback((direction: 'up' | 'down' | 'left' | 'right') => {
    const delta = { x: 0, y: 0 };
    const step = 8; // Grid snap size

    switch (direction) {
      case 'up':
        delta.y = -step;
        break;
      case 'down':
        delta.y = step;
        break;
      case 'left':
        delta.x = -step;
        break;
      case 'right':
        delta.x = step;
        break;
    }

    setNodes((nds) =>
      nds.map(node =>
        node.selected
          ? {
              ...node,
              position: {
                x: node.position.x + delta.x,
                y: node.position.y + delta.y,
              },
            }
          : node
      )
    );
  }, [setNodes]);

  // Fit view
  const fitView = useCallback(() => {
    if (reactFlowInstance) {
      reactFlowInstance.fitView({ padding: 0.2, duration: 200 });
    }
  }, [reactFlowInstance]);

  // Main keyboard event handler
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const isMod = e.metaKey || e.ctrlKey;

      // Cmd/Ctrl + S: Save
      if (isMod && e.key === 's') {
        e.preventDefault();
        if (handlers.onSave) {
          handlers.onSave();
        }
        return;
      }

      // Cmd/Ctrl + Z: Undo
      if (isMod && e.key === 'z' && !e.shiftKey) {
        e.preventDefault();
        if (handlers.onUndo) {
          handlers.onUndo();
        }
        return;
      }

      // Cmd/Ctrl + Shift + Z: Redo
      if (isMod && e.key === 'z' && e.shiftKey) {
        e.preventDefault();
        if (handlers.onRedo) {
          handlers.onRedo();
        }
        return;
      }

      // Cmd/Ctrl + D: Duplicate
      if (isMod && e.key === 'd') {
        e.preventDefault();
        duplicateSelectedNodes();
        return;
      }

      // Delete/Backspace: Delete selected
      if (e.key === 'Delete' || e.key === 'Backspace') {
        // Only if not in an input field
        const target = e.target as HTMLElement;
        if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
          return;
        }
        e.preventDefault();
        deleteSelected();
        return;
      }

      // Cmd/Ctrl + A: Select all
      if (isMod && e.key === 'a') {
        e.preventDefault();
        selectAll();
        return;
      }

      // Cmd/Ctrl + Shift + L: Auto-layout
      if (isMod && e.shiftKey && e.key === 'l') {
        e.preventDefault();
        if (handlers.onAutoLayout) {
          handlers.onAutoLayout();
        }
        return;
      }

      // Cmd/Ctrl + Enter: Execute workflow
      if (isMod && e.key === 'Enter') {
        e.preventDefault();
        if (handlers.onExecute) {
          handlers.onExecute();
        }
        return;
      }

      // F: Fit view
      if (e.key === 'f' && !isMod) {
        e.preventDefault();
        fitView();
        return;
      }

      // Arrow keys: Nudge selected nodes
      if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
        const selectedCount = getSelectedNodes().length;
        if (selectedCount === 0) return;

        e.preventDefault();

        const directionMap: Record<string, 'up' | 'down' | 'left' | 'right'> = {
          ArrowUp: 'up',
          ArrowDown: 'down',
          ArrowLeft: 'left',
          ArrowRight: 'right',
        };

        nudgeSelectedNodes(directionMap[e.key]);
        return;
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [
    nodes,
    edges,
    handlers,
    duplicateSelectedNodes,
    deleteSelected,
    selectAll,
    nudgeSelectedNodes,
    fitView,
    getSelectedNodes,
  ]);

  return {
    duplicateSelectedNodes,
    deleteSelected,
    selectAll,
    nudgeSelectedNodes,
    fitView,
  };
}
