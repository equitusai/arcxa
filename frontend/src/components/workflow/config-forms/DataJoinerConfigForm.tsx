/**
 * Data Joiner Configuration Form
 * Configure JOIN operations between datasets with multi-key join support
 */

import { Merge, AlertCircle, Database, ArrowRight, Plus, X } from 'lucide-react';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { DataJoinerConfig } from '@/lib/workflow-etl-config';

export interface DataJoinerConfigFormProps {
  config?: DataJoinerConfig;
  onUpdate: (updates: Partial<DataJoinerConfig>) => void;
  nodeId?: string;
  leftSchema?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
  }>;
  rightSchema?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
  }>;
  leftNodeLabel?: string;
  rightNodeLabel?: string;
}

export function DataJoinerConfigForm({
  config,
  onUpdate,
  leftSchema = [],
  rightSchema = [],
  leftNodeLabel = 'Left Source',
  rightNodeLabel = 'Right Source',
}: DataJoinerConfigFormProps) {
  const joinType = config?.join_type || 'inner';
  const leftKey = config?.left_key || [];
  const rightKey = config?.right_key || [];

  const hasLeftSource = leftSchema.length > 0;
  const hasRightSource = rightSchema.length > 0;
  const hasBothSources = hasLeftSource && hasRightSource;

  // Ensure arrays have equal length
  const maxConditions = Math.max(leftKey.length, rightKey.length);
  const joinConditions = Array.from({ length: maxConditions }, (_, i) => ({
    left: leftKey[i] || '',
    right: rightKey[i] || '',
  }));

  // If no conditions exist, add one empty condition
  if (joinConditions.length === 0) {
    joinConditions.push({ left: '', right: '' });
  }

  const handleUpdateCondition = (index: number, side: 'left' | 'right', value: string) => {
    const newLeftKey = [...leftKey];
    const newRightKey = [...rightKey];

    // Ensure arrays are long enough
    while (newLeftKey.length <= index) newLeftKey.push('');
    while (newRightKey.length <= index) newRightKey.push('');

    if (side === 'left') {
      newLeftKey[index] = value;
    } else {
      newRightKey[index] = value;
    }

    // Remove empty pairs from the end
    while (
      newLeftKey.length > 0 &&
      newRightKey.length > 0 &&
      newLeftKey[newLeftKey.length - 1] === '' &&
      newRightKey[newRightKey.length - 1] === ''
    ) {
      newLeftKey.pop();
      newRightKey.pop();
    }

    onUpdate({
      left_key: newLeftKey,
      right_key: newRightKey,
    });
  };

  const handleAddCondition = () => {
    onUpdate({
      left_key: [...leftKey, ''],
      right_key: [...rightKey, ''],
    });
  };

  const handleRemoveCondition = (index: number) => {
    const newLeftKey = leftKey.filter((_, i) => i !== index);
    const newRightKey = rightKey.filter((_, i) => i !== index);

    // Ensure at least one condition exists
    if (newLeftKey.length === 0) {
      newLeftKey.push('');
      newRightKey.push('');
    }

    onUpdate({
      left_key: newLeftKey,
      right_key: newRightKey,
    });
  };

  // Validation
  const hasCompleteConditions = joinConditions.every((c) => c.left && c.right);
  const hasAnyCondition = joinConditions.some((c) => c.left || c.right);
  const hasIncompleteConditions = joinConditions.some(
    (c) => (c.left && !c.right) || (!c.left && c.right)
  );

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Merge className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Data Joiner Configuration</h3>
      </div>

      {/* No Sources Warning */}
      {!hasBothSources && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 dark:text-amber-400 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800 dark:text-amber-200">
            {!hasLeftSource && !hasRightSource && (
              <div>
                <div className="font-medium mb-1">No upstream sources connected</div>
                <div className="text-amber-700 dark:text-amber-300">
                  Connect two data sources to this Data Joiner node to configure the join operation.
                </div>
              </div>
            )}
            {hasLeftSource && !hasRightSource && (
              <div>
                <div className="font-medium mb-1">Right source not connected</div>
                <div className="text-amber-700 dark:text-amber-300">
                  Connect a second data source to configure the join operation.
                </div>
              </div>
            )}
            {!hasLeftSource && hasRightSource && (
              <div>
                <div className="font-medium mb-1">Left source not connected</div>
                <div className="text-amber-700 dark:text-amber-300">
                  Connect a second data source to configure the join operation.
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Source Information Panels */}
      {hasBothSources && (
        <div className="space-y-2">
          <Label className="text-xs font-medium text-foreground">Connected Sources</Label>
          <div className="grid grid-cols-2 gap-2">
            {/* Left Source */}
            <div className="p-2 bg-blue-50 dark:bg-blue-950 border border-blue-200 dark:border-blue-800 rounded">
              <div className="flex items-center gap-1.5 mb-1">
                <Database className="w-3 h-3 text-blue-600 dark:text-blue-400" />
                <span className="text-xs font-medium text-blue-900 dark:text-blue-100">Left</span>
              </div>
              <div className="text-xs text-blue-700 dark:text-blue-300 truncate" title={leftNodeLabel}>
                {leftNodeLabel}
              </div>
              <div className="text-xs text-blue-600 dark:text-blue-400 mt-0.5">
                {leftSchema.length} field{leftSchema.length !== 1 ? 's' : ''}
              </div>
            </div>

            {/* Right Source */}
            <div className="p-2 bg-purple-50 dark:bg-purple-950 border border-purple-200 dark:border-purple-800 rounded">
              <div className="flex items-center gap-1.5 mb-1">
                <Database className="w-3 h-3 text-purple-600 dark:text-purple-400" />
                <span className="text-xs font-medium text-purple-900 dark:text-purple-100">Right</span>
              </div>
              <div className="text-xs text-purple-700 dark:text-purple-300 truncate" title={rightNodeLabel}>
                {rightNodeLabel}
              </div>
              <div className="text-xs text-purple-600 dark:text-purple-400 mt-0.5">
                {rightSchema.length} field{rightSchema.length !== 1 ? 's' : ''}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Join Type */}
      <div className="space-y-2">
        <Label htmlFor="join-type" className="text-xs font-medium text-foreground">
          Join Type <span className="text-red-500">*</span>
        </Label>
        <Select
          value={joinType}
          onValueChange={(value) => onUpdate({ join_type: value as 'inner' | 'left' | 'right' | 'full' })}
        >
          <SelectTrigger id="join-type" className="text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="inner">Inner Join - Only matching rows</SelectItem>
            <SelectItem value="left">Left Join - All left + matching right</SelectItem>
            <SelectItem value="right">Right Join - All right + matching left</SelectItem>
            <SelectItem value="full">Full Outer Join - All rows from both</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Join Conditions Builder */}
      {hasBothSources && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label className="text-xs font-medium text-foreground">
              Join Conditions <span className="text-red-500">*</span>
            </Label>
            {joinConditions.length > 1 && (
              <span className="text-xs text-muted-foreground">
                {joinConditions.filter((c) => c.left && c.right).length} / {joinConditions.length} complete
              </span>
            )}
          </div>

          <div className="space-y-2">
            {joinConditions.map((condition, index) => (
              <div
                key={index}
                className="flex items-start gap-2 p-3 bg-muted/30 border border-border rounded"
              >
                <div className="flex-1 space-y-2">
                  {/* Left Field Picker */}
                  <div className="space-y-1">
                    <Label className="text-xs text-muted-foreground">Left Field</Label>
                    <Select
                      value={condition.left}
                      onValueChange={(value) => handleUpdateCondition(index, 'left', value)}
                    >
                      <SelectTrigger className="text-sm h-8">
                        <SelectValue placeholder="Select field..." />
                      </SelectTrigger>
                      <SelectContent>
                        {leftSchema.map((field) => (
                          <SelectItem key={field.name} value={field.name}>
                            <div className="flex items-center gap-2">
                              <span className="font-mono text-xs">{field.name}</span>
                              <span className="text-xs text-muted-foreground">({field.type})</span>
                            </div>
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>

                  {/* Equals Sign */}
                  <div className="flex items-center justify-center">
                    <ArrowRight className="w-3 h-3 text-muted-foreground" />
                  </div>

                  {/* Right Field Picker */}
                  <div className="space-y-1">
                    <Label className="text-xs text-muted-foreground">Right Field</Label>
                    <Select
                      value={condition.right}
                      onValueChange={(value) => handleUpdateCondition(index, 'right', value)}
                    >
                      <SelectTrigger className="text-sm h-8">
                        <SelectValue placeholder="Select field..." />
                      </SelectTrigger>
                      <SelectContent>
                        {rightSchema.map((field) => (
                          <SelectItem key={field.name} value={field.name}>
                            <div className="flex items-center gap-2">
                              <span className="font-mono text-xs">{field.name}</span>
                              <span className="text-xs text-muted-foreground">({field.type})</span>
                            </div>
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                </div>

                {/* Remove Button */}
                {joinConditions.length > 1 && (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => handleRemoveCondition(index)}
                    className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
                    title="Remove condition"
                  >
                    <X className="w-4 h-4" />
                  </Button>
                )}
              </div>
            ))}
          </div>

          {/* Add Condition Button */}
          <Button
            variant="outline"
            size="sm"
            onClick={handleAddCondition}
            className="w-full text-xs"
          >
            <Plus className="w-3 h-3 mr-1" />
            Add Join Condition
          </Button>

          <p className="text-xs text-muted-foreground">
            Multiple conditions are combined with AND logic. Records must match ALL conditions to be joined.
          </p>
        </div>
      )}

      {/* Validation Messages */}
      {hasBothSources && !hasAnyCondition && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 dark:text-amber-400 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800 dark:text-amber-200">
            Add at least one join condition to configure the join operation
          </div>
        </div>
      )}

      {hasBothSources && hasIncompleteConditions && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 dark:text-amber-400 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800 dark:text-amber-200">
            Each join condition must have both left and right fields selected
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      {hasBothSources && hasCompleteConditions && joinConditions.length > 0 && (
        <div className="p-3 bg-green-50 dark:bg-green-950 border border-green-200 dark:border-green-800 rounded text-xs space-y-2">
          <div className="font-medium text-green-900 dark:text-green-100 mb-2">
            ✓ Join Configuration Complete
          </div>

          <div className="flex items-center justify-between py-1">
            <span className="text-green-700 dark:text-green-300">Join type:</span>
            <span className="font-medium text-green-900 dark:text-green-100 capitalize">
              {joinType.replace('_', ' ')}
            </span>
          </div>

          <div className="pt-2 border-t border-green-200 dark:border-green-800">
            <div className="text-green-700 dark:text-green-300 mb-1.5">
              Join condition{joinConditions.length > 1 ? 's' : ''} (AND):
            </div>
            <div className="space-y-1">
              {joinConditions.map((condition, index) => (
                <div
                  key={index}
                  className="flex items-center gap-2 font-mono text-green-900 dark:text-green-100 text-xs bg-green-100 dark:bg-green-900 p-2 rounded"
                >
                  <span className="text-blue-600 dark:text-blue-400">
                    {leftNodeLabel}.{condition.left}
                  </span>
                  <ArrowRight className="w-3 h-3 text-green-600 dark:text-green-400" />
                  <span className="text-purple-600 dark:text-purple-400">
                    {rightNodeLabel}.{condition.right}
                  </span>
                </div>
              ))}
            </div>
          </div>

          <div className="pt-2 border-t border-green-200 dark:border-green-800 text-green-700 dark:text-green-300">
            This will join records where {joinConditions.length > 1 ? 'ALL' : 'the'} specified field
            {joinConditions.length > 1 ? 's' : ''} match, according to the {joinType} join strategy.
          </div>
        </div>
      )}
    </div>
  );
}
