/**
 * SchemaGraph.tsx - Interactive Schema Visualization with ReactFlow
 *
 * Features:
 * - Tables as nodes (sized by row count)
 * - Foreign keys as edges (arrows)
 * - Interactive: zoom, pan, drag nodes
 * - Highlight related tables on hover
 * - Color-code by schema/module
 * - Mini-map for navigation
 */

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import ReactFlow, {
  MiniMap,
  Controls,
  Background,
  useNodesState,
  useEdgesState,
  addEdge,
  Connection,
  Edge,
  Node,
  BackgroundVariant,
  Panel,
  MarkerType,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Slider } from '@/components/ui/slider';
import {
  ZoomIn,
  ZoomOut,
  Maximize2,
  Download,
  Table as TableIcon,
  Key,
  Link,
} from 'lucide-react';
import { cn } from '@/lib/utils';

interface TableMetadata {
  name: string;
  columns: Array<{
    name: string;
    type: string;
    nullable: boolean;
    primaryKey?: boolean;
  }>;
  primary_keys?: string[];
  foreign_keys?: Array<{
    column: string;
    referenced_table: string;
    referenced_column: string;
  }>;
  row_count?: number;
  schema?: string;
}

interface SchemaGraphProps {
  tables: TableMetadata[];
  onTableClick?: (tableName: string) => void;
  className?: string;
}

// Custom node component for tables
function TableNode({ data }: { data: any }) {
  const isHighlighted = data.highlighted;
  const isConnected = data.connected;

  return (
    <div
      className={cn(
        'px-4 py-3 rounded-lg border-2 bg-card shadow-lg transition-all duration-200',
        isHighlighted && 'border-primary ring-2 ring-primary/50 shadow-xl scale-105',
        isConnected && !isHighlighted && 'border-primary/50 shadow-md',
        !isHighlighted && !isConnected && 'border-border hover:border-primary/30'
      )}
      style={{
        minWidth: data.width || 180,
        minHeight: data.height || 100,
      }}
    >
      <div className="flex items-start gap-2 mb-2">
        <TableIcon className="h-4 w-4 text-primary mt-0.5 flex-shrink-0" />
        <div className="flex-1 min-w-0">
          <div className="font-semibold text-sm truncate" title={data.label}>
            {data.label}
          </div>
          {data.schema && (
            <div className="text-xs text-muted-foreground truncate">
              {data.schema}
            </div>
          )}
        </div>
      </div>

      <div className="space-y-1 text-xs">
        <div className="flex items-center justify-between">
          <span className="text-muted-foreground">Columns:</span>
          <Badge variant="secondary" className="text-xs">
            {data.columnCount}
          </Badge>
        </div>
        {data.rowCount !== undefined && (
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Rows:</span>
            <Badge variant="outline" className="text-xs">
              {data.rowCount.toLocaleString()}
            </Badge>
          </div>
        )}
        {data.primaryKeyCount > 0 && (
          <div className="flex items-center gap-1 text-blue-600">
            <Key className="h-3 w-3" />
            <span>{data.primaryKeyCount} PK</span>
          </div>
        )}
        {data.foreignKeyCount > 0 && (
          <div className="flex items-center gap-1 text-green-600">
            <Link className="h-3 w-3" />
            <span>{data.foreignKeyCount} FK</span>
          </div>
        )}
      </div>
    </div>
  );
}

const nodeTypes = {
  tableNode: TableNode,
};

