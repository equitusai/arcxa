/**
 * Field Transformer Configuration Form
 *
 * Enterprise-grade UI for configuring field-level transformations
 * Design: Oracle Redwood + Microsoft Fluent (Graphica Design System)
 *
 * Features:
 * - Visual field selection from upstream schema
 * - Multi-operation transformation pipeline with drag-and-drop
 * - Operation-specific parameter builders
 * - Real-time preview with sample data
 * - Validation and error handling
 */

import React, { useState, useMemo } from 'react';
import {
  Wand2,
  Plus,
  AlertCircle,
  Play,
  ChevronRight,
  GripVertical,
  Trash2,
  Search,
  Check,
  X,
  Eye,
  Sparkles,
  Code2,
} from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { FieldTransformerConfig, FieldTransformation } from '@/lib/workflow-etl-config';
import { OperationBuilder } from './field-transformer/OperationBuilder';
import { TransformationPreview } from './field-transformer/TransformationPreview';
import { FieldSelector } from './field-transformer/FieldSelector';
import { TransformationPipeline } from './field-transformer/TransformationPipeline';

export interface FieldTransformerConfigFormProps {
  config?: FieldTransformerConfig;
  onUpdate: (updates: Partial<FieldTransformerConfig>) => void;
  nodeId?: string;
  /** Schema from upstream node (auto-detected via React Flow) */
  upstreamSchema?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
  }>;
}

