/**
 * Validation Rule List Component
 *
 * Displays configured validation rules with:
 * - Visual distinction by severity (error/warning)
 * - Drag-and-drop reordering
 * - Quick actions (edit, duplicate, delete)
 * - Severity toggle
 */

import React from 'react';
import {
  GripVertical,
  AlertCircle,
  AlertTriangle,
  Trash2,
  Copy,
  ChevronRight,
  Hash,
  TextCursorInput,
  ListFilter,
  CheckCircle,
  Code2,
  Layers,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import type { ValidationRule } from '@/lib/workflow-etl-config';

export interface ValidationRuleListProps {
  rules: ValidationRule[];
  selectedIndex: number | null;
  onSelect: (index: number) => void;
  onDelete: (index: number) => void;
  onDuplicate: (index: number) => void;
  onToggleSeverity: (index: number) => void;
}

function normalizeRuleTypeLabel(ruleType: unknown): string {
  if (typeof ruleType === 'string') {
    return ruleType.replace(/_/g, ' ');
  }

  if (!ruleType || typeof ruleType !== 'object') {
    return 'CUSTOM';
  }

  const entries = Object.entries(ruleType as Record<string, unknown>);
  if (entries.length === 1) {
    return entries[0][0].replace(/_/g, ' ');
  }

  return 'CUSTOM';
}

function normalizeRuleTypeKey(ruleType: unknown): string {
  if (typeof ruleType === 'string') {
    return ruleType;
  }

  if (!ruleType || typeof ruleType !== 'object') {
    return 'CUSTOM';
  }

  const entries = Object.entries(ruleType as Record<string, unknown>);
  return entries.length === 1 ? entries[0][0] : 'CUSTOM';
}

// Icon map for rule types
const RULE_ICONS: Record<string, any> = {
  NOT_NULL: CheckCircle,
  REGEX: TextCursorInput,
  RANGE: Hash,
  IN_SET: ListFilter,
  UNIQUE: Layers,
  CUSTOM: Code2,
};

// Color map for rule types
const RULE_COLORS: Record<string, string> = {
  NOT_NULL: 'text-green-600',
  REGEX: 'text-blue-600',
  RANGE: 'text-purple-600',
  IN_SET: 'text-orange-600',
  UNIQUE: 'text-cyan-600',
  CUSTOM: 'text-pink-600',
};

export function ValidationRuleList({
  rules,
  selectedIndex,
  onSelect,
  onDelete,
  onDuplicate,
  onToggleSeverity,
}: ValidationRuleListProps) {
  if (rules.length === 0) {
    return (
      <div className="p-8 text-center">
        <div className="w-16 h-16 mx-auto mb-3 rounded-full bg-neutral-100 dark:bg-neutral-800 flex items-center justify-center">
          <AlertCircle className="w-8 h-8 text-neutral-400" />
        </div>
        <div className="text-sm font-medium text-foreground mb-1">No validation rules</div>
        <div className="text-xs text-muted-foreground max-w-xs mx-auto">
          Add fields from the left panel to start creating validation rules.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      {rules.map((rule, index) => {
        const ruleTypeKey = normalizeRuleTypeKey(rule.rule_type);
        const ruleTypeLabel = normalizeRuleTypeLabel(rule.rule_type);
        const Icon = RULE_ICONS[ruleTypeKey] || AlertCircle;
        const isSelected = selectedIndex === index;
        const isError = rule.severity === 'error';

        return (
          <div
            key={index}
            onClick={() => onSelect(index)}
            className={`group relative p-3 rounded border transition-all cursor-pointer ${
              isSelected
                ? isError
                  ? 'bg-red-50 dark:bg-red-950/20 border-red-300 dark:border-red-700 shadow-sm'
                  : 'bg-amber-50 dark:bg-amber-950/20 border-amber-300 dark:border-amber-700 shadow-sm'
                : 'bg-white dark:bg-neutral-800 border-border hover:border-neutral-300 dark:hover:border-neutral-600'
            }`}
          >
            {/* Drag handle */}
            <div className="absolute left-1 top-1/2 -translate-y-1/2">
              <GripVertical className="w-3.5 h-3.5 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity cursor-grab" />
            </div>

            <div className="pl-4">
              {/* Header */}
              <div className="flex items-start justify-between gap-2 mb-2">
                <div className="flex items-center gap-2 flex-1 min-w-0">
                  {/* Severity indicator */}
                  {isError ? (
                    <AlertCircle className="w-4 h-4 text-red-600 flex-shrink-0" />
                  ) : (
                    <AlertTriangle className="w-4 h-4 text-amber-600 flex-shrink-0" />
                  )}

                  {/* Field name */}
                  <div className="flex-1 min-w-0">
                    <div className="font-medium text-foreground text-sm truncate">
                      {rule.field}
                    </div>
                  </div>

                  {/* Quick actions */}
                  <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        onToggleSeverity(index);
                      }}
                      className="h-6 w-6 p-0"
                      title={`Change to ${isError ? 'warning' : 'error'}`}
                    >
                      {isError ? (
                        <AlertTriangle className="w-3.5 h-3.5 text-amber-600" />
                      ) : (
                        <AlertCircle className="w-3.5 h-3.5 text-red-600" />
                      )}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        onDuplicate(index);
                      }}
                      className="h-6 w-6 p-0"
                      title="Duplicate rule"
                    >
                      <Copy className="w-3.5 h-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        onDelete(index);
                      }}
                      className="h-6 w-6 p-0 text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950/30"
                      title="Delete rule"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </Button>
                  </div>

                  {/* Expand indicator */}
                  <ChevronRight
                    className={`w-4 h-4 transition-transform flex-shrink-0 ${
                      isSelected ? 'rotate-90 text-foreground' : 'text-muted-foreground'
                    }`}
                  />
                </div>
              </div>

              {/* Rule details */}
              <div className="flex items-center gap-2 text-xs">
                {/* Rule type badge */}
                <Badge
                  variant="outline"
                  className={`${RULE_COLORS[ruleTypeKey] || 'text-neutral-600'} bg-white dark:bg-neutral-900 flex items-center gap-1`}
                >
                  <Icon className="w-3 h-3" />
                  {ruleTypeLabel}
                </Badge>

                {/* Severity badge */}
                <Badge
                  variant={isError ? 'destructive' : 'default'}
                  className={isError ? '' : 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200'}
                >
                  {isError ? 'Error' : 'Warning'}
                </Badge>

                {/* Rule-specific preview */}
                {ruleTypeKey === 'REGEX' && rule.params?.pattern && (
                  <span className="text-muted-foreground font-mono truncate max-w-[200px]">
                    /{rule.params.pattern.substring(0, 30)}{rule.params.pattern.length > 30 ? '...' : ''}/
                  </span>
                )}
                {ruleTypeKey === 'RANGE' && (
                  <span className="text-muted-foreground">
                    {rule.params?.min !== undefined ? `≥ ${rule.params.min}` : ''}
                    {rule.params?.min !== undefined && rule.params?.max !== undefined ? ' & ' : ''}
                    {rule.params?.max !== undefined ? `≤ ${rule.params.max}` : ''}
                  </span>
                )}
                {ruleTypeKey === 'IN_SET' && rule.params?.allowed_values && (
                  <span className="text-muted-foreground">
                    {rule.params.allowed_values.length} value{rule.params.allowed_values.length !== 1 ? 's' : ''}
                  </span>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
