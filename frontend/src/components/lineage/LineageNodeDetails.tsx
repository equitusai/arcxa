/**
 * LineageNodeDetails Component
 * Right-side panel showing detailed information about a selected lineage node
 */

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Database,
  FileText,
  Brain,
  Calendar,
  TrendingUp,
  ChevronRight,
  X,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { LineageNode, LineageEdge } from '@/hooks/useLineageGraph';
import { format } from 'date-fns';

interface LineageNodeDetailsProps {
  node: LineageNode | null;
  relatedEdges?: LineageEdge[];
  onClose?: () => void;
  className?: string;
}

export function LineageNodeDetails({
  node,
  relatedEdges = [],
  onClose,
  className,
}: LineageNodeDetailsProps) {
  if (!node) {
    return (
      <Card className={cn('h-full flex flex-col', className)}>
        <CardContent className="flex-1 flex items-center justify-center">
          <div className="text-center text-muted-foreground text-sm">
            <FileText className="h-12 w-12 mx-auto mb-3 opacity-40" />
            <p>Select a node to view details</p>
          </div>
        </CardContent>
      </Card>
    );
  }

  const incomingEdges = relatedEdges.filter((e) => e.target === node.id);
  const outgoingEdges = relatedEdges.filter((e) => e.source === node.id);

  const getNodeIcon = () => {
    switch (node.type) {
      case 'dataset':
        return <Database className="h-5 w-5" />;
      case 'model':
        return <Brain className="h-5 w-5" />;
      case 'record':
      case 'field':
      default:
        return <FileText className="h-5 w-5" />;
    }
  };

  const getNodeTypeLabel = () => {
    switch (node.type) {
      case 'dataset':
        return 'Dataset';
      case 'model':
        return 'Model';
      case 'record':
        return 'Record';
      case 'field':
        return 'Field';
      default:
        return 'Unknown';
    }
  };

  const getConfidenceBadgeVariant = (
    confidence?: number
  ): 'default' | 'secondary' | 'destructive' | 'outline' => {
    if (!confidence) return 'secondary';
    if (confidence >= 0.9) return 'default';
    if (confidence >= 0.7) return 'secondary';
    return 'destructive';
  };

  const getConfidenceLabel = (confidence?: number): string => {
    if (!confidence) return 'Unknown';
    if (confidence >= 0.9) return 'High';
    if (confidence >= 0.7) return 'Medium';
    return 'Low';
  };

  return (
    <Card className={cn('h-full flex flex-col', className)}>
      {/* Header */}
      <CardHeader className="pb-3 space-y-0">
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-start gap-2.5 flex-1 min-w-0">
            <div
              className="p-2 rounded-md flex-shrink-0"
              style={{ backgroundColor: node.color || '#9CA1AB', opacity: 0.15 }}
            >
              {getNodeIcon()}
            </div>
            <div className="flex-1 min-w-0">
              <CardTitle className="text-sm font-semibold leading-tight truncate">
                {node.label}
              </CardTitle>
              <div className="flex items-center gap-2 mt-1">
                <Badge variant="outline" className="text-[10px] px-1.5 py-0 h-5">
                  {getNodeTypeLabel()}
                </Badge>
                {node.confidence !== undefined && (
                  <Badge
                    variant={getConfidenceBadgeVariant(node.confidence)}
                    className="text-[10px] px-1.5 py-0 h-5"
                  >
                    {getConfidenceLabel(node.confidence)} •{' '}
                    {(node.confidence * 100).toFixed(0)}%
                  </Badge>
                )}
              </div>
            </div>
          </div>
          {onClose && (
            <button
              onClick={onClose}
              className="p-1 hover:bg-muted rounded transition-colors flex-shrink-0"
            >
              <X className="h-4 w-4 text-muted-foreground" />
            </button>
          )}
        </div>
      </CardHeader>

      <Separator />

      {/* Content */}
      <ScrollArea className="flex-1">
        <CardContent className="pt-4 space-y-4">
          {/* Basic Info */}
          <div className="space-y-2">
            <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
              Basic Information
            </h4>
            <div className="space-y-1.5">
              {node.recordId && (
                <div className="flex items-start gap-2">
                  <span className="text-xs text-muted-foreground w-20 flex-shrink-0">
                    Record ID:
                  </span>
                  <span className="text-xs font-mono text-foreground break-all flex-1">
                    {node.recordId}
                  </span>
                </div>
              )}
              {node.dataset && (
                <div className="flex items-start gap-2">
                  <span className="text-xs text-muted-foreground w-20 flex-shrink-0">
                    Dataset:
                  </span>
                  <span className="text-xs text-foreground break-all flex-1">{node.dataset}</span>
                </div>
              )}
              {node.modelId && (
                <div className="flex items-start gap-2">
                  <span className="text-xs text-muted-foreground w-20 flex-shrink-0">
                    Model ID:
                  </span>
                  <span className="text-xs font-mono text-foreground break-all flex-1">
                    {node.modelId}
                  </span>
                </div>
              )}
              <div className="flex items-start gap-2">
                <span className="text-xs text-muted-foreground w-20 flex-shrink-0">
                  Timestamp:
                </span>
                <span className="text-xs text-foreground flex-1">
                  {format(new Date(node.timestamp), 'PPpp')}
                </span>
              </div>
            </div>
          </div>

          <Separator />

          {/* Confidence Progress Bar */}
          {node.confidence !== undefined && (
            <>
              <div className="space-y-2">
                <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
                  Confidence Score
                </h4>
                <div className="space-y-1.5">
                  <div className="flex items-center gap-2">
                    <TrendingUp className="h-3.5 w-3.5 text-muted-foreground" />
                    <div className="flex-1 h-2 bg-muted rounded-full overflow-hidden">
                      <div
                        className={cn(
                          'h-full rounded-full transition-all',
                          node.confidence >= 0.9
                            ? 'bg-[#107C10]'
                            : node.confidence >= 0.7
                            ? 'bg-[#FDB913]'
                            : 'bg-[#D13438]'
                        )}
                        style={{ width: `${node.confidence * 100}%` }}
                      />
                    </div>
                    <span className="text-xs font-semibold text-foreground w-10 text-right">
                      {(node.confidence * 100).toFixed(1)}%
                    </span>
                  </div>
                </div>
              </div>
              <Separator />
            </>
          )}

          {/* Incoming Dependencies */}
          {incomingEdges.length > 0 && (
            <>
              <div className="space-y-2">
                <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide flex items-center gap-1.5">
                  <ChevronRight className="h-3.5 w-3.5 rotate-180" />
                  Incoming ({incomingEdges.length})
                </h4>
                <div className="space-y-1">
                  {incomingEdges.slice(0, 5).map((edge, idx) => (
                    <div
                      key={`in-${idx}`}
                      className="flex items-center gap-2 px-2 py-1.5 bg-muted/50 rounded text-xs"
                    >
                      <Badge
                        variant="outline"
                        className="text-[9px] px-1 py-0 h-4 uppercase font-semibold"
                      >
                        {edge.operation}
                      </Badge>
                      <span className="text-muted-foreground truncate flex-1 font-mono text-[10px]">
                        {edge.source}
                      </span>
                    </div>
                  ))}
                  {incomingEdges.length > 5 && (
                    <div className="text-[10px] text-muted-foreground text-center py-1">
                      +{incomingEdges.length - 5} more
                    </div>
                  )}
                </div>
              </div>
              <Separator />
            </>
          )}

          {/* Outgoing Dependencies */}
          {outgoingEdges.length > 0 && (
            <>
              <div className="space-y-2">
                <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide flex items-center gap-1.5">
                  <ChevronRight className="h-3.5 w-3.5" />
                  Outgoing ({outgoingEdges.length})
                </h4>
                <div className="space-y-1">
                  {outgoingEdges.slice(0, 5).map((edge, idx) => (
                    <div
                      key={`out-${idx}`}
                      className="flex items-center gap-2 px-2 py-1.5 bg-muted/50 rounded text-xs"
                    >
                      <Badge
                        variant="outline"
                        className="text-[9px] px-1 py-0 h-4 uppercase font-semibold"
                      >
                        {edge.operation}
                      </Badge>
                      <span className="text-muted-foreground truncate flex-1 font-mono text-[10px]">
                        {edge.target}
                      </span>
                    </div>
                  ))}
                  {outgoingEdges.length > 5 && (
                    <div className="text-[10px] text-muted-foreground text-center py-1">
                      +{outgoingEdges.length - 5} more
                    </div>
                  )}
                </div>
              </div>
              <Separator />
            </>
          )}

          {/* Metadata */}
          {node.metadata && Object.keys(node.metadata).length > 0 && (
            <div className="space-y-2">
              <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
                Metadata
              </h4>
              <div className="space-y-1">
                {Object.entries(node.metadata).map(([key, value]) => (
                  <div key={key} className="flex items-start gap-2">
                    <span className="text-xs text-muted-foreground w-24 flex-shrink-0 break-words">
                      {key}:
                    </span>
                    <span className="text-xs font-mono text-foreground break-all flex-1">
                      {typeof value === 'object' ? JSON.stringify(value) : String(value)}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </CardContent>
      </ScrollArea>
    </Card>
  );
}
