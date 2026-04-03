/**
 * Transformation Pipeline Component
 *
 * Displays and manages the sequential operations applied to a field
 * Supports drag-and-drop reordering, adding/removing operations
 */

import React, { useState } from 'react';
import {
  Plus,
  GripVertical,
  Trash2,
  ChevronDown,
  ChevronUp,
  ArrowRight,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { FieldTransformation } from '@/lib/workflow-etl-config';
import { OperationBuilder } from './OperationBuilder';

interface TransformationPipelineProps {
  transformation: FieldTransformation;
  onUpdate: (updates: Partial<FieldTransformation>) => void;
  upstreamSchema: Array<{ name: string; type: string; sample_values?: string[] }>;
}

type OperationType = 'TRIM' | 'LOWER' | 'UPPER' | 'ROUND' | 'REGEX' | 'CONCAT' | 'SPLIT' | 'CUSTOM';

const OPERATION_META: Record<
  OperationType,
  { label: string; description: string; color: string; icon: string }
> = {
  TRIM: {
    label: 'Trim',
    description: 'Remove leading/trailing whitespace',
    color: 'bg-blue-50 dark:bg-blue-950/30 border-blue-200 dark:border-blue-800 text-blue-700 dark:text-blue-300',
    icon: '✂️',
  },
  LOWER: {
    label: 'Lowercase',
    description: 'Convert to lowercase',
    color: 'bg-purple-50 dark:bg-purple-950/30 border-purple-200 dark:border-purple-800 text-purple-700 dark:text-purple-300',
    icon: '⬇️',
  },
  UPPER: {
    label: 'Uppercase',
    description: 'Convert to UPPERCASE',
    color: 'bg-indigo-50 dark:bg-indigo-950/30 border-indigo-200 dark:border-indigo-800 text-indigo-700 dark:text-indigo-300',
    icon: '⬆️',
  },
  ROUND: {
    label: 'Round',
    description: 'Round numeric values to fixed decimals',
    color: 'bg-emerald-50 dark:bg-emerald-950/30 border-emerald-200 dark:border-emerald-800 text-emerald-700 dark:text-emerald-300',
    icon: '🔢',
  },
  REGEX: {
    label: 'Regex',
    description: 'Pattern extraction/replacement',
    color: 'bg-orange-50 dark:bg-orange-950/30 border-orange-200 dark:border-orange-800 text-orange-700 dark:text-orange-300',
    icon: '🔍',
  },
  CONCAT: {
    label: 'Concatenate',
    description: 'Combine multiple fields',
    color: 'bg-green-50 dark:bg-green-950/30 border-green-200 dark:border-green-800 text-green-700 dark:text-green-300',
    icon: '🔗',
  },
  SPLIT: {
    label: 'Split',
    description: 'Split field into parts',
    color: 'bg-pink-50 dark:bg-pink-950/30 border-pink-200 dark:border-pink-800 text-pink-700 dark:text-pink-300',
    icon: '✂️',
  },
  CUSTOM: {
    label: 'Custom JS',
    description: 'JavaScript expression',
    color: 'bg-yellow-50 dark:bg-yellow-950/30 border-yellow-200 dark:border-yellow-800 text-yellow-700 dark:text-yellow-300',
    icon: '⚡',
  },
};

export function TransformationPipeline({
  transformation,
  onUpdate,
  upstreamSchema,
}: TransformationPipelineProps) {
  const [expandedIndex, setExpandedIndex] = useState<number | null>(0);
  const [addingOperation, setAddingOperation] = useState(false);
  const [selectedOpType, setSelectedOpType] = useState<OperationType | ''>('');

  const operations = transformation.operations || [];

  // Add operation
  const handleAddOperation = () => {
    if (!selectedOpType) return;

    const newOperation = {
      type: selectedOpType as OperationType,
      params: {},
    };

    onUpdate({
      operations: [...operations, newOperation],
    });

    setExpandedIndex(operations.length);
    setAddingOperation(false);
    setSelectedOpType('');
  };

  // Update operation
  const handleUpdateOperation = (index: number, params: Record<string, any>) => {
    const updated = [...operations];
    updated[index] = { ...updated[index], params };
    onUpdate({ operations: updated });
  };

  // Delete operation
  const handleDeleteOperation = (index: number) => {
    const updated = operations.filter((_, i) => i !== index);
    onUpdate({ operations: updated });

    if (expandedIndex === index) {
      setExpandedIndex(null);
    } else if (expandedIndex !== null && expandedIndex > index) {
      setExpandedIndex(expandedIndex - 1);
    }
  };

  // Move operation
  const handleMoveOperation = (index: number, direction: 'up' | 'down') => {
    const targetIndex = direction === 'up' ? index - 1 : index + 1;
    if (targetIndex < 0 || targetIndex >= operations.length) return;

    const updated = [...operations];
    [updated[index], updated[targetIndex]] = [updated[targetIndex], updated[index]];
    onUpdate({ operations: updated });

    // Adjust expanded index
    if (expandedIndex === index) {
      setExpandedIndex(targetIndex);
    } else if (expandedIndex === targetIndex) {
      setExpandedIndex(index);
    }
  };

  return (
    <div className="space-y-4">
      {/* Header */}
      <div>
        <Label className="text-xs font-medium text-foreground mb-1 block">
          Transformation Pipeline
        </Label>
        <p className="text-xs text-muted-foreground">
          Operations are applied sequentially in order. Drag to reorder.
        </p>
      </div>

      {/* Pipeline */}
      {operations.length > 0 ? (
        <div className="space-y-2">
          {operations.map((op, index) => {
            const meta = OPERATION_META[op.type as OperationType];
            const isExpanded = expandedIndex === index;

            return (
              <div
                key={index}
                className={`border rounded transition-all ${
                  isExpanded
                    ? 'border-green-300 dark:border-green-700 bg-green-50/50 dark:bg-green-950/10'
                    : 'border-border bg-white dark:bg-neutral-800'
                }`}
              >
                {/* Operation Header */}
                <div className="flex items-center gap-2 p-2.5">
                  <GripVertical className="w-4 h-4 text-muted-foreground cursor-grab active:cursor-grabbing flex-shrink-0" />

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <Badge variant="outline" className={`text-xs px-2 py-0.5 ${meta.color}`}>
                        <span className="mr-1">{meta.icon}</span>
                        {meta.label}
                      </Badge>
                      <span className="text-xs text-muted-foreground">Step {index + 1}</span>
                    </div>
                    {!isExpanded && (
                      <div className="text-xs text-muted-foreground mt-0.5 truncate">
                        {meta.description}
                      </div>
                    )}
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-1 flex-shrink-0">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleMoveOperation(index, 'up')}
                      disabled={index === 0}
                      className="h-6 w-6 p-0"
                    >
                      <ChevronUp className="w-3.5 h-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleMoveOperation(index, 'down')}
                      disabled={index === operations.length - 1}
                      className="h-6 w-6 p-0"
                    >
                      <ChevronDown className="w-3.5 h-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => handleDeleteOperation(index)}
                      className="h-6 w-6 p-0 text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950/30"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setExpandedIndex(isExpanded ? null : index)}
                      className="h-6 w-6 p-0"
                    >
                      {isExpanded ? (
                        <ChevronUp className="w-3.5 h-3.5" />
                      ) : (
                        <ChevronDown className="w-3.5 h-3.5" />
                      )}
                    </Button>
                  </div>
                </div>

                {/* Operation Parameters (when expanded) */}
                {isExpanded && (
                  <div className="px-3 pb-3 pt-1 border-t border-border">
                    <OperationBuilder
                      operationType={op.type as OperationType}
                      params={op.params || {}}
                      onUpdate={(params) => handleUpdateOperation(index, params)}
                      upstreamSchema={upstreamSchema}
                      fieldName={transformation.field}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ) : (
        <div className="p-6 border border-dashed border-border rounded text-center">
          <div className="text-xs text-muted-foreground">
            No operations configured yet.
            <br />
            Add your first transformation below.
          </div>
        </div>
      )}

      {/* Add Operation */}
      {!addingOperation ? (
        <Button
          variant="outline"
          size="sm"
          onClick={() => setAddingOperation(true)}
          className="w-full"
        >
          <Plus className="w-3.5 h-3.5 mr-1.5" />
          Add Operation
        </Button>
      ) : (
        <div className="p-3 border border-green-300 dark:border-green-700 bg-green-50/50 dark:bg-green-950/10 rounded">
          <Label className="text-xs font-medium text-foreground mb-2 block">
            Select Operation Type
          </Label>
          <div className="flex gap-2">
            <Select value={selectedOpType} onValueChange={(v) => setSelectedOpType(v as OperationType)}>
              <SelectTrigger className="flex-1 h-8 text-xs">
                <SelectValue placeholder="Choose operation..." />
              </SelectTrigger>
              <SelectContent>
                {(Object.keys(OPERATION_META) as OperationType[]).map((type) => (
                  <SelectItem key={type} value={type} className="text-xs">
                    <div className="flex items-center gap-2">
                      <span>{OPERATION_META[type].icon}</span>
                      <span className="font-medium">{OPERATION_META[type].label}</span>
                      <span className="text-muted-foreground text-xs">
                        - {OPERATION_META[type].description}
                      </span>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button
              size="sm"
              onClick={handleAddOperation}
              disabled={!selectedOpType}
              className="h-8 px-3"
            >
              Add
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setAddingOperation(false);
                setSelectedOpType('');
              }}
              className="h-8 px-3"
            >
              Cancel
            </Button>
          </div>
        </div>
      )}

      {/* Pipeline Visualization */}
      {operations.length > 0 && (
        <div className="p-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
          <Label className="text-xs font-medium text-foreground mb-2 block">
            Pipeline Flow
          </Label>
          <div className="flex items-center gap-2 flex-wrap text-xs">
            <Badge variant="outline" className="px-2 py-0.5 font-mono">
              {transformation.field}
            </Badge>
            {operations.map((op, idx) => (
              <React.Fragment key={idx}>
                <ArrowRight className="w-3 h-3 text-muted-foreground" />
                <Badge variant="outline" className={`px-2 py-0.5 ${OPERATION_META[op.type as OperationType].color}`}>
                  {op.type}
                </Badge>
              </React.Fragment>
            ))}
            <ArrowRight className="w-3 h-3 text-muted-foreground" />
            <Badge variant="default" className="px-2 py-0.5 bg-green-600 text-white">
              Output
            </Badge>
          </div>
        </div>
      )}
    </div>
  );
}
