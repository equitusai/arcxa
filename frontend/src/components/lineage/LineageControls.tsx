/**
 * LineageControls Component
 * Filter controls for lineage graph visualization
 */

import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Slider } from '@/components/ui/slider';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Filter,
  X,
  Database,
  Brain,
  TrendingUp,
  Focus,
  RotateCcw,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { LineageFilters } from '@/hooks/useLineageGraph';

interface LineageControlsProps {
  filters: LineageFilters;
  onFiltersChange: (filters: LineageFilters) => void;
  availableDatasets: string[];
  availableModels: string[];
  className?: string;
}

export function LineageControls({
  filters,
  onFiltersChange,
  availableDatasets,
  availableModels,
  className,
}: LineageControlsProps) {
  const activeFilterCount = [
    filters.confidenceRange,
    filters.timeRange,
    filters.selectedDatasets?.length,
    filters.selectedModels?.length,
    filters.focusRecordId,
  ].filter(Boolean).length;

  const handleResetFilters = () => {
    onFiltersChange({});
  };

  const handleConfidenceChange = (values: number[]) => {
    onFiltersChange({
      ...filters,
      confidenceRange: [values[0] / 100, values[1] / 100],
    });
  };

  const toggleDataset = (dataset: string) => {
    const current = filters.selectedDatasets || [];
    const updated = current.includes(dataset)
      ? current.filter((d) => d !== dataset)
      : [...current, dataset];
    onFiltersChange({
      ...filters,
      selectedDatasets: updated.length > 0 ? updated : undefined,
    });
  };

  const toggleModel = (model: string) => {
    const current = filters.selectedModels || [];
    const updated = current.includes(model)
      ? current.filter((m) => m !== model)
      : [...current, model];
    onFiltersChange({
      ...filters,
      selectedModels: updated.length > 0 ? updated : undefined,
    });
  };

  const confidenceMin = filters.confidenceRange ? filters.confidenceRange[0] * 100 : 0;
  const confidenceMax = filters.confidenceRange ? filters.confidenceRange[1] * 100 : 100;

  return (
    <Card className={cn('h-full flex flex-col', className)}>
      {/* Header */}
      <CardHeader className="pb-3 space-y-0">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Filter className="h-4 w-4 text-muted-foreground" />
            <CardTitle className="text-sm font-semibold">Filters</CardTitle>
            {activeFilterCount > 0 && (
              <Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-5">
                {activeFilterCount}
              </Badge>
            )}
          </div>
          {activeFilterCount > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={handleResetFilters}
              className="h-7 px-2 text-xs"
            >
              <RotateCcw className="h-3 w-3 mr-1" />
              Reset
            </Button>
          )}
        </div>
      </CardHeader>

      <Separator />

      {/* Content */}
      <ScrollArea className="flex-1">
        <CardContent className="pt-4 space-y-5">
          {/* Confidence Range */}
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <TrendingUp className="h-4 w-4 text-muted-foreground" />
              <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
                Confidence Range
              </h4>
            </div>
            <div className="space-y-3 px-1">
              <Slider
                min={0}
                max={100}
                step={1}
                value={[confidenceMin, confidenceMax]}
                onValueChange={handleConfidenceChange}
                className="w-full"
              />
              <div className="flex items-center justify-between text-xs">
                <div className="flex items-center gap-1">
                  <span className="text-muted-foreground">Min:</span>
                  <Badge variant="outline" className="text-[10px] px-1.5 py-0 h-5">
                    {confidenceMin.toFixed(0)}%
                  </Badge>
                </div>
                <div className="flex items-center gap-1">
                  <span className="text-muted-foreground">Max:</span>
                  <Badge variant="outline" className="text-[10px] px-1.5 py-0 h-5">
                    {confidenceMax.toFixed(0)}%
                  </Badge>
                </div>
              </div>
            </div>
          </div>

          <Separator />

          {/* Datasets Filter */}
          {availableDatasets.length > 0 && (
            <>
              <div className="space-y-2">
                <div className="flex items-center gap-2">
                  <Database className="h-4 w-4 text-muted-foreground" />
                  <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
                    Datasets
                  </h4>
                  <Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-5">
                    {filters.selectedDatasets?.length || availableDatasets.length}
                  </Badge>
                </div>
                <div className="space-y-1">
                  {availableDatasets.map((dataset) => {
                    const isSelected = filters.selectedDatasets?.includes(dataset) ?? true;
                    return (
                      <button
                        key={dataset}
                        onClick={() => toggleDataset(dataset)}
                        className={cn(
                          'w-full flex items-center gap-2 px-2 py-1.5 rounded text-xs transition-colors text-left',
                          isSelected
                            ? 'bg-accent/10 text-foreground border border-accent/20'
                            : 'bg-muted/30 text-muted-foreground hover:bg-muted/50'
                        )}
                      >
                        <div
                          className={cn(
                            'w-2 h-2 rounded-full flex-shrink-0',
                            isSelected ? 'bg-accent' : 'bg-muted-foreground/30'
                          )}
                        />
                        <span className="truncate flex-1">{dataset}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
              <Separator />
            </>
          )}

          {/* Models Filter */}
          {availableModels.length > 0 && (
            <>
              <div className="space-y-2">
                <div className="flex items-center gap-2">
                  <Brain className="h-4 w-4 text-muted-foreground" />
                  <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
                    Models
                  </h4>
                  <Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-5">
                    {filters.selectedModels?.length || availableModels.length}
                  </Badge>
                </div>
                <div className="space-y-1">
                  {availableModels.map((model) => {
                    const isSelected = filters.selectedModels?.includes(model) ?? true;
                    return (
                      <button
                        key={model}
                        onClick={() => toggleModel(model)}
                        className={cn(
                          'w-full flex items-center gap-2 px-2 py-1.5 rounded text-xs transition-colors text-left',
                          isSelected
                            ? 'bg-accent/10 text-foreground border border-accent/20'
                            : 'bg-muted/30 text-muted-foreground hover:bg-muted/50'
                        )}
                      >
                        <div
                          className={cn(
                            'w-2 h-2 rounded-full flex-shrink-0',
                            isSelected ? 'bg-accent' : 'bg-muted-foreground/30'
                          )}
                        />
                        <span className="truncate flex-1 font-mono text-[10px]">{model}</span>
                      </button>
                    );
                  })}
                </div>
              </div>
              <Separator />
            </>
          )}

          {/* Focus Mode */}
          {filters.focusRecordId && (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <Focus className="h-4 w-4 text-muted-foreground" />
                <h4 className="text-xs font-semibold text-foreground uppercase tracking-wide">
                  Focus Mode
                </h4>
              </div>
              <div className="flex items-center gap-2 px-2 py-1.5 bg-accent/10 border border-accent/20 rounded text-xs">
                <span className="flex-1 font-mono text-[10px] truncate">
                  {filters.focusRecordId}
                </span>
                <button
                  onClick={() => onFiltersChange({ ...filters, focusRecordId: undefined })}
                  className="p-0.5 hover:bg-accent/20 rounded transition-colors"
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            </div>
          )}
        </CardContent>
      </ScrollArea>
    </Card>
  );
}