export function FieldTransformerConfigForm({
  config,
  onUpdate,
  nodeId,
  upstreamSchema = [],
}: FieldTransformerConfigFormProps) {
  const transformations = config?.transformations || [];

  // UI state
  const [selectedTransformIndex, setSelectedTransformIndex] = useState<number | null>(null);
  const [activeTab, setActiveTab] = useState<'configure' | 'preview'>('configure');
  const [searchQuery, setSearchQuery] = useState('');

  // Filtered available fields (exclude already-transformed fields)
  const transformedFieldNames = new Set(transformations.map((t) => t.field));
  const availableFields = useMemo(() => {
    return upstreamSchema.filter((field) => {
      const matchesSearch = field.name.toLowerCase().includes(searchQuery.toLowerCase());
      const notTransformed = !transformedFieldNames.has(field.name);
      return matchesSearch && notTransformed;
    });
  }, [upstreamSchema, transformedFieldNames, searchQuery]);

  // Add new transformation for a field
  const handleAddTransformation = (fieldName: string) => {
    const newTransformation: FieldTransformation = {
      field: fieldName,
      operations: [],
    };

    onUpdate({
      transformations: [...transformations, newTransformation],
    });

    // Select the newly added transformation
    setSelectedTransformIndex(transformations.length);
    setActiveTab('configure');
  };

  // Update specific transformation
  const handleUpdateTransformation = (index: number, updates: Partial<FieldTransformation>) => {
    const updated = [...transformations];
    updated[index] = { ...updated[index], ...updates };
    onUpdate({ transformations: updated });
  };

  // Delete transformation
  const handleDeleteTransformation = (index: number) => {
    const updated = transformations.filter((_, i) => i !== index);
    onUpdate({ transformations: updated });

    // Adjust selection
    if (selectedTransformIndex === index) {
      setSelectedTransformIndex(null);
    } else if (selectedTransformIndex !== null && selectedTransformIndex > index) {
      setSelectedTransformIndex(selectedTransformIndex - 1);
    }
  };

  // Reorder transformations
  const handleReorderTransformations = (fromIndex: number, toIndex: number) => {
    const updated = [...transformations];
    const [moved] = updated.splice(fromIndex, 1);
    updated.splice(toIndex, 0, moved);
    onUpdate({ transformations: updated });

    // Adjust selection
    if (selectedTransformIndex === fromIndex) {
      setSelectedTransformIndex(toIndex);
    }
  };

  const selectedTransformation =
    selectedTransformIndex !== null ? transformations[selectedTransformIndex] : null;

  const hasSchema = upstreamSchema.length > 0;

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border bg-white dark:bg-neutral-900">
        <Wand2 className="w-4 h-4 text-green-600" />
        <h3 className="text-sm font-semibold text-foreground">Field Transformer</h3>
        {transformations.length > 0 && (
          <Badge variant="secondary" className="ml-auto text-xs">
            {transformations.length} field{transformations.length !== 1 ? 's' : ''}
          </Badge>
        )}
      </div>

      {/* No upstream schema warning */}
      {!hasSchema && (
        <div className="p-4">
          <div className="flex items-start gap-2 p-3 bg-amber-50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-800 rounded text-xs">
            <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
            <div className="space-y-1">
              <div className="font-medium text-amber-900 dark:text-amber-200">
                No upstream schema detected
              </div>
              <div className="text-amber-800 dark:text-amber-300">
                Connect this node to an upstream data source (CSV Source, DB Extract, etc.) to enable
                field selection and transformation configuration.
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Main Content */}
      {hasSchema && (
        <div className="flex-1 flex overflow-hidden">
          {/* LEFT: Field Selection + Transformation List */}
          <div className="w-80 border-r border-border flex flex-col bg-neutral-50 dark:bg-neutral-900/50">
            {/* Field Search */}
            <div className="p-3 border-b border-border bg-white dark:bg-neutral-900">
              <div className="relative">
                <Search className="absolute left-2.5 top-2 h-3.5 w-3.5 text-muted-foreground" />
                <Input
                  type="search"
                  placeholder="Search fields..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="pl-8 h-8 text-xs"
                />
              </div>
            </div>

            {/* Available Fields */}
            {availableFields.length > 0 && (
              <div className="p-3 border-b border-border">
                <Label className="text-xs font-medium text-muted-foreground mb-2 block">
                  Available Fields
                </Label>
                <ScrollArea className="h-32">
                  <div className="space-y-1">
                    {availableFields.slice(0, 20).map((field) => (
                      <button
                        key={field.name}
                        onClick={() => handleAddTransformation(field.name)}
                        className="w-full flex items-center justify-between p-2 text-left hover:bg-white dark:hover:bg-neutral-800 rounded border border-transparent hover:border-border transition-colors group"
                      >
                        <div className="flex-1 min-w-0">
                          <div className="text-xs font-medium text-foreground truncate">
                            {field.name}
                          </div>
                          <div className="text-xs text-muted-foreground">{field.type}</div>
                        </div>
                        <Plus className="w-3.5 h-3.5 text-green-600 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0 ml-2" />
                      </button>
                    ))}
                    {availableFields.length > 20 && (
                      <div className="text-xs text-muted-foreground text-center py-1">
                        + {availableFields.length - 20} more...
                      </div>
                    )}
                  </div>
                </ScrollArea>
              </div>
            )}

            {/* Configured Transformations */}
            <div className="flex-1 flex flex-col overflow-hidden">
              <div className="px-3 py-2 border-b border-border bg-white dark:bg-neutral-900">
                <Label className="text-xs font-medium text-muted-foreground">
                  Transformations ({transformations.length})
                </Label>
              </div>
              <ScrollArea className="flex-1">
                <div className="p-2 space-y-1">
                  {transformations.length === 0 && (
                    <div className="p-8 text-center text-xs text-muted-foreground">
                      No transformations configured.
                      <br />
                      Click a field to add one.
                    </div>
                  )}
                  {transformations.map((transform, index) => (
                    <button
                      key={index}
                      onClick={() => {
                        setSelectedTransformIndex(index);
                        setActiveTab('configure');
                      }}
                      className={`w-full p-2.5 text-left rounded border transition-all ${
                        selectedTransformIndex === index
                          ? 'bg-green-50 dark:bg-green-950/20 border-green-300 dark:border-green-700'
                          : 'bg-white dark:bg-neutral-800 border-border hover:border-green-200 dark:hover:border-green-800'
                      }`}
                    >
                      <div className="flex items-start gap-2">
                        <GripVertical className="w-3.5 h-3.5 text-muted-foreground mt-0.5 flex-shrink-0" />
                        <div className="flex-1 min-w-0">
                          <div className="text-xs font-medium text-foreground mb-1 truncate">
                            {transform.field}
                          </div>
                          <div className="text-xs text-muted-foreground">
                            {transform.operations.length === 0 ? (
                              <span className="text-amber-600">No operations</span>
                            ) : (
                              <span>
                                {transform.operations.length} operation
                                {transform.operations.length !== 1 ? 's' : ''}
                              </span>
                            )}
                          </div>
                          {transform.operations.length > 0 && (
                            <div className="flex flex-wrap gap-1 mt-1.5">
                              {transform.operations.slice(0, 3).map((op, opIdx) => (
                                <Badge
                                  key={opIdx}
                                  variant="outline"
                                  className="text-xs px-1.5 py-0 h-5 bg-green-50 dark:bg-green-950/30 border-green-200 dark:border-green-800 text-green-700 dark:text-green-300"
                                >
                                  {op.type}
                                </Badge>
                              ))}
                              {transform.operations.length > 3 && (
                                <Badge variant="outline" className="text-xs px-1.5 py-0 h-5">
                                  +{transform.operations.length - 3}
                                </Badge>
                              )}
                            </div>
                          )}
                        </div>
                        <ChevronRight
                          className={`w-3.5 h-3.5 transition-transform flex-shrink-0 ${
                            selectedTransformIndex === index
                              ? 'text-green-600 rotate-90'
                              : 'text-muted-foreground'
                          }`}
                        />
                      </div>
                    </button>
                  ))}
                </div>
              </ScrollArea>
            </div>
          </div>

          {/* RIGHT: Transformation Details */}
          <div className="flex-1 flex flex-col bg-white dark:bg-neutral-900">
            {selectedTransformation ? (
              <>
                {/* Tabs */}
                <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as any)} className="flex-1 flex flex-col">
                  <div className="border-b border-border">
                    <TabsList className="w-full justify-start h-10 bg-transparent px-4 gap-4">
                      <TabsTrigger value="configure" className="text-xs">
                        <Sparkles className="w-3.5 h-3.5 mr-1.5" />
                        Configure
                      </TabsTrigger>
                      <TabsTrigger value="preview" className="text-xs">
                        <Eye className="w-3.5 h-3.5 mr-1.5" />
                        Preview
                      </TabsTrigger>
                    </TabsList>
                  </div>

                  {/* Configure Tab */}
                  <TabsContent value="configure" className="flex-1 m-0 p-4 space-y-4">
                    {/* Field Info */}
                    <div className="p-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
                      <div className="flex items-center justify-between mb-2">
                        <Label className="text-xs font-medium text-foreground">Field</Label>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() =>
                            handleDeleteTransformation(selectedTransformIndex as number)
                          }
                          className="h-6 px-2 text-xs text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950/30"
                        >
                          <Trash2 className="w-3 h-3 mr-1" />
                          Remove
                        </Button>
                      </div>
                      <div className="font-mono text-sm font-semibold text-foreground">
                        {selectedTransformation.field}
                      </div>
                      <div className="text-xs text-muted-foreground mt-1">
                        {upstreamSchema.find((f) => f.name === selectedTransformation.field)
                          ?.type || 'Unknown type'}
                      </div>
                    </div>

                    <Separator />

                    {/* Transformation Pipeline */}
                    <TransformationPipeline
                      transformation={selectedTransformation}
                      onUpdate={(updates) =>
                        handleUpdateTransformation(selectedTransformIndex as number, updates)
                      }
                      upstreamSchema={upstreamSchema}
                    />
                  </TabsContent>

                  {/* Preview Tab */}
                  <TabsContent value="preview" className="flex-1 m-0 p-4">
                    <TransformationPreview
                      transformation={selectedTransformation}
                      upstreamSchema={upstreamSchema}
                    />
                  </TabsContent>
                </Tabs>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center p-8">
                <div className="text-center space-y-3">
                  <div className="w-16 h-16 mx-auto rounded-full bg-neutral-100 dark:bg-neutral-800 flex items-center justify-center">
                    <Wand2 className="w-8 h-8 text-neutral-400" />
                  </div>
                  <div className="text-sm font-medium text-foreground">
                    Select a transformation
                  </div>
                  <div className="text-xs text-muted-foreground max-w-xs mx-auto">
                    Choose a field from the list on the left to configure transformations, or add a
                    new field to transform.
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Footer Summary */}
      {hasSchema && transformations.length > 0 && (
        <div className="px-4 py-2.5 border-t border-border bg-neutral-50 dark:bg-neutral-900/50">
          <div className="flex items-center justify-between text-xs">
            <div className="text-muted-foreground">
              <span className="font-medium text-foreground">{transformations.length}</span> field
              {transformations.length !== 1 ? 's' : ''} configured
            </div>
            <div className="text-muted-foreground">
              Total operations:{' '}
              <span className="font-medium text-foreground">
                {transformations.reduce((sum, t) => sum + t.operations.length, 0)}
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
