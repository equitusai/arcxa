/**
 * Aggregation Builder Component
 * Phase 2.2: Configure aggregations (COUNT, SUM, AVG, MIN, MAX) for joined data
 * UX Redesign: Horizontal formula layout for compact, scannable interface
 */

import React from 'react';
import { Plus, Trash2, Calculator, Copy, ChevronDown, ChevronUp } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';

export interface Aggregation {
  field: string;
  operation: 'COUNT' | 'SUM' | 'AVG' | 'MIN' | 'MAX';
  alias: string;
}

export interface AggregationBuilderProps {
  aggregations: Aggregation[];
  availableFields: Array<{ name: string; type: string }>;
  onUpdate: (aggregations: Aggregation[]) => void;
  sourceAlias?: string;
}

export function AggregationBuilder({
  aggregations,
  availableFields,
  onUpdate,
  sourceAlias,
}: AggregationBuilderProps) {
  const [showHelp, setShowHelp] = React.useState(false);

  const handleAddAggregation = () => {
    const newAgg: Aggregation = {
      field: availableFields[0]?.name || '',
      operation: 'COUNT',
      alias: `${availableFields[0]?.name || 'field'}_count`,
    };
    onUpdate([...aggregations, newAgg]);
  };

  const handleUpdateAggregation = (index: number, updates: Partial<Aggregation>) => {
    const updated = aggregations.map((agg, i) =>
      i === index ? { ...agg, ...updates } : agg
    );
    onUpdate(updated);
  };

  const handleRemoveAggregation = (index: number) => {
    onUpdate(aggregations.filter((_, i) => i !== index));
  };

  const handleDuplicateAggregation = (index: number) => {
    const original = aggregations[index];
    if (!original) return;

    // Smart alias incrementing
    const aliasMatch = original.alias.match(/^(.+?)(\d+)$/);
    const newAlias = aliasMatch
      ? `${aliasMatch[1]}${parseInt(aliasMatch[2]) + 1}`
      : `${original.alias}_2`;

    const duplicate: Aggregation = {
      ...original,
      alias: newAlias,
    };

    const updated = [...aggregations];
    updated.splice(index + 1, 0, duplicate);
    onUpdate(updated);
  };

  const handleAddTemplate = (template: Aggregation) => {
    onUpdate([...aggregations, template]);
  };

  const getOperationColor = (operation: string) => {
    switch (operation) {
      case 'COUNT':
        return 'bg-blue-100 text-blue-700 border-blue-200';
      case 'SUM':
        return 'bg-green-100 text-green-700 border-green-200';
      case 'AVG':
        return 'bg-purple-100 text-purple-700 border-purple-200';
      case 'MIN':
        return 'bg-orange-100 text-orange-700 border-orange-200';
      case 'MAX':
        return 'bg-red-100 text-red-700 border-red-200';
      default:
        return 'bg-gray-100 text-gray-700 border-gray-200';
    }
  };

  const getOperationDescription = (operation: string) => {
    switch (operation) {
      case 'COUNT':
        return 'Count number of occurrences';
      case 'SUM':
        return 'Sum of all values';
      case 'AVG':
        return 'Average of all values';
      case 'MIN':
        return 'Minimum value';
      case 'MAX':
        return 'Maximum value';
      default:
        return '';
    }
  };

  const getSuggestedAlias = (field: string, operation: string) => {
    const cleanField = field.toLowerCase().replace(/[^a-z0-9]/g, '_');
    switch (operation) {
      case 'COUNT':
        return `${cleanField}_count`;
      case 'SUM':
        return `total_${cleanField}`;
      case 'AVG':
        return `avg_${cleanField}`;
      case 'MIN':
        return `min_${cleanField}`;
      case 'MAX':
        return `max_${cleanField}`;
      default:
        return cleanField;
    }
  };

  // Common templates
  const templates: Array<{ label: string; agg: Aggregation }> = [
    {
      label: 'Count records',
      agg: {
        field: availableFields[0]?.name || 'id',
        operation: 'COUNT',
        alias: 'total_count',
      },
    },
    {
      label: 'Sum amounts',
      agg: {
        field: availableFields.find(f => f.name.includes('amount'))?.name || availableFields[0]?.name || 'amount',
        operation: 'SUM',
        alias: 'total_amount',
      },
    },
    {
      label: 'Average rating',
      agg: {
        field: availableFields.find(f => f.name.includes('rating'))?.name || availableFields[0]?.name || 'rating',
        operation: 'AVG',
        alias: 'avg_rating',
      },
    },
  ];

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Calculator className="h-4 w-4 text-muted-foreground" />
          <Label className="text-xs font-medium">
            Aggregations {aggregations.length > 0 && `(${aggregations.length})`}
          </Label>
        </div>
        <div className="flex items-center gap-2">
          {aggregations.length === 0 && availableFields.length > 0 && (
            <Button
              onClick={() => setShowHelp(!showHelp)}
              variant="ghost"
              size="sm"
              className="h-7 text-xs"
            >
              {showHelp ? <ChevronUp className="h-3 w-3 mr-1" /> : <ChevronDown className="h-3 w-3 mr-1" />}
              Help
            </Button>
          )}
          <Button
            onClick={handleAddAggregation}
            variant="outline"
            size="sm"
            className="h-7 text-xs"
            disabled={availableFields.length === 0}
          >
            <Plus className="h-3 w-3 mr-1" />
            Add
          </Button>
        </div>
      </div>

      {/* Description */}
      <p className="text-xs text-muted-foreground">
        Compute aggregate values when joining this source (e.g., total orders per customer)
      </p>

      {/* No fields available */}
      {availableFields.length === 0 && (
        <div className="p-3 bg-amber-50 border border-amber-200 rounded text-xs text-amber-800">
          No fields available for aggregation. Configure the source schema first.
        </div>
      )}

      {/* Quick Templates */}
      {aggregations.length === 0 && availableFields.length > 0 && showHelp && (
        <div className="p-3 bg-blue-50 border border-blue-200 rounded space-y-2">
          <div className="text-xs font-semibold text-blue-900">Quick Start Templates</div>
          <div className="flex flex-wrap gap-2">
            {templates.map((template, idx) => (
              <Button
                key={idx}
                onClick={() => handleAddTemplate(template.agg)}
                variant="outline"
                size="sm"
                className="h-7 text-xs bg-white hover:bg-blue-100"
              >
                {template.label}
              </Button>
            ))}
          </div>
        </div>
      )}

      {/* Aggregation List - HORIZONTAL LAYOUT */}
      {aggregations.length > 0 ? (
        <div className="space-y-2">
          {aggregations.map((agg, index) => (
            <div
              key={index}
              className="group relative border border-neutral-200 rounded-md bg-white hover:bg-neutral-50 transition-colors"
            >
              {/* Inline formula preview ABOVE controls */}
              <div className="px-3 pt-2 pb-1 text-xs font-mono text-purple-700 bg-purple-50/50">
                <span className="text-neutral-500">➜ </span>
                {sourceAlias && `${sourceAlias}.`}
                {agg.alias} = {agg.operation}({agg.field})
              </div>

              {/* Horizontal controls: OPERATION ( field ) AS alias [actions] */}
              <div className="flex items-center gap-2 px-3 py-2">
                {/* Operation - 110px */}
                <Select
                  value={agg.operation}
                  onValueChange={(value: Aggregation['operation']) => {
                    handleUpdateAggregation(index, {
                      operation: value,
                      alias: getSuggestedAlias(agg.field, value),
                    });
                  }}
                >
                  <SelectTrigger className="w-[110px] text-xs h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="COUNT" className="text-xs">
                      COUNT
                    </SelectItem>
                    <SelectItem value="SUM" className="text-xs">
                      SUM
                    </SelectItem>
                    <SelectItem value="AVG" className="text-xs">
                      AVG
                    </SelectItem>
                    <SelectItem value="MIN" className="text-xs">
                      MIN
                    </SelectItem>
                    <SelectItem value="MAX" className="text-xs">
                      MAX
                    </SelectItem>
                  </SelectContent>
                </Select>

                {/* Opening parenthesis */}
                <span className="text-neutral-500 text-sm font-mono">(</span>

                {/* Field - flex-grow with min-width */}
                <Select
                  value={agg.field}
                  onValueChange={(value) => {
                    handleUpdateAggregation(index, {
                      field: value,
                      alias: getSuggestedAlias(value, agg.operation),
                    });
                  }}
                >
                  <SelectTrigger className="flex-1 min-w-[180px] text-xs h-8 font-mono">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {availableFields.map((field) => (
                      <SelectItem
                        key={field.name}
                        value={field.name}
                        className="font-mono text-xs"
                      >
                        {field.name}
                        <span className="ml-2 text-muted-foreground">({field.type})</span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>

                {/* Closing parenthesis */}
                <span className="text-neutral-500 text-sm font-mono">)</span>

                {/* AS keyword */}
                <span className="text-neutral-600 text-xs font-semibold">AS</span>

                {/* Alias - 200px */}
                <Input
                  value={agg.alias}
                  onChange={(e) =>
                    handleUpdateAggregation(index, { alias: e.target.value })
                  }
                  className="w-[200px] text-xs font-mono h-8"
                  placeholder={getSuggestedAlias(agg.field, agg.operation)}
                />

                {/* Actions */}
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <Button
                    onClick={() => handleDuplicateAggregation(index)}
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 p-0"
                    title="Duplicate"
                  >
                    <Copy className="h-3 w-3 text-neutral-600" />
                  </Button>
                  <Button
                    onClick={() => handleRemoveAggregation(index)}
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 p-0"
                    title="Remove"
                  >
                    <Trash2 className="h-3 w-3 text-red-600" />
                  </Button>
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : (
        availableFields.length > 0 && !showHelp && (
          <div className="p-3 bg-muted/50 rounded text-xs text-muted-foreground text-center">
            No aggregations configured. Click "Add" or "Help" for templates.
          </div>
        )
      )}
    </div>
  );
}
