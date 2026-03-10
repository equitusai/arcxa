/**
 * Field Picker Component
 *
 * Schema-aware field selection for validation rules:
 * - Search and filter fields
 * - Show field types and sample values
 * - Visual metadata display
 * - Quick add to validation rules
 */

import React, { useState, useMemo } from 'react';
import {
  Search,
  Plus,
  Database,
  Hash,
  Calendar,
  Type,
  CheckCircle,
  AlertCircle,
} from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';

export interface FieldPickerProps {
  fields: Array<{
    name: string;
    type: string;
    sample_values?: string[];
    nullable?: boolean;
    completeness?: number; // % of non-null values
  }>;
  selectedFields: string[]; // Fields already in validation rules
  onSelectField: (fieldName: string, fieldType: string) => void;
}

// Icon map for field types
const TYPE_ICONS: Record<string, any> = {
  STRING: Type,
  INTEGER: Hash,
  FLOAT: Hash,
  BOOLEAN: CheckCircle,
  DATE: Calendar,
  TIMESTAMP: Calendar,
};

// Color map for field types
const TYPE_COLORS: Record<string, string> = {
  STRING: 'bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-200',
  INTEGER: 'bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-200',
  FLOAT: 'bg-purple-100 text-purple-700 dark:bg-purple-900 dark:text-purple-200',
  BOOLEAN: 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-200',
  DATE: 'bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-200',
  TIMESTAMP: 'bg-orange-100 text-orange-700 dark:bg-orange-900 dark:text-orange-200',
};

