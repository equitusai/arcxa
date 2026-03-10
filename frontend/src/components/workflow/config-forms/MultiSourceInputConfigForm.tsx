/**
 * Multi-Source Input Configuration Form
 * Phase 2.1: Select and join multiple sources from Data Catalogue
 */

import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { FolderInput, Plus, Trash2, Star, Database, AlertCircle, CheckCircle2, ExternalLink } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { useQuery } from '@tanstack/react-query';
import { listUnifiedSources } from '@/api/dataCatalogue';
import { AggregationBuilder } from '../AggregationBuilder';
import type { MultiSourceInputConfig, WorkflowInputSource } from '@/lib/workflow-etl-config';
import type { UnifiedDataSource } from '@/api/dataCatalogue';
import type { Aggregation } from '../AggregationBuilder';

export interface MultiSourceInputConfigFormProps {
  config?: MultiSourceInputConfig;
  onUpdate: (updates: Partial<MultiSourceInputConfig>) => void;
  nodeId?: string;
}

export function MultiSourceInputConfigForm({ config, onUpdate }: MultiSourceInputConfigFormProps) {
  const [showSourcePicker, setShowSourcePicker] = useState(false);
  const navigate = useNavigate();

  // Fetch available sources from Data Catalogue
  const { data: catalogueData, isLoading } = useQuery({
    queryKey: ['unified-sources'],
    queryFn: () => listUnifiedSources(),
  });

  const sources = config?.sources || [];
  const primarySource = sources.find(s => s.isPrimary);
  const secondarySources = sources.filter(s => !s.isPrimary);

  // Add a new source
  const handleAddSource = (catalogueSource: UnifiedDataSource) => {
    console.log('[MultiSourceInput] Adding source:', {
      name: catalogueSource.name,
      schema_info: catalogueSource.schema_info,
      columns: catalogueSource.schema_info?.columns,
    });

    const newSource: WorkflowInputSource = {
      sourceId: catalogueSource.id,
      sourceName: catalogueSource.name,
      alias: catalogueSource.name.toLowerCase().replace(/[^a-z0-9]/g, '_'),
      isPrimary: sources.length === 0, // First source is primary by default
      schema: catalogueSource.schema_info
        ? [
            ...(catalogueSource.schema_info.columns || []).map(col => ({
              name: col,
              type: 'string',
            })),
          ]
        : undefined,
      rowCount: catalogueSource.schema_info?.row_count,
    };

    console.log('[MultiSourceInput] Created WorkflowInputSource:', {
      alias: newSource.alias,
      schema: newSource.schema,
      schemaLength: newSource.schema?.length,
    });

    onUpdate({
      sources: [...sources, newSource],
    });
    setShowSourcePicker(false);
  };

  // Remove a source
  const handleRemoveSource = (sourceId: string) => {
    const updatedSources = sources.filter(s => s.sourceId !== sourceId);

    // If we removed the primary, make the first remaining source primary
    if (updatedSources.length > 0 && !updatedSources.some(s => s.isPrimary)) {
      updatedSources[0].isPrimary = true;
    }

    onUpdate({ sources: updatedSources });
  };

  // Update a source's alias
  const handleUpdateAlias = (sourceId: string, alias: string) => {
    const updatedSources = sources.map(s =>
      s.sourceId === sourceId ? { ...s, alias } : s
    );
    onUpdate({ sources: updatedSources });
  };

  // Set a source as primary
  const handleSetPrimary = (sourceId: string) => {
    const updatedSources = sources.map(s => ({
      ...s,
      isPrimary: s.sourceId === sourceId,
      // Clear join config if becoming primary
      join: s.sourceId === sourceId ? undefined : s.join,
    }));
    onUpdate({ sources: updatedSources });
  };

  // Update join configuration
  const handleUpdateJoin = (
    sourceId: string,
    join: WorkflowInputSource['join']
  ) => {
    const updatedSources = sources.map(s =>
      s.sourceId === sourceId ? { ...s, join } : s
    );
    onUpdate({ sources: updatedSources });
  };

  // Update aggregations for a source
  const handleUpdateAggregations = (
    sourceId: string,
    aggregations: Aggregation[]
  ) => {
    const updatedSources = sources.map(s =>
      s.sourceId === sourceId
        ? {
            ...s,
            join: s.join
              ? { ...s.join, aggregations }
              : undefined,
          }
        : s
    );
    onUpdate({ sources: updatedSources });
  };

  // Get primary source schema for join field selection
  const getPrimaryFields = (): string[] => {
    console.log('[MultiSourceInput] getPrimaryFields called:', {
      hasPrimarySource: !!primarySource,
      primarySourceAlias: primarySource?.alias,
      schema: primarySource?.schema,
      schemaLength: primarySource?.schema?.length,
    });
    if (!primarySource?.schema) return [];
    return primarySource.schema.map(f => f.name);
  };

  return (
    <div className="space-y-4 p-4 max-h-[600px] overflow-y-auto">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <FolderInput className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">
          Multi-Source Input Configuration
        </h3>
      </div>

      {/* Description */}
      <div className="text-xs text-muted-foreground bg-muted/50 p-3 rounded border border-border">
        Select multiple sources from the Data Catalogue and configure joins between them.
        One source must be designated as the primary source.
      </div>

      {/* Source Picker Button */}
      {!showSourcePicker && (
        <Button
          onClick={() => setShowSourcePicker(true)}
          variant="outline"
          className="w-full gap-2"
          size="sm"
        >
          <Plus className="h-4 w-4" />
          Add Source from Data Catalogue
        </Button>
      )}

      {/* Source Picker */}
      {showSourcePicker && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Select Source</CardTitle>
            <CardDescription className="text-xs">
              Choose from available sources in the Data Catalogue
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 max-h-64 overflow-y-auto">
            {isLoading ? (
              <div className="text-xs text-muted-foreground text-center py-4">
                Loading sources...
              </div>
            ) : catalogueData?.sources && catalogueData.sources.length > 0 ? (
              catalogueData.sources
                .filter(cs => !sources.some(s => s.sourceId === cs.id))
                .map(catalogueSource => (
                  <button
                    key={catalogueSource.id}
                    onClick={() => handleAddSource(catalogueSource)}
                    className="w-full text-left p-3 bg-background hover:bg-muted border border-border rounded text-xs transition-colors"
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex-1 min-w-0">
                        <div className="font-medium text-foreground truncate">
                          {catalogueSource.name}
                        </div>
                        <div className="text-muted-foreground mt-1 flex items-center gap-2 flex-wrap">
                          <Badge variant="outline" className="text-xs">
                            {catalogueSource.type}
                          </Badge>
                          {catalogueSource.schema_info?.row_count && (
                            <span>{catalogueSource.schema_info.row_count.toLocaleString()} rows</span>
                          )}
                          {catalogueSource.schema_info?.columns && catalogueSource.schema_info.columns.length > 0 ? (
                            <Badge variant="outline" className="text-xs bg-green-50 text-green-700 border-green-200">
                              {catalogueSource.schema_info.columns.length} fields
                            </Badge>
                          ) : (
                            <Badge variant="outline" className="text-xs bg-amber-50 text-amber-700 border-amber-200">
                              No schema
                            </Badge>
                          )}
                        </div>
                      </div>
                    </div>
                  </button>
                ))
            ) : (
              <div className="text-xs text-muted-foreground text-center py-4">
                No available sources
              </div>
            )}
            <Button
              onClick={() => setShowSourcePicker(false)}
              variant="ghost"
              size="sm"
              className="w-full mt-2"
            >
              Cancel
            </Button>
          </CardContent>
        </Card>
      )}

      {/* Selected Sources */}
      {sources.length > 0 && (
        <div className="space-y-3">
          <div className="text-xs font-medium text-foreground">
            Selected Sources ({sources.length})
          </div>

          {/* Primary Source */}
          {primarySource && (
            <Card className="border-primary bg-primary/5">
              <CardHeader className="pb-2">
                <div className="flex items-start justify-between">
                  <div className="flex items-center gap-2">
                    <Star className="h-4 w-4 text-primary fill-primary" />
                    <CardTitle className="text-sm">Primary Source</CardTitle>
                  </div>
                  <Button
                    onClick={() => handleRemoveSource(primarySource.sourceId)}
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0"
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-2">
                <div className="text-xs">
                  <div className="text-muted-foreground mb-1">Source Name</div>
                  <div className="font-medium text-foreground">{primarySource.sourceName}</div>
                </div>

                <div className="space-y-1">
                  <Label htmlFor={`alias-${primarySource.sourceId}`} className="text-xs">
                    Alias
                  </Label>
                  <Input
                    id={`alias-${primarySource.sourceId}`}
                    value={primarySource.alias}
                    onChange={(e) => handleUpdateAlias(primarySource.sourceId, e.target.value)}
                    className="text-xs font-mono h-8"
                    placeholder="customers"
                  />
                </div>

                {primarySource.schema && primarySource.schema.length > 0 && (
                  <div className="text-xs">
                    <div className="text-muted-foreground mb-1">
                      Fields ({primarySource.schema.length})
                    </div>
                    <div className="flex flex-wrap gap-1">
                      {primarySource.schema.slice(0, 5).map((field, idx) => (
                        <Badge key={idx} variant="secondary" className="text-xs font-mono">
                          {field.name}
                        </Badge>
                      ))}
                      {primarySource.schema.length > 5 && (
                        <Badge variant="secondary" className="text-xs">
                          +{primarySource.schema.length - 5} more
                        </Badge>
                      )}
                    </div>
                  </div>
                )}

                {primarySource.rowCount && (
                  <div className="text-xs text-muted-foreground">
                    <Database className="h-3 w-3 inline mr-1" />
                    {primarySource.rowCount.toLocaleString()} rows
                  </div>
                )}
              </CardContent>
            </Card>
          )}

          {/* Secondary Sources with Join Configuration */}
          {secondarySources.map((source) => (
            <Card key={source.sourceId} className="border-border">
              <CardHeader className="pb-2">
                <div className="flex items-start justify-between">
                  <div className="flex items-center gap-2">
                    <CardTitle className="text-sm">{source.sourceName}</CardTitle>
                    <Button
                      onClick={() => handleSetPrimary(source.sourceId)}
                      variant="ghost"
                      size="sm"
                      className="h-6 px-2 text-xs"
                    >
                      <Star className="h-3 w-3 mr-1" />
                      Set as Primary
                    </Button>
                  </div>
                  <Button
                    onClick={() => handleRemoveSource(source.sourceId)}
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0"
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="space-y-3">
                {/* Alias */}
                <div className="space-y-1">
                  <Label htmlFor={`alias-${source.sourceId}`} className="text-xs">
                    Alias
                  </Label>
                  <Input
                    id={`alias-${source.sourceId}`}
                    value={source.alias}
                    onChange={(e) => handleUpdateAlias(source.sourceId, e.target.value)}
                    className="text-xs font-mono h-8"
                    placeholder="orders"
                  />
                </div>

                <Separator />

                {/* Join Configuration */}
                <div className="space-y-2">
                  <div className="text-xs font-medium text-foreground">
                    Join Configuration
                  </div>

                  {!primarySource ? (
                    <div className="flex items-start gap-2 p-2 bg-amber-50 border border-amber-200 rounded text-xs">
                      <AlertCircle className="h-3 w-3 text-amber-600 flex-shrink-0 mt-0.5" />
                      <div className="text-amber-800">
                        Select a primary source first
                      </div>
                    </div>
                  ) : (
                    <>
                      {/* Join Type */}
                      <div className="space-y-1">
                        <Label className="text-xs">Join Type</Label>
                        <Select
                          value={source.join?.type || 'LEFT'}
                          onValueChange={(value: 'LEFT' | 'INNER' | 'OUTER') =>
                            handleUpdateJoin(source.sourceId, {
                              ...source.join,
                              type: value,
                              localField: source.join?.localField || '',
                              foreignField: source.join?.foreignField || '',
                            })
                          }
                        >
                          <SelectTrigger className="text-xs h-8">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="LEFT">Left Join</SelectItem>
                            <SelectItem value="INNER">Inner Join</SelectItem>
                            <SelectItem value="OUTER">Full Outer Join</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>

                      {/* Join Fields */}
                      {!primarySource.schema || primarySource.schema.length === 0 || !source.schema || source.schema.length === 0 ? (
                        <div className="p-3 bg-amber-50 border border-amber-200 rounded text-xs space-y-3">
                          <div>
                            <AlertCircle className="h-4 w-4 text-amber-600 inline mr-2" />
                            <span className="text-amber-800 font-semibold">Schema information missing</span>
                          </div>
                          <div className="text-amber-700">
                            {!primarySource.schema || primarySource.schema.length === 0 ? (
                              <div>• Primary source "{primarySource.sourceName}" has no schema</div>
                            ) : null}
                            {!source.schema || source.schema.length === 0 ? (
                              <div>• Source "{source.sourceName}" has no schema</div>
                            ) : null}
                          </div>

                          {/* QW4: Action buttons for one-click recovery */}
                          <div className="flex flex-col gap-2 pt-1">
                            <Button
                              variant="outline"
                              size="sm"
                              className="w-full gap-2 bg-white hover:bg-amber-100 text-amber-800 border-amber-300"
                              onClick={() => {
                                navigate('/file-library', {
                                  state: {
                                    highlightFiles: [
                                      !primarySource.schema || primarySource.schema.length === 0 ? primarySource.sourceId : null,
                                      !source.schema || source.schema.length === 0 ? source.sourceId : null,
                                    ].filter(Boolean),
                                    action: 'profile'
                                  }
                                });
                              }}
                            >
                              <ExternalLink className="h-3.5 w-3.5" />
                              Go to File Library to Profile Files
                            </Button>
                            <div className="text-xs text-amber-600">
                              This will open the File Library where you can profile the missing files. Your workflow will be preserved.
                            </div>
                          </div>
                        </div>
                      ) : (
                        <div className="grid grid-cols-2 gap-2">
                          <div className="space-y-1">
                            <Label className="text-xs">Primary Field</Label>
                            <Select
                              value={source.join?.localField || ''}
                              onValueChange={(value) =>
                                handleUpdateJoin(source.sourceId, {
                                  ...source.join,
                                  type: source.join?.type || 'LEFT',
                                  localField: value,
                                  foreignField: source.join?.foreignField || '',
                                })
                              }
                            >
                              <SelectTrigger className="text-xs h-8 font-mono">
                                <SelectValue placeholder="Select field" />
                              </SelectTrigger>
                              <SelectContent>
                                {getPrimaryFields().map((field) => (
                                  <SelectItem key={field} value={field} className="font-mono text-xs">
                                    {field}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          </div>

                          <div className="space-y-1">
                            <Label className="text-xs">This Source Field</Label>
                            <Select
                              value={source.join?.foreignField || ''}
                              onValueChange={(value) =>
                                handleUpdateJoin(source.sourceId, {
                                  ...source.join,
                                  type: source.join?.type || 'LEFT',
                                  localField: source.join?.localField || '',
                                  foreignField: value,
                                })
                              }
                            >
                              <SelectTrigger className="text-xs h-8 font-mono">
                                <SelectValue placeholder="Select field" />
                              </SelectTrigger>
                              <SelectContent>
                                {(source.schema || []).map((field) => (
                                  <SelectItem key={field.name} value={field.name} className="font-mono text-xs">
                                    {field.name}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          </div>
                        </div>
                      )}

                      {/* Join Summary */}
                      {source.join?.localField && source.join?.foreignField && (
                        <div className="flex items-start gap-2 p-2 bg-green-50 border border-green-200 rounded text-xs">
                          <CheckCircle2 className="h-3 w-3 text-green-600 flex-shrink-0 mt-0.5" />
                          <div className="text-green-800 font-mono">
                            {primarySource.alias}.{source.join.localField} = {source.alias}.{source.join.foreignField}
                          </div>
                        </div>
                      )}
                    </>
                  )}
                </div>

                {/* Aggregations */}
                {primarySource && source.join?.localField && source.join?.foreignField && (
                  <>
                    <Separator />
                    <AggregationBuilder
                      aggregations={source.join?.aggregations || []}
                      availableFields={source.schema || []}
                      onUpdate={(aggregations) =>
                        handleUpdateAggregations(source.sourceId, aggregations)
                      }
                      sourceAlias={source.alias}
                    />
                  </>
                )}

                {/* Source Schema Preview */}
                {source.schema && source.schema.length > 0 && (
                  <>
                    <Separator />
                    <div className="text-xs">
                      <div className="text-muted-foreground mb-1">
                        Fields ({source.schema.length})
                      </div>
                      <div className="flex flex-wrap gap-1">
                        {source.schema.slice(0, 5).map((field, idx) => (
                          <Badge key={idx} variant="secondary" className="text-xs font-mono">
                            {field.name}
                          </Badge>
                        ))}
                        {source.schema.length > 5 && (
                          <Badge variant="secondary" className="text-xs">
                            +{source.schema.length - 5} more
                          </Badge>
                        )}
                      </div>
                    </div>
                  </>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {/* Validation Messages */}
      {sources.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Add at least one source from the Data Catalogue to get started
          </div>
        </div>
      )}

      {sources.length > 0 && !primarySource && (
        <div className="flex items-start gap-2 p-3 bg-red-50 border border-red-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-red-600 flex-shrink-0 mt-0.5" />
          <div className="text-red-800">
            One source must be designated as the primary source
          </div>
        </div>
      )}

      {/* Merged Schema Preview */}
      {sources.length > 1 && primarySource && (
        <Card className="bg-muted/30">
          <CardHeader className="pb-2">
            <CardTitle className="text-sm flex items-center gap-2">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              Merged Schema Preview
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              {sources.map((source) => (
                <div key={source.sourceId} className="text-xs">
                  <div className="font-medium text-foreground mb-1 flex items-center gap-2">
                    {source.isPrimary && <Star className="h-3 w-3 text-primary fill-primary" />}
                    {source.alias}
                    {source.join && (
                      <Badge variant="outline" className="text-xs">
                        {source.join.type} JOIN
                      </Badge>
                    )}
                  </div>
                  <div className="pl-4 text-muted-foreground space-y-0.5">
                    {/* Regular fields */}
                    {source.schema?.slice(0, 3).map((f, i) => (
                      <div key={i} className="font-mono">
                        {source.alias}.{f.name}
                      </div>
                    ))}
                    {source.schema && source.schema.length > 3 && (
                      <div>... +{source.schema.length - 3} more fields</div>
                    )}

                    {/* Aggregated fields */}
                    {source.join?.aggregations && source.join.aggregations.length > 0 && (
                      <>
                        <div className="text-xs text-purple-600 font-semibold mt-1">
                          Aggregated:
                        </div>
                        {source.join.aggregations.map((agg, i) => (
                          <div key={i} className="font-mono text-purple-700">
                            {source.alias}.{agg.alias} = {agg.operation}({agg.field})
                          </div>
                        ))}
                      </>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
