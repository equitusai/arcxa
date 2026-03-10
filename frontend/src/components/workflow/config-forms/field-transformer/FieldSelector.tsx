/**
 * Field Selector Component
 *
 * Simple reusable field selector (currently unused but available for future enhancement)
 */

import React from 'react';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

interface FieldSelectorProps {
  fields: Array<{ name: string; type: string }>;
  value?: string;
  onSelect: (fieldName: string) => void;
  label?: string;
  placeholder?: string;
}

export function FieldSelector({
  fields,
  value,
  onSelect,
  label = 'Select Field',
  placeholder = 'Choose a field...',
}: FieldSelectorProps) {
  return (
    <div className="space-y-1.5">
      <Label className="text-xs font-medium text-foreground">{label}</Label>
      <Select value={value} onValueChange={onSelect}>
        <SelectTrigger className="h-8 text-xs">
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent>
          {fields.map((field) => (
            <SelectItem key={field.name} value={field.name} className="text-xs">
              <div className="flex items-center justify-between gap-4">
                <span className="font-medium">{field.name}</span>
                <span className="text-muted-foreground">{field.type}</span>
              </div>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
