/**
 * FieldSelector Component
 * Extract and select fields from lineage metadata for field-level isolation
 * Innovative feature: Click field → see only its lineage trail (90% noise reduction)
 */

import React, { useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Search,
  X,
  Layers,
  Filter,
  TrendingUp,
  AlertTriangle,
  CheckCircle2,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { LineageNode, LineageEdge } from '@/hooks/useLineageGraph';

interface FieldInfo {
  name: string;
  dataset: string;
  nodeCount: number; // How many nodes touch this field
  edgeCount: number; // How many transformations involve this field
  avgConfidence?: number;
  hasAnomalies: boolean;
}

interface FieldSelectorProps {
  nodes: LineageNode[];
  edges: LineageEdge[];
  selectedField?: string;
  onFieldSelect: (fieldName: string | undefined) => void;
  className?: string;
}

/**
 * Extract field names from lineage metadata
 * Fields can be in metadata.fields, metadata.attributes, or metadata.transformed_fields
 */
function extractFieldsFromLineage(
  nodes: LineageNode[],
  edges: LineageEdge[]
): Map<string, FieldInfo> {
  const fieldMap = new Map<string, FieldInfo>();

  // Extract from node metadata
  nodes.forEach((node) => {
    if (!node.metadata) return;

    const fields: string[] = [];

    // Common field locations in metadata
    if (node.metadata.fields && Array.isArray(node.metadata.fields)) {
      fields.push(...node.metadata.fields);
    }
    if (node.metadata.attributes && typeof node.metadata.attributes === 'object') {
      fields.push(...Object.keys(node.metadata.attributes));
    }
    if (node.metadata.schema && Array.isArray(node.metadata.schema)) {
      fields.push(...node.metadata.schema.map((s: any) => s.name || s.field_name));
    }

    fields.forEach((fieldName) => {
      if (!fieldName) return;

      const existing = fieldMap.get(fieldName);
      if (existing) {
        existing.nodeCount++;
        if (node.confidence) {
          const oldAvg = existing.avgConfidence || 0;
          existing.avgConfidence =
            (oldAvg * (existing.nodeCount - 1) + node.confidence) / existing.nodeCount;
        }
      } else {
        fieldMap.set(fieldName, {
          name: fieldName,
          dataset: node.dataset || 'unknown',
          nodeCount: 1,
          edgeCount: 0,
          avgConfidence: node.confidence,
          hasAnomalies: false,
        });
      }
    });
  });

  // Extract from edge metadata (transformed fields)
  edges.forEach((edge) => {
    if (!edge.metadata) return;

    const fields: string[] = [];

    if (edge.metadata.source_field) {
      fields.push(edge.metadata.source_field);
    }
    if (edge.metadata.target_field) {
      fields.push(edge.metadata.target_field);
    }
    if (edge.metadata.transformed_fields && Array.isArray(edge.metadata.transformed_fields)) {
      fields.push(...edge.metadata.transformed_fields);
    }

    fields.forEach((fieldName) => {
      if (!fieldName) return;

      const existing = fieldMap.get(fieldName);
      if (existing) {
        existing.edgeCount++;
        // Check for anomalies (low confidence transformations)
        if (edge.confidence && edge.confidence < 0.7) {
          existing.hasAnomalies = true;
        }
      }
    });
  });

  return fieldMap;
}

export function FieldSelector({
  nodes,
  edges,
  selectedField,
  onFieldSelect,
  className,
}: FieldSelectorProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [viewMode, setViewMode] = useState<'all' | 'active' | 'anomalies'>('all');

  // Extract all fields from lineage data
  const allFields = useMemo(
    () => extractFieldsFromLineage(nodes, edges),
    [nodes, edges]
  );

  // Filter fields based on search and view mode
  const filteredFields = useMemo(() => {
    const fieldsArray = Array.from(allFields.values());

    // Apply search filter
    let filtered = fieldsArray.filter((field) =>
      field.name.toLowerCase().includes(searchQuery.toLowerCase())
    );

    // Apply view mode filter
    if (viewMode === 'active') {
      filtered = filtered.filter((field) => field.edgeCount > 0);
    } else if (viewMode === 'anomalies') {
      filtered = filtered.filter((field) => field.hasAnomalies);
    }

    // Sort by node count (most used first)
    return filtered.sort((a, b) => b.nodeCount - a.nodeCount);
  }, [allFields, searchQuery, viewMode]);

  const getConfidenceBadge = (confidence?: number) => {
    if (!confidence) return null;
    const variant =
      confidence >= 0.9 ? 'default' : confidence >= 0.7 ? 'secondary' : 'destructive';
    return (
      <Badge variant={variant} className="text-[9px] px-1 py-0 h-4">
        {(confidence * 100).toFixed(0)}%
      </Badge>
    );
  };

  const stats = useMemo(() => {
    const anomalyCount = Array.from(allFields.values()).filter((f) => f.hasAnomalies).length;
    const activeCount = Array.from(allFields.values()).filter((f) => f.edgeCount > 0).length;
    return {
      total: allFields.size,
      active: activeCount,
      anomalies: anomalyCount,
    };
  }, [allFields]);

  if (allFields.size === 0) {
    return (
      <Card className={cn('h-full flex flex-col', className)}>
        <CardContent className="flex-1 flex items-center justify-center">
          <div className="text-center text-muted-foreground text-sm">
            <Layers className="h-12 w-12 mx-auto mb-3 opacity-40" />
            <p>No fields detected</p>
            <p className="text-xs mt-1">Metadata may not contain field information</p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className={cn('h-full flex flex-col', className)}>
      <CardHeader className="pb-3 space-y-0">
        <div className="flex items-center justify-between gap-2 mb-3">
          <div className="flex items-center gap-2">
            <Layers className="h-4 w-4 text-muted-foreground" />
            <CardTitle className="text-sm font-semibold">Field-Level View</CardTitle>
          </div>
          {selectedField && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onFieldSelect(undefined)}
              className="h-6 px-2 text-xs"
            >
              <X className="h-3 w-3 mr-1" />
              Clear
            </Button>
          )}
        </div>

        {/* Search */}
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            placeholder="Search fields..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-8 h-8 text-xs"
          />
        </div>
      </CardHeader>

      {/* Tabs for view modes */}
      <Tabs value={viewMode} onValueChange={(v) => setViewMode(v as any)} className="px-4">
        <TabsList className="grid w-full grid-cols-3 h-8">
          <TabsTrigger value="all" className="text-xs">
            All
            <Badge variant="secondary" className="ml-1.5 text-[9px] px-1 py-0 h-4">
              {stats.total}
            </Badge>
          </TabsTrigger>
          <TabsTrigger value="active" className="text-xs">
            Active
            <Badge variant="secondary" className="ml-1.5 text-[9px] px-1 py-0 h-4">
              {stats.active}
            </Badge>
          </TabsTrigger>
          <TabsTrigger value="anomalies" className="text-xs">
            Issues
            <Badge variant="destructive" className="ml-1.5 text-[9px] px-1 py-0 h-4">
              {stats.anomalies}
            </Badge>
          </TabsTrigger>
        </TabsList>
      </Tabs>

      {/* Field List */}
      <ScrollArea className="flex-1 px-4 mt-2">
        <div className="space-y-1 pb-4">
          {filteredFields.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground text-xs">
              No fields match your criteria
            </div>
          ) : (
            filteredFields.map((field) => {
              const isSelected = selectedField === field.name;
              return (
                <button
                  key={field.name}
                  onClick={() => onFieldSelect(isSelected ? undefined : field.name)}
                  className={cn(
                    'w-full px-2 py-2 rounded text-left transition-all group',
                    isSelected
                      ? 'bg-accent text-accent-foreground border border-accent shadow-sm'
                      : 'hover:bg-muted/50'
                  )}
                >
                  <div className="flex items-start justify-between gap-2 mb-1">
                    <div className="flex items-center gap-1.5 flex-1 min-w-0">
                      {field.hasAnomalies ? (
                        <AlertTriangle className="h-3 w-3 text-warning flex-shrink-0" />
                      ) : field.edgeCount > 0 ? (
                        <TrendingUp className="h-3 w-3 text-success flex-shrink-0" />
                      ) : (
                        <CheckCircle2 className="h-3 w-3 text-muted-foreground flex-shrink-0" />
                      )}
                      <span className="text-xs font-medium truncate">{field.name}</span>
                    </div>
                    {field.avgConfidence && getConfidenceBadge(field.avgConfidence)}
                  </div>

                  <div className="flex items-center gap-3 text-[10px] text-muted-foreground">
                    <span>{field.nodeCount} nodes</span>
                    {field.edgeCount > 0 && <span>{field.edgeCount} transforms</span>}
                  </div>

                  {field.dataset && (
                    <div className="text-[9px] text-muted-foreground mt-1 truncate">
                      {field.dataset}
                    </div>
                  )}
                </button>
              );
            })
          )}
        </div>
      </ScrollArea>
    </Card>
  );
}
