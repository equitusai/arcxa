/**
 * Field List Component
 * Expandable list of fields with type annotations
 */

import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface Field {
  name: string;
  type: string;
  icon?: string;
  onClick?: () => void;
}

interface FieldListProps {
  fields: Field[];
  maxVisible?: number;
  className?: string;
}

export function FieldList({ fields, maxVisible = 3, className }: FieldListProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  const visibleFields = isExpanded ? fields : fields.slice(0, maxVisible);
  const hasMore = fields.length > maxVisible;
  const hiddenCount = fields.length - maxVisible;

  return (
    <div className={cn('space-y-1', className)}>
      {visibleFields.map((field, idx) => (
        <button
          key={idx}
          onClick={field.onClick}
          disabled={!field.onClick}
          className={cn(
            'w-full flex items-center gap-2 text-xs text-left transition-colors',
            field.onClick
              ? 'hover:text-blue-600 cursor-pointer'
              : 'text-neutral-600 cursor-default'
          )}
          title={field.onClick ? `Click to edit ${field.name}` : undefined}
        >
          {field.icon && <span className="flex-shrink-0">{field.icon}</span>}
          <span className="flex-shrink-0 text-green-600">✓</span>
          <span className="font-medium truncate">{field.name}</span>
          <span className="text-neutral-500">({field.type})</span>
        </button>
      ))}

      {hasMore && !isExpanded && (
        <button
          onClick={() => setIsExpanded(true)}
          className="w-full flex items-center gap-1.5 text-xs text-blue-600 hover:text-blue-700 font-medium transition-colors py-0.5"
        >
          <ChevronRight className="w-3 h-3" />
          <span>+ {hiddenCount} more...</span>
        </button>
      )}

      {isExpanded && hasMore && (
        <button
          onClick={() => setIsExpanded(false)}
          className="w-full flex items-center gap-1.5 text-xs text-blue-600 hover:text-blue-700 font-medium transition-colors py-0.5"
        >
          <ChevronDown className="w-3 h-3" />
          <span>Show less</span>
        </button>
      )}
    </div>
  );
}