export function FieldPicker({ fields, selectedFields, onSelectField }: FieldPickerProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [typeFilter, setTypeFilter] = useState<string | null>(null);
  const [showOnlyAvailable, setShowOnlyAvailable] = useState(true);

  // Filter fields
  const filteredFields = useMemo(() => {
    return fields.filter((field) => {
      const matchesSearch = field.name.toLowerCase().includes(searchQuery.toLowerCase());
      const matchesType = !typeFilter || field.type === typeFilter;
      const matchesAvailability = !showOnlyAvailable || !selectedFields.includes(field.name);
      return matchesSearch && matchesType && matchesAvailability;
    });
  }, [fields, searchQuery, typeFilter, selectedFields, showOnlyAvailable]);

  // Get unique field types for filter
  const uniqueTypes = useMemo(() => {
    return Array.from(new Set(fields.map((f) => f.type))).sort();
  }, [fields]);

  return (
    <div className="flex flex-col h-full">
      {/* Search */}
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

      {/* Type filter */}
      <div className="p-3 border-b border-border bg-neutral-50 dark:bg-neutral-900/50">
        <Label className="text-xs font-medium text-muted-foreground mb-2 block">Field Type</Label>
        <div className="flex flex-wrap gap-1">
          <Button
            variant={typeFilter === null ? 'default' : 'outline'}
            size="sm"
            onClick={() => setTypeFilter(null)}
            className="h-6 px-2 text-xs"
          >
            All
          </Button>
          {uniqueTypes.map((type) => (
            <Button
              key={type}
              variant={typeFilter === type ? 'default' : 'outline'}
              size="sm"
              onClick={() => setTypeFilter(type)}
              className="h-6 px-2 text-xs"
            >
              {type}
            </Button>
          ))}
        </div>
      </div>

      {/* Availability toggle */}
      <div className="px-3 py-2 border-b border-border bg-neutral-50 dark:bg-neutral-900/50">
        <button
          onClick={() => setShowOnlyAvailable(!showOnlyAvailable)}
          className="flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
        >
          <div
            className={`w-4 h-4 rounded border-2 flex items-center justify-center transition-colors ${
              showOnlyAvailable
                ? 'bg-blue-600 border-blue-600'
                : 'border-neutral-300 dark:border-neutral-600'
            }`}
          >
            {showOnlyAvailable && <CheckCircle className="w-3 h-3 text-white" />}
          </div>
          <span>Show only available fields</span>
        </button>
      </div>

      {/* Field list */}
      <ScrollArea className="flex-1">
        <div className="p-2 space-y-1">
          {filteredFields.length === 0 ? (
            <div className="p-8 text-center text-xs text-muted-foreground">
              {searchQuery || typeFilter ? (
                <>
                  No fields match your filter.
                  <br />
                  Try adjusting your search or type filter.
                </>
              ) : (
                <>
                  No available fields.
                  <br />
                  All fields have validation rules.
                </>
              )}
            </div>
          ) : (
            filteredFields.map((field) => {
              const Icon = TYPE_ICONS[field.type] || Database;
              const isSelected = selectedFields.includes(field.name);
              const completeness = field.completeness ?? 100;

              return (
                <button
                  key={field.name}
                  onClick={() => !isSelected && onSelectField(field.name, field.type)}
                  disabled={isSelected}
                  className={`w-full p-2.5 text-left rounded border transition-all ${
                    isSelected
                      ? 'bg-neutral-100 dark:bg-neutral-800 border-border opacity-50 cursor-not-allowed'
                      : 'bg-white dark:bg-neutral-800 border-border hover:border-blue-300 dark:hover:border-blue-700 hover:shadow-sm'
                  }`}
                >
                  <div className="flex items-start gap-2 mb-2">
                    <Icon className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
                    <div className="flex-1 min-w-0">
                      <div className="font-medium text-foreground text-sm truncate mb-0.5">
                        {field.name}
                      </div>
                      <div className="flex items-center gap-1.5">
                        <Badge variant="secondary" className={`text-xs px-1.5 py-0 h-4 ${TYPE_COLORS[field.type] || ''}`}>
                          {field.type}
                        </Badge>
                        {field.nullable && (
                          <Badge variant="outline" className="text-xs px-1.5 py-0 h-4 text-amber-700 border-amber-200">
                            nullable
                          </Badge>
                        )}
                      </div>
                    </div>
                    {!isSelected && (
                      <Plus className="w-4 h-4 text-blue-600 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
                    )}
                  </div>

                  {/* Completeness indicator */}
                  {field.completeness !== undefined && (
                    <div className="space-y-1">
                      <div className="flex items-center justify-between text-xs">
                        <span className="text-muted-foreground">Completeness</span>
                        <span
                          className={`font-medium ${
                            completeness >= 95
                              ? 'text-green-600'
                              : completeness >= 80
                              ? 'text-amber-600'
                              : 'text-red-600'
                          }`}
                        >
                          {Math.round(completeness)}%
                        </span>
                      </div>
                      <div className="h-1 bg-neutral-200 dark:bg-neutral-700 rounded-full overflow-hidden">
                        <div
                          className={`h-full transition-all ${
                            completeness >= 95
                              ? 'bg-green-500'
                              : completeness >= 80
                              ? 'bg-amber-500'
                              : 'bg-red-500'
                          }`}
                          style={{ width: `${completeness}%` }}
                        />
                      </div>
                    </div>
                  )}

                  {/* Sample values */}
                  {field.sample_values && field.sample_values.length > 0 && (
                    <div className="mt-2 pt-2 border-t border-border">
                      <div className="text-xs text-muted-foreground mb-1">Sample values:</div>
                      <div className="flex flex-wrap gap-1">
                        {field.sample_values.slice(0, 3).map((value, idx) => (
                          <code
                            key={idx}
                            className="px-1.5 py-0.5 bg-neutral-100 dark:bg-neutral-900 text-xs font-mono rounded border border-border truncate max-w-[120px]"
                          >
                            {value}
                          </code>
                        ))}
                        {field.sample_values.length > 3 && (
                          <span className="text-xs text-muted-foreground">
                            +{field.sample_values.length - 3} more
                          </span>
                        )}
                      </div>
                    </div>
                  )}

                  {/* Already selected indicator */}
                  {isSelected && (
                    <div className="mt-2 flex items-center gap-1.5 text-xs text-green-600">
                      <CheckCircle className="w-3 h-3" />
                      <span>Rule configured</span>
                    </div>
                  )}
                </button>
              );
            })
          )}
        </div>
      </ScrollArea>

      {/* Footer stats */}
      <div className="px-3 py-2 border-t border-border bg-neutral-50 dark:bg-neutral-900/50">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>
            {filteredFields.length} field{filteredFields.length !== 1 ? 's' : ''}
          </span>
          <span>
            {selectedFields.length} / {fields.length} with rules
          </span>
        </div>
      </div>
    </div>
  );
}
