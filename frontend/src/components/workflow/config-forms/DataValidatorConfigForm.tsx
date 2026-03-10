/**
 * Data Validator Configuration Form
 *
 * Enterprise-grade UI for configuring data validation rules
 * Design: Oracle Redwood + Microsoft Fluent (Graphica Design System)
 *
 * Features:
 * - Visual field selection from upstream schema
 * - Rule type-specific builders (NOT_NULL, REGEX, RANGE, IN_SET, UNIQUE, CUSTOM)
 * - Validation preview with sample data
 * - Quality scorecard integration
 * - Severity management (error vs warning)
 * - Drag-and-drop rule reordering
 */

import React, { useState, useMemo } from 'react';
import {
  ShieldCheck,
  Plus,
  AlertCircle,
  Eye,
  Sparkles,
  Settings2,
  ChevronRight,
  Download,
  Upload,
} from 'lucide-react';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import type { DataValidatorConfig, ValidationRule } from '@/lib/workflow-etl-config';
import { FieldPicker } from './data-validator/FieldPicker';
import { RuleBuilder } from './data-validator/RuleBuilder';
import { ValidationRuleList } from './data-validator/ValidationRuleList';
import { ValidationPreview } from './data-validator/ValidationPreview';

export interface DataValidatorConfigFormProps {
  config?: DataValidatorConfig;
  onUpdate: (updates: Partial<DataValidatorConfig>) => void;
  nodeId?: string;
  /** Schema from upstream node (auto-detected via React Flow) */
  upstreamSchema?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
    nullable?: boolean;
    completeness?: number;
  }>;
  /** Dataset ID for quality API integration */
  datasetId?: string;
}