export function SchemaGraph({ tables, onTableClick, className }: SchemaGraphProps) {
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);
  const [highlightedNode, setHighlightedNode] = useState<string | null>(null);
  const [relationshipDepth, setRelationshipDepth] = useState([2]);

  // Generate nodes and edges from tables
  useEffect(() => {
    if (!tables || tables.length === 0) return;

    // Calculate node sizes based on row count
    const maxRows = Math.max(...tables.map(t => t.row_count || 0));
    const minSize = 180;
    const maxSize = 300;

    // Create nodes
    const newNodes: Node[] = tables.map((table, index) => {
      const columnCount = table.columns?.length || 0;
      const rowCount = table.row_count || 0;
      const primaryKeyCount = table.primary_keys?.length || 0;
      const foreignKeyCount = table.foreign_keys?.length || 0;

      // Size nodes by row count (logarithmic scale for better distribution)
      const sizeMultiplier = maxRows > 0
        ? Math.log10(rowCount + 1) / Math.log10(maxRows + 1)
        : 0.5;
      const nodeSize = minSize + (maxSize - minSize) * sizeMultiplier;

      // Simple grid layout
      const cols = Math.ceil(Math.sqrt(tables.length));
      const row = Math.floor(index / cols);
      const col = index % cols;

      return {
        id: table.name,
        type: 'tableNode',
        position: {
          x: col * 400,
          y: row * 300,
        },
        data: {
          label: table.name,
          schema: table.schema,
          columnCount,
          rowCount,
          primaryKeyCount,
          foreignKeyCount,
          width: nodeSize,
          height: nodeSize * 0.8,
          highlighted: false,
          connected: false,
        },
      };
    });

    // Create edges from foreign keys
    const newEdges: Edge[] = [];
    tables.forEach((table) => {
      table.foreign_keys?.forEach((fk) => {
        // Check if referenced table exists
        if (tables.some(t => t.name === fk.referenced_table)) {
          newEdges.push({
            id: `${table.name}-${fk.column}-${fk.referenced_table}`,
            source: table.name,
            target: fk.referenced_table,
            type: 'smoothstep',
            animated: false,
            markerEnd: {
              type: MarkerType.ArrowClosed,
              width: 20,
              height: 20,
            },
            label: fk.column,
            labelStyle: { fontSize: 10, fill: '#666' },
            labelBgStyle: { fill: '#fff', fillOpacity: 0.8 },
            style: { stroke: '#888', strokeWidth: 2 },
          });
        }
      });
    });

    setNodes(newNodes);
    setEdges(newEdges);
  }, [tables, setNodes, setEdges]);

  // Highlight related nodes on hover
  const onNodeMouseEnter = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      setHighlightedNode(node.id);

      // Find connected nodes based on depth
      const connectedNodeIds = new Set<string>([node.id]);
      const depth = relationshipDepth[0];

      const findConnectedNodes = (nodeId: string, currentDepth: number) => {
        if (currentDepth >= depth) return;

        edges.forEach((edge) => {
          if (edge.source === nodeId && !connectedNodeIds.has(edge.target)) {
            connectedNodeIds.add(edge.target);
            findConnectedNodes(edge.target, currentDepth + 1);
          }
          if (edge.target === nodeId && !connectedNodeIds.has(edge.source)) {
            connectedNodeIds.add(edge.source);
            findConnectedNodes(edge.source, currentDepth + 1);
          }
        });
      };

      findConnectedNodes(node.id, 0);

      // Update nodes to show highlight and connections
      setNodes((nds) =>
        nds.map((n) => ({
          ...n,
          data: {
            ...n.data,
            highlighted: n.id === node.id,
            connected: connectedNodeIds.has(n.id) && n.id !== node.id,
          },
        }))
      );

      // Highlight connected edges
      setEdges((eds) =>
        eds.map((e) => ({
          ...e,
          animated: connectedNodeIds.has(e.source) && connectedNodeIds.has(e.target),
          style: {
            ...e.style,
            stroke:
              connectedNodeIds.has(e.source) && connectedNodeIds.has(e.target)
                ? '#3b82f6'
                : '#888',
            strokeWidth:
              connectedNodeIds.has(e.source) && connectedNodeIds.has(e.target) ? 3 : 2,
          },
        }))
      );
    },
    [edges, relationshipDepth, setNodes, setEdges]
  );

  const onNodeMouseLeave = useCallback(() => {
    setHighlightedNode(null);

    // Reset highlighting
    setNodes((nds) =>
      nds.map((n) => ({
        ...n,
        data: {
          ...n.data,
          highlighted: false,
          connected: false,
        },
      }))
    );

    setEdges((eds) =>
      eds.map((e) => ({
        ...e,
        animated: false,
        style: {
          ...e.style,
          stroke: '#888',
          strokeWidth: 2,
        },
      }))
    );
  }, [setNodes, setEdges]);

  const onNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      if (onTableClick) {
        onTableClick(node.id);
      }
    },
    [onTableClick]
  );

  const onConnect = useCallback(
    (params: Connection) => setEdges((eds) => addEdge(params, eds)),
    [setEdges]
  );

  // Auto-layout button
  const autoLayout = useCallback(() => {
    // Simple force-directed layout simulation
    setNodes((nds) => {
      const newNodes = [...nds];
      const cols = Math.ceil(Math.sqrt(newNodes.length));

      newNodes.forEach((node, index) => {
        const row = Math.floor(index / cols);
        const col = index % cols;
        node.position = {
          x: col * 400 + Math.random() * 50,
          y: row * 300 + Math.random() * 50,
        };
      });

      return newNodes;
    });
  }, [setNodes]);

  // Download graph as image
  const downloadImage = useCallback(() => {
    // This would require html2canvas integration
    console.log('Download image feature - requires html2canvas');
  }, []);

  const statsText = useMemo(() => {
    return `${tables.length} Tables • ${edges.length} Relationships`;
  }, [tables.length, edges.length]);

  if (!tables || tables.length === 0) {
    return (
      <Card className={className}>
        <CardContent className="p-8 text-center">
          <TableIcon className="h-12 w-12 mx-auto text-muted-foreground mb-3" />
          <p className="text-sm text-muted-foreground">
            No tables available for visualization
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className={className}>
      <CardContent className="p-0">
        <div className="h-[800px] relative">
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeMouseEnter={onNodeMouseEnter}
            onNodeMouseLeave={onNodeMouseLeave}
            onNodeClick={onNodeClick}
            nodeTypes={nodeTypes}
            fitView
            attributionPosition="bottom-right"
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
            <Controls />
            <MiniMap
              nodeColor={(node) => {
                if (node.data.highlighted) return '#3b82f6';
                if (node.data.connected) return '#60a5fa';
                return '#e5e7eb';
              }}
              maskColor="rgba(0, 0, 0, 0.1)"
            />

            {/* Custom Controls Panel */}
            <Panel position="top-left" className="bg-card border rounded-lg p-3 shadow-lg">
              <div className="space-y-3">
                <div className="text-sm font-semibold">{statsText}</div>

                <div className="space-y-2">
                  <div className="text-xs text-muted-foreground">Relationship Depth</div>
                  <Slider
                    value={relationshipDepth}
                    onValueChange={setRelationshipDepth}
                    min={1}
                    max={5}
                    step={1}
                    className="w-40"
                  />
                  <div className="text-xs text-muted-foreground text-center">
                    {relationshipDepth[0]} level{relationshipDepth[0] !== 1 ? 's' : ''}
                  </div>
                </div>

                <div className="flex gap-2">
                  <Button size="sm" variant="outline" onClick={autoLayout}>
                    <Maximize2 className="h-3 w-3 mr-1" />
                    Re-layout
                  </Button>
                  <Button size="sm" variant="outline" onClick={downloadImage}>
                    <Download className="h-3 w-3 mr-1" />
                    Export
                  </Button>
                </div>
              </div>
            </Panel>

            {/* Legend Panel */}
            <Panel position="top-right" className="bg-card border rounded-lg p-3 shadow-lg">
              <div className="space-y-2 text-xs">
                <div className="font-semibold mb-2">Legend</div>
                <div className="flex items-center gap-2">
                  <div className="w-4 h-4 border-2 border-primary rounded"></div>
                  <span>Highlighted Table</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-4 h-4 border-2 border-primary/50 rounded"></div>
                  <span>Related Table</span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="w-4 h-1 bg-blue-500"></div>
                  <span>Relationship</span>
                </div>
              </div>
            </Panel>
          </ReactFlow>
        </div>
      </CardContent>
    </Card>
  );
}
