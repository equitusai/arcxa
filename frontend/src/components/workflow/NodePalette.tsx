/**
 * Node Palette Component
 * Draggable step types organized by category with accordion layout
 */

import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card';
import { cn } from '@/lib/utils';
import { getStepTypesByCategory } from '@/lib/workflow-step-config';
import type { StepTypeConfig } from '@/lib/workflow-step-config';
import { getETLStepTypesByCategory } from '@/lib/workflow-etl-config';
import type { ETLStepTypeConfig } from '@/lib/workflow-etl-config';
import { ChevronDown, Info, GripVertical } from 'lucide-react';

interface NodePaletteProps {
  onDragStart: (event: React.DragEvent, stepType: string, label: string) => void;
}

interface Category {
  id: string;
  label: string;
  steps: (StepTypeConfig | ETLStepTypeConfig)[];
}

interface CompactStepCardProps {
  step: StepTypeConfig | ETLStepTypeConfig;
  onDragStart: (event: React.DragEvent, stepType: string, label: string) => void;
}

function CompactStepCard({ step, onDragStart }: CompactStepCardProps) {
  const StepIcon = step.icon;

  return (
    <div className="group relative">
      {/* Drag handle (visible on hover) */}
      <div className="absolute -left-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none">
        <GripVertical className="h-4 w-4 text-muted-foreground" />
      </div>

      <div
        draggable
        onDragStart={(e) => onDragStart(e, step.id, step.label)}
        className={cn(
          'flex items-center gap-2 px-2 py-1.5 rounded border border-border/50',
          'cursor-move transition-all',
          'hover:border-border hover:shadow-sm hover:translate-x-0.5',
          'active:opacity-60 active:scale-[0.98]'
        )}
        style={{
          background: `linear-gradient(135deg, ${step.color.surface} 0%, ${step.color.subtle} 100%)`,
          borderColor: step.color.border,
        }}
      >
        {/* Icon with color */}
        <StepIcon
          className="h-4 w-4 flex-shrink-0"
          style={{ color: step.color.text }}
          strokeWidth={2}
        />

        {/* Label (truncate) */}
        <span
          className="text-sm font-medium truncate flex-1"
          style={{ color: step.color.text }}
        >
          {step.label}
        </span>

        {/* Info button with tooltip */}
        <HoverCard openDelay={200}>
          <HoverCardTrigger asChild>
            <button
              className="flex-shrink-0 p-0.5 hover:bg-background-secondary rounded transition-colors"
              onClick={(e) => e.stopPropagation()}
              tabIndex={-1}
            >
              <Info className="h-3.5 w-3.5 text-muted-foreground" />
            </button>
          </HoverCardTrigger>
          <HoverCardContent
            side="right"
            align="start"
            className="w-64 p-3"
            sideOffset={8}
          >
            <div className="space-y-1.5">
              <div className="flex items-center gap-2">
                <StepIcon
                  className="h-4 w-4"
                  style={{ color: step.color.text }}
                />
                <p className="text-sm font-semibold text-foreground">
                  {step.label}
                </p>
              </div>
              <p className="text-xs text-muted-foreground uppercase tracking-wide">
                {step.category}
              </p>
              <p className="text-sm text-muted-foreground leading-relaxed">
                {step.description}
              </p>
            </div>
          </HoverCardContent>
        </HoverCard>
      </div>
    </div>
  );
}

interface CategoryAccordionProps {
  category: Category;
  expanded: boolean;
  onToggle: () => void;
  onDragStart: (event: React.DragEvent, stepType: string, label: string) => void;
}

function CategoryAccordion({ category, expanded, onToggle, onDragStart }: CategoryAccordionProps) {
  return (
    <Collapsible open={expanded} onOpenChange={onToggle}>
      <CollapsibleTrigger className="flex items-center w-full px-2 py-1.5 text-xs font-semibold text-foreground hover:bg-muted/50 rounded transition-colors">
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 mr-1.5 transition-transform",
            !expanded && "-rotate-90"
          )}
        />
        <span className="flex-1 text-left">{category.label}</span>
        <Badge variant="secondary" className="text-xs px-1.5 py-0 h-5">
          {category.steps.length}
        </Badge>
      </CollapsibleTrigger>

      <CollapsibleContent className="space-y-1 mt-1 pl-1">
        {category.steps.map(step => (
          <CompactStepCard key={step.id} step={step} onDragStart={onDragStart} />
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

export function NodePalette({ onDragStart }: NodePaletteProps) {
  // Default expanded categories (ETL Extract and Transform)
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(
    new Set(['extract', 'transform', 'prediction', 'logic'])
  );

  const categories: Category[] = [
    // ETL Categories
    { id: 'extract', label: '📥 Extract', steps: getETLStepTypesByCategory('extract') },
    { id: 'etl-transform', label: '🔄 Transform (ETL)', steps: getETLStepTypesByCategory('transform') },
    { id: 'quality', label: '✅ Quality', steps: getETLStepTypesByCategory('quality') },
    { id: 'load', label: '📤 Load', steps: getETLStepTypesByCategory('load') },
    { id: 'orchestration', label: '⏰ Orchestrate', steps: getETLStepTypesByCategory('orchestration') },
    // ML/Fusion Categories
    { id: 'prediction', label: '🧠 Prediction', steps: getStepTypesByCategory('prediction') },
    { id: 'logic', label: '💡 Logic', steps: getStepTypesByCategory('logic') },
    { id: 'aggregation', label: 'Σ Aggregation', steps: getStepTypesByCategory('aggregation') },
    { id: 'routing', label: '🔀 Routing', steps: getStepTypesByCategory('routing') },
    { id: 'transformation', label: '🪄 Transform (ML)', steps: getStepTypesByCategory('transformation') },
  ];

  const toggleCategory = (categoryId: string) => {
    setExpandedCategories(prev => {
      const next = new Set(prev);
      next.has(categoryId) ? next.delete(categoryId) : next.add(categoryId);
      return next;
    });
  };

  return (
    <Card className="h-full flex flex-col">
      <CardHeader className="pb-3 space-y-0">
        <CardTitle className="text-sm font-semibold">Step Types</CardTitle>
        <p className="text-xs text-muted-foreground mt-1">
          Drag steps onto the canvas
        </p>
      </CardHeader>

      <CardContent className="flex-1 overflow-y-auto px-3">
        <div className="space-y-1">
          {categories.map(category => (
            <CategoryAccordion
              key={category.id}
              category={category}
              expanded={expandedCategories.has(category.id)}
              onToggle={() => toggleCategory(category.id)}
              onDragStart={onDragStart}
            />
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