export function DataValidatorConfigForm({
  config,
  onUpdate,
  nodeId,
  upstreamSchema = [],
  datasetId,
}: DataValidatorConfigFormProps) {
  const rules = config?.rules || [];
  const failOnError = config?.fail_on_error ?? true;

  // UI state
  const [selectedRuleIndex, setSelectedRuleIndex] = useState<number | null>(null);
  const [activeTab, setActiveTab] = useState<'configure' | 'preview'>('configure');

  // Derived state
  const selectedFields = useMemo(() => rules.map((r) => r.field), [rules]);
  const hasSchema = upstreamSchema.length > 0;
  const selectedRule = selectedRuleIndex !== null ? rules[selectedRuleIndex] : null;

  // Add new validation rule for a field
  const handleAddRule = (fieldName: string, fieldType: string) => {
    // Determine default rule type based on field type
    let defaultRuleType: ValidationRule['rule_type'] = 'NOT_NULL';
    let defaultParams: Record<string, any> = {};

    if (fieldType === 'INTEGER' || fieldType === 'FLOAT') {
      defaultRuleType = 'RANGE';
      defaultParams = { inclusive: true };
    } else if (fieldType === 'STRING') {
      defaultRuleType = 'NOT_NULL';
    }

    const newRule: ValidationRule = {
      field: fieldName,
      rule_type: defaultRuleType,
      params: defaultParams,
      severity: 'error',
    };

    onUpdate({
      rules: [...rules, newRule],
    });

    // Select the newly added rule
    setSelectedRuleIndex(rules.length);
    setActiveTab('configure');
  };

  // Update specific rule
  const handleUpdateRule = (index: number, updates: Partial<ValidationRule>) => {
    const updated = [...rules];
    updated[index] = { ...updated[index], ...updates };
    onUpdate({ rules: updated });
  };

  // Delete rule
  const handleDeleteRule = (index: number) => {
    const updated = rules.filter((_, i) => i !== index);
    onUpdate({ rules: updated });

    // Adjust selection
    if (selectedRuleIndex === index) {
      setSelectedRuleIndex(null);
    } else if (selectedRuleIndex !== null && selectedRuleIndex > index) {
      setSelectedRuleIndex(selectedRuleIndex - 1);
    }
  };

  // Duplicate rule
  const handleDuplicateRule = (index: number) => {
    const ruleToDuplicate = rules[index];
    const duplicated: ValidationRule = {
      ...ruleToDuplicate,
      field: ruleToDuplicate.field + '_copy',
    };
    onUpdate({ rules: [...rules, duplicated] });
    setSelectedRuleIndex(rules.length);
  };

  // Toggle rule severity
  const handleToggleSeverity = (index: number) => {
    const updated = [...rules];
    updated[index] = {
      ...updated[index],
      severity: updated[index].severity === 'error' ? 'warning' : 'error',
    };
    onUpdate({ rules: updated });
  };

  // Change rule type
  const handleChangeRuleType = (ruleType: ValidationRule['rule_type']) => {
    if (selectedRuleIndex === null) return;

    // Reset params when changing rule type
    let newParams: Record<string, any> = {};
    if (ruleType === 'RANGE') {
      newParams = { inclusive: true };
    } else if (ruleType === 'IN_SET') {
      newParams = { allowed_values: [], case_sensitive: true };
    }

    selectedRuleIndex !== null && handleUpdateRule(selectedRuleIndex, {
      rule_type: ruleType,
      params: newParams,
    });
  };

  // Export/Import rules
  const handleExportRules = () => {
    const dataStr = JSON.stringify(rules, null, 2);
    const dataUri = 'data:application/json;charset=utf-8,' + encodeURIComponent(dataStr);
    const exportFileDefaultName = 'validation-rules.json';

    const linkElement = document.createElement('a');
    linkElement.setAttribute('href', dataUri);
    linkElement.setAttribute('download', exportFileDefaultName);
    linkElement.click();
  };

  const handleImportRules = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        const imported = JSON.parse(e.target?.result as string);
        if (Array.isArray(imported)) {
          onUpdate({ rules: imported });
        }
      } catch (error) {
        console.error('Failed to import rules:', error);
      }
    };
    reader.readAsText(file);
  };

  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border bg-white dark:bg-neutral-900">
        <ShieldCheck className="w-4 h-4 text-red-600" />
        <h3 className="text-sm font-semibold text-foreground">Data Validator</h3>
        {rules.length > 0 && (
          <Badge variant="secondary" className="ml-auto text-xs">
            {rules.length} rule{rules.length !== 1 ? 's' : ''}
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
                field selection and validation configuration.
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Main Content */}
      {hasSchema && (
        <div className="flex-1 flex overflow-hidden">
          {/* LEFT: Field Picker + Rule List */}
          <div className="w-80 border-r border-border flex flex-col bg-neutral-50 dark:bg-neutral-900/50">
            {/* Available Fields */}
            <div className="flex-1 flex flex-col overflow-hidden border-b border-border">
              <div className="px-3 py-2 border-b border-border bg-white dark:bg-neutral-900">
                <Label className="text-xs font-medium text-muted-foreground">
                  Available Fields ({upstreamSchema.length})
                </Label>
              </div>
              <FieldPicker
                fields={upstreamSchema}
                selectedFields={selectedFields}
                onSelectField={handleAddRule}
              />
            </div>

            {/* Configured Rules */}
            <div className="flex-1 flex flex-col overflow-hidden">
              <div className="px-3 py-2 border-b border-border bg-white dark:bg-neutral-900">
                <div className="flex items-center justify-between">
                  <Label className="text-xs font-medium text-muted-foreground">
                    Validation Rules ({rules.length})
                  </Label>
                  {rules.length > 0 && (
                    <div className="flex gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={handleExportRules}
                        className="h-6 px-2 text-xs"
                        title="Export rules"
                      >
                        <Download className="w-3 h-3" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => document.getElementById('import-rules')?.click()}
                        className="h-6 px-2 text-xs"
                        title="Import rules"
                      >
                        <Upload className="w-3 h-3" />
                      </Button>
                      <input
                        id="import-rules"
                        type="file"
                        accept=".json"
                        onChange={handleImportRules}
                        className="hidden"
                      />
                    </div>
                  )}
                </div>
              </div>
              <div className="flex-1 overflow-auto p-2">
                <ValidationRuleList
                  rules={rules}
                  selectedIndex={selectedRuleIndex}
                  onSelect={setSelectedRuleIndex}
                  onDelete={handleDeleteRule}
                  onDuplicate={handleDuplicateRule}
                  onToggleSeverity={handleToggleSeverity}
                />
              </div>
            </div>
          </div>

          {/* RIGHT: Rule Configuration / Preview */}
          <div className="flex-1 flex flex-col bg-white dark:bg-neutral-900">
            {selectedRule ? (
              <>
                {/* Tabs */}
                <Tabs
                  value={activeTab}
                  onValueChange={(v) => setActiveTab(v as any)}
                  className="flex-1 flex flex-col"
                >
                  <div className="border-b border-border">
                    <TabsList className="w-full justify-start h-10 bg-transparent px-4 gap-4">
                      <TabsTrigger value="configure" className="text-xs">
                        <Settings2 className="w-3.5 h-3.5 mr-1.5" />
                        Configure Rule
                      </TabsTrigger>
                      <TabsTrigger value="preview" className="text-xs">
                        <Eye className="w-3.5 h-3.5 mr-1.5" />
                        Preview & Quality
                      </TabsTrigger>
                    </TabsList>
                  </div>

                  {/* Configure Tab */}
                  <TabsContent value="configure" className="flex-1 overflow-auto m-0 p-4 space-y-4">
                    {/* Field Info */}
                    <div className="p-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
                      <div className="flex items-center justify-between mb-2">
                        <Label className="text-xs font-medium text-foreground">Field</Label>
                        <div className="flex gap-1">
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => selectedRuleIndex !== null && handleDuplicateRule(selectedRuleIndex)}
                            className="h-6 px-2 text-xs"
                          >
                            Duplicate
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => selectedRuleIndex !== null && handleDeleteRule(selectedRuleIndex)}
                            className="h-6 px-2 text-xs text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950/30"
                          >
                            Delete
                          </Button>
                        </div>
                      </div>
                      <div className="font-mono text-sm font-semibold text-foreground">
                        {selectedRule.field}
                      </div>
                      <div className="text-xs text-muted-foreground mt-1">
                        {upstreamSchema.find((f) => f.name === selectedRule.field)?.type || 'Unknown type'}
                      </div>
                    </div>

                    <Separator />

                    {/* Rule Type Selector */}
                    <div className="space-y-2">
                      <Label htmlFor="rule-type" className="text-xs font-medium text-foreground">
                        Validation Type
                      </Label>
                      <Select
                        value={selectedRule.rule_type}
                        onValueChange={handleChangeRuleType}
                      >
                        <SelectTrigger id="rule-type" className="h-9 text-xs">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="NOT_NULL">NOT NULL - Field must not be empty</SelectItem>
                          <SelectItem value="REGEX">REGEX - Match regular expression pattern</SelectItem>
                          <SelectItem value="RANGE">RANGE - Value within min/max range</SelectItem>
                          <SelectItem value="IN_SET">IN SET - Value in allowed set</SelectItem>
                          <SelectItem value="UNIQUE">UNIQUE - No duplicate values</SelectItem>
                          <SelectItem value="CUSTOM">CUSTOM - Custom validation expression</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>

                    {/* Severity Selector */}
                    <div className="space-y-2">
                      <Label htmlFor="severity" className="text-xs font-medium text-foreground">
                        Severity Level
                      </Label>
                      <Select
                        value={selectedRule.severity}
                        onValueChange={(severity: 'error' | 'warning') =>
                          selectedRuleIndex !== null && handleUpdateRule(selectedRuleIndex, { severity })
                        }
                      >
                        <SelectTrigger id="severity" className="h-9 text-xs">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="error">
                            <div className="flex items-center gap-2">
                              <AlertCircle className="w-3.5 h-3.5 text-red-600" />
                              <div>
                                <div className="font-medium">Error</div>
                                <div className="text-xs text-muted-foreground">
                                  Stops workflow if fail-on-error is enabled
                                </div>
                              </div>
                            </div>
                          </SelectItem>
                          <SelectItem value="warning">
                            <div className="flex items-center gap-2">
                              <AlertCircle className="w-3.5 h-3.5 text-amber-600" />
                              <div>
                                <div className="font-medium">Warning</div>
                                <div className="text-xs text-muted-foreground">
                                  Logs violation but continues workflow
                                </div>
                              </div>
                            </div>
                          </SelectItem>
                        </SelectContent>
                      </Select>
                    </div>

                    <Separator />

                    {/* Rule Builder */}
                    <div className="space-y-2">
                      <Label className="text-xs font-medium text-foreground">Rule Configuration</Label>
                      <RuleBuilder
                        rule={selectedRule}
                        onUpdate={(updates) => selectedRuleIndex !== null && handleUpdateRule(selectedRuleIndex, updates)}
                        fieldType={upstreamSchema.find((f) => f.name === selectedRule.field)?.type}
                        sampleValues={upstreamSchema.find((f) => f.name === selectedRule.field)?.sample_values}
                      />
                    </div>
                  </TabsContent>

                  {/* Preview Tab */}
                  <TabsContent value="preview" className="flex-1 overflow-auto m-0 p-4">
                    <ValidationPreview
                      rules={rules}
                      upstreamSchema={upstreamSchema}
                      datasetId={datasetId}
                    />
                  </TabsContent>
                </Tabs>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center p-8">
                <div className="text-center space-y-3">
                  <div className="w-16 h-16 mx-auto rounded-full bg-neutral-100 dark:bg-neutral-800 flex items-center justify-center">
                    <ShieldCheck className="w-8 h-8 text-neutral-400" />
                  </div>
                  <div className="text-sm font-medium text-foreground">Select a validation rule</div>
                  <div className="text-xs text-muted-foreground max-w-xs mx-auto">
                    Choose a rule from the list on the left to configure its validation logic, or add a
                    new field to validate.
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Footer */}
      <div className="px-4 py-3 border-t border-border bg-neutral-50 dark:bg-neutral-900/50 space-y-3">
        {/* Fail on Error Toggle */}
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="fail-on-error" className="text-xs font-medium text-foreground">
              Fail workflow on validation errors
            </Label>
            <p className="text-xs text-muted-foreground">
              Stop workflow execution when {rules.filter((r) => r.severity === 'error').length || 'error-severity'} violations are found
            </p>
          </div>
          <Switch
            id="fail-on-error"
            checked={failOnError}
            onCheckedChange={(checked) => onUpdate({ fail_on_error: checked })}
          />
        </div>

        {/* Summary */}
        {hasSchema && rules.length > 0 && (
          <div className="flex items-center justify-between text-xs pt-2 border-t border-border">
            <div className="text-muted-foreground">
              <span className="font-medium text-foreground">{rules.length}</span> rule
              {rules.length !== 1 ? 's' : ''} configured
            </div>
            <div className="flex gap-2">
              {rules.filter((r) => r.severity === 'error').length > 0 && (
                <Badge variant="destructive" className="text-xs">
                  {rules.filter((r) => r.severity === 'error').length} error
                </Badge>
              )}
              {rules.filter((r) => r.severity === 'warning').length > 0 && (
                <Badge className="text-xs bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200">
                  {rules.filter((r) => r.severity === 'warning').length} warning
                </Badge>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
