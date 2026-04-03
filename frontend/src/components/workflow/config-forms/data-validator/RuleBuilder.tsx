/**
 * Data Validator Rule Builder
 *
 * Type-specific UI for each validation rule type:
 * - NOT_NULL: Simple toggle
 * - REGEX: Pattern + flags + test input
 * - RANGE: Min/max with type awareness
 * - IN_SET: Tag input for allowed values
 * - UNIQUE: Simple toggle with options
 * - CUSTOM: Code editor for custom validation
 */

import React, { useState } from 'react';
import {
  Hash,
  TextCursorInput,
  ListFilter,
  CheckCircle,
  Code2,
  Sparkles,
  Play,
  CheckCheck,
  X,
} from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import type { ValidationRule } from '@/lib/workflow-etl-config';

export interface RuleBuilderProps {
  rule: ValidationRule;
  onUpdate: (updates: Partial<ValidationRule>) => void;
  fieldType?: string;
  sampleValues?: string[];
}

function formatRuleTypeLabel(ruleType: unknown): string {
  if (typeof ruleType === 'string') {
    return ruleType.replace(/_/g, ' ');
  }

  if (!ruleType || typeof ruleType !== 'object') {
    return 'CUSTOM';
  }

  const entries = Object.entries(ruleType as Record<string, unknown>);
  return entries.length === 1 ? entries[0][0].replace(/_/g, ' ') : 'CUSTOM';
}

export function RuleBuilder({ rule, onUpdate, fieldType, sampleValues = [] }: RuleBuilderProps) {
  const [testValue, setTestValue] = useState('');
  const [testResult, setTestResult] = useState<{ valid: boolean; message: string } | null>(null);

  // Test validation rule against a value
  const handleTest = () => {
    try {
      switch (rule.rule_type) {
        case 'NOT_NULL':
          setTestResult({
            valid: testValue.trim() !== '',
            message: testValue.trim() !== '' ? 'Valid: Value is not empty' : 'Invalid: Value is empty',
          });
          break;

        case 'REGEX':
          if (!rule.params?.pattern) {
            setTestResult({ valid: false, message: 'Error: No pattern defined' });
            return;
          }
          const regex = new RegExp(rule.params.pattern, rule.params.flags || '');
          const matches = regex.test(testValue);
          setTestResult({
            valid: matches,
            message: matches ? 'Valid: Pattern matches' : 'Invalid: Pattern does not match',
          });
          break;

        case 'RANGE':
          const num = parseFloat(testValue);
          if (isNaN(num)) {
            setTestResult({ valid: false, message: 'Invalid: Not a valid number' });
            return;
          }
          const { min, max, inclusive } = rule.params || {};
          const minValid = min === undefined || (inclusive ? num >= min : num > min);
          const maxValid = max === undefined || (inclusive ? num <= max : num < max);
          setTestResult({
            valid: minValid && maxValid,
            message: minValid && maxValid ? 'Valid: Within range' : 'Invalid: Out of range',
          });
          break;

        case 'IN_SET':
          const allowed = rule.params?.allowed_values || [];
          const caseSensitive = rule.params?.case_sensitive ?? true;
          const found = caseSensitive
            ? allowed.includes(testValue)
            : allowed.some((v: string) => v.toLowerCase() === testValue.toLowerCase());
          setTestResult({
            valid: found,
            message: found ? 'Valid: Value in allowed set' : 'Invalid: Value not in allowed set',
          });
          break;

        case 'UNIQUE':
          setTestResult({
            valid: true,
            message: 'Uniqueness cannot be tested on a single value',
          });
          break;

        case 'CUSTOM':
          setTestResult({
            valid: true,
            message: 'Custom validation requires full dataset context',
          });
          break;
      }
    } catch (error: any) {
      setTestResult({ valid: false, message: `Error: ${error.message}` });
    }
  };

  // Render type-specific UI
  switch (rule.rule_type) {
    case 'NOT_NULL':
      return (
        <div className="space-y-4">
          <div className="p-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
            <div className="flex items-start gap-2 text-xs">
              <CheckCircle className="w-4 h-4 text-green-600 flex-shrink-0 mt-0.5" />
              <div>
                <div className="font-medium text-foreground mb-1">Not Null Validation</div>
                <div className="text-muted-foreground">
                  Ensures the field contains a non-empty value. Empty strings, null, and undefined values will fail.
                </div>
              </div>
            </div>
          </div>

          {/* Test section */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Test Validation</Label>
            <div className="flex gap-2">
              <Input
                type="text"
                placeholder="Enter test value..."
                value={testValue}
                onChange={(e) => setTestValue(e.target.value)}
                className="flex-1 h-8 text-xs"
              />
              <Button onClick={handleTest} size="sm" variant="outline" className="h-8 text-xs">
                <Play className="w-3 h-3 mr-1" />
                Test
              </Button>
            </div>
            {testResult && (
              <div
                className={`p-2 rounded text-xs flex items-start gap-2 ${
                  testResult.valid
                    ? 'bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800 text-green-700 dark:text-green-300'
                    : 'bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300'
                }`}
              >
                {testResult.valid ? (
                  <CheckCheck className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                ) : (
                  <X className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                )}
                <span>{testResult.message}</span>
              </div>
            )}
          </div>
        </div>
      );

    case 'REGEX':
      return (
        <div className="space-y-4">
          {/* Pattern input */}
          <div className="space-y-2">
            <Label htmlFor="regex-pattern" className="text-xs font-medium text-foreground">
              Regular Expression Pattern
            </Label>
            <Input
              id="regex-pattern"
              type="text"
              placeholder="^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z]{2,}$"
              value={rule.params?.pattern || ''}
              onChange={(e) => onUpdate({ params: { ...rule.params, pattern: e.target.value } })}
              className="font-mono text-xs h-8"
            />
            <p className="text-xs text-muted-foreground">JavaScript-compatible regex pattern</p>
          </div>

          {/* Flags */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Flags</Label>
            <div className="flex flex-wrap gap-2">
              {[
                { flag: 'i', label: 'Case Insensitive', desc: 'Ignore case' },
                { flag: 'g', label: 'Global', desc: 'Find all matches' },
                { flag: 'm', label: 'Multiline', desc: '^/$ match line breaks' },
              ].map(({ flag, label, desc }) => {
                const flags = rule.params?.flags || '';
                const isEnabled = flags.includes(flag);
                return (
                  <button
                    key={flag}
                    onClick={() => {
                      const newFlags = isEnabled
                        ? flags.replace(flag, '')
                        : flags + flag;
                      onUpdate({ params: { ...rule.params, flags: newFlags } });
                    }}
                    className={`px-3 py-1.5 text-xs rounded border transition-colors ${
                      isEnabled
                        ? 'bg-blue-50 dark:bg-blue-950/30 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                        : 'bg-white dark:bg-neutral-800 border-border text-muted-foreground hover:border-blue-200'
                    }`}
                  >
                    <div className="font-medium">{label}</div>
                    <div className="text-xs opacity-75">{desc}</div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Common patterns library */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Quick Patterns</Label>
            <div className="grid grid-cols-2 gap-2">
              {[
                { label: 'Email', pattern: '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Z|a-z]{2,}$' },
                { label: 'Phone (US)', pattern: '^\\(?([0-9]{3})\\)?[-. ]?([0-9]{3})[-. ]?([0-9]{4})$' },
                { label: 'URL', pattern: '^https?:\\/\\/(www\\.)?[-a-zA-Z0-9@:%._\\+~#=]{1,256}\\.[a-zA-Z0-9()]{1,6}\\b' },
                { label: 'Alphanumeric', pattern: '^[a-zA-Z0-9]+$' },
              ].map((preset) => (
                <Button
                  key={preset.label}
                  variant="outline"
                  size="sm"
                  onClick={() => onUpdate({ params: { ...rule.params, pattern: preset.pattern } })}
                  className="h-auto py-1.5 text-xs justify-start"
                >
                  <Sparkles className="w-3 h-3 mr-1.5 text-blue-600" />
                  {preset.label}
                </Button>
              ))}
            </div>
          </div>

          {/* Test section */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Test Pattern</Label>
            <div className="flex gap-2">
              <Input
                type="text"
                placeholder="Enter test value..."
                value={testValue}
                onChange={(e) => setTestValue(e.target.value)}
                className="flex-1 h-8 text-xs"
              />
              <Button onClick={handleTest} size="sm" variant="outline" className="h-8 text-xs">
                <Play className="w-3 h-3 mr-1" />
                Test
              </Button>
            </div>
            {testResult && (
              <div
                className={`p-2 rounded text-xs flex items-start gap-2 ${
                  testResult.valid
                    ? 'bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800 text-green-700 dark:text-green-300'
                    : 'bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300'
                }`}
              >
                {testResult.valid ? (
                  <CheckCheck className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                ) : (
                  <X className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                )}
                <span>{testResult.message}</span>
              </div>
            )}
          </div>
        </div>
      );

    case 'RANGE':
      return (
        <div className="space-y-4">
          {/* Min/Max inputs */}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-2">
              <Label htmlFor="range-min" className="text-xs font-medium text-foreground">
                Minimum Value
              </Label>
              <Input
                id="range-min"
                type={fieldType === 'DATE' || fieldType === 'TIMESTAMP' ? 'date' : 'number'}
                placeholder="No limit"
                value={rule.params?.min ?? ''}
                onChange={(e) => {
                  const value = e.target.value === '' ? undefined : (fieldType === 'DATE' || fieldType === 'TIMESTAMP' ? e.target.value : parseFloat(e.target.value));
                  onUpdate({ params: { ...rule.params, min: value } });
                }}
                className="h-8 text-xs"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="range-max" className="text-xs font-medium text-foreground">
                Maximum Value
              </Label>
              <Input
                id="range-max"
                type={fieldType === 'DATE' || fieldType === 'TIMESTAMP' ? 'date' : 'number'}
                placeholder="No limit"
                value={rule.params?.max ?? ''}
                onChange={(e) => {
                  const value = e.target.value === '' ? undefined : (fieldType === 'DATE' || fieldType === 'TIMESTAMP' ? e.target.value : parseFloat(e.target.value));
                  onUpdate({ params: { ...rule.params, max: value } });
                }}
                className="h-8 text-xs"
              />
            </div>
          </div>

          {/* Inclusive toggle */}
          <div className="flex items-center justify-between py-2 px-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
            <div className="space-y-0.5">
              <Label htmlFor="range-inclusive" className="text-xs font-medium text-foreground">
                Inclusive Range
              </Label>
              <p className="text-xs text-muted-foreground">
                Include min/max values as valid (≤ / ≥ instead of &lt; / &gt;)
              </p>
            </div>
            <Switch
              id="range-inclusive"
              checked={rule.params?.inclusive ?? true}
              onCheckedChange={(checked) => onUpdate({ params: { ...rule.params, inclusive: checked } })}
            />
          </div>

          {/* Quick presets */}
          {fieldType === 'INTEGER' || fieldType === 'FLOAT' ? (
            <div className="space-y-2">
              <Label className="text-xs font-medium text-foreground">Quick Presets</Label>
              <div className="grid grid-cols-2 gap-2">
                {[
                  { label: 'Positive Numbers', min: 0, max: undefined },
                  { label: 'Percentage (0-100)', min: 0, max: 100 },
                  { label: 'Negative Only', min: undefined, max: 0 },
                  { label: 'Non-Zero', min: undefined, max: undefined, custom: true },
                ].map((preset) => (
                  <Button
                    key={preset.label}
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      if (!preset.custom) {
                        onUpdate({ params: { ...rule.params, min: preset.min, max: preset.max, inclusive: true } });
                      }
                    }}
                    className="h-auto py-1.5 text-xs justify-start"
                  >
                    <Sparkles className="w-3 h-3 mr-1.5 text-blue-600" />
                    {preset.label}
                  </Button>
                ))}
              </div>
            </div>
          ) : null}

          {/* Test section */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Test Range</Label>
            <div className="flex gap-2">
              <Input
                type={fieldType === 'DATE' || fieldType === 'TIMESTAMP' ? 'date' : 'number'}
                placeholder="Enter test value..."
                value={testValue}
                onChange={(e) => setTestValue(e.target.value)}
                className="flex-1 h-8 text-xs"
              />
              <Button onClick={handleTest} size="sm" variant="outline" className="h-8 text-xs">
                <Play className="w-3 h-3 mr-1" />
                Test
              </Button>
            </div>
            {testResult && (
              <div
                className={`p-2 rounded text-xs flex items-start gap-2 ${
                  testResult.valid
                    ? 'bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800 text-green-700 dark:text-green-300'
                    : 'bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300'
                }`}
              >
                {testResult.valid ? (
                  <CheckCheck className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                ) : (
                  <X className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                )}
                <span>{testResult.message}</span>
              </div>
            )}
          </div>
        </div>
      );

    case 'IN_SET':
      const allowedValues = rule.params?.allowed_values || [];
      const [newValue, setNewValue] = useState('');

      return (
        <div className="space-y-4">
          {/* Tag input for allowed values */}
          <div className="space-y-2">
            <Label htmlFor="in-set-values" className="text-xs font-medium text-foreground">
              Allowed Values
            </Label>
            <div className="flex gap-2">
              <Input
                id="in-set-values"
                type="text"
                placeholder="Enter value and press Enter..."
                value={newValue}
                onChange={(e) => setNewValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && newValue.trim()) {
                    e.preventDefault();
                    const updated = [...allowedValues, newValue.trim()];
                    onUpdate({ params: { ...rule.params, allowed_values: updated } });
                    setNewValue('');
                  }
                }}
                className="flex-1 h-8 text-xs"
              />
              <Button
                onClick={() => {
                  if (newValue.trim()) {
                    const updated = [...allowedValues, newValue.trim()];
                    onUpdate({ params: { ...rule.params, allowed_values: updated } });
                    setNewValue('');
                  }
                }}
                size="sm"
                variant="outline"
                className="h-8 text-xs"
              >
                Add
              </Button>
            </div>
            {allowedValues.length > 0 && (
              <div className="flex flex-wrap gap-1.5 p-2 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
                {allowedValues.map((value: string, idx: number) => (
                  <Badge
                    key={idx}
                    variant="secondary"
                    className="text-xs px-2 py-0.5 flex items-center gap-1"
                  >
                    {value}
                    <button
                      onClick={() => {
                        const updated = allowedValues.filter((_: string, i: number) => i !== idx);
                        onUpdate({ params: { ...rule.params, allowed_values: updated } });
                      }}
                      className="hover:bg-red-100 dark:hover:bg-red-900 rounded-full p-0.5"
                    >
                      <X className="w-2.5 h-2.5" />
                    </button>
                  </Badge>
                ))}
              </div>
            )}
            <p className="text-xs text-muted-foreground">{allowedValues.length} allowed value(s)</p>
          </div>

          {/* Case sensitivity */}
          <div className="flex items-center justify-between py-2 px-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
            <div className="space-y-0.5">
              <Label htmlFor="in-set-case" className="text-xs font-medium text-foreground">
                Case Sensitive
              </Label>
              <p className="text-xs text-muted-foreground">
                Distinguish between uppercase and lowercase
              </p>
            </div>
            <Switch
              id="in-set-case"
              checked={rule.params?.case_sensitive ?? true}
              onCheckedChange={(checked) => onUpdate({ params: { ...rule.params, case_sensitive: checked } })}
            />
          </div>

          {/* Auto-detect from sample data */}
          {sampleValues.length > 0 && (
            <div className="space-y-2">
              <Label className="text-xs font-medium text-foreground">
                Detected Unique Values ({sampleValues.slice(0, 20).length})
              </Label>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  const unique = Array.from(new Set(sampleValues.slice(0, 100)));
                  onUpdate({ params: { ...rule.params, allowed_values: unique } });
                }}
                className="w-full h-auto py-1.5 text-xs justify-start"
              >
                <Sparkles className="w-3 h-3 mr-1.5 text-blue-600" />
                Import unique values from sample data
              </Button>
            </div>
          )}

          {/* Test section */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Test Value</Label>
            <div className="flex gap-2">
              <Input
                type="text"
                placeholder="Enter test value..."
                value={testValue}
                onChange={(e) => setTestValue(e.target.value)}
                className="flex-1 h-8 text-xs"
              />
              <Button onClick={handleTest} size="sm" variant="outline" className="h-8 text-xs">
                <Play className="w-3 h-3 mr-1" />
                Test
              </Button>
            </div>
            {testResult && (
              <div
                className={`p-2 rounded text-xs flex items-start gap-2 ${
                  testResult.valid
                    ? 'bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800 text-green-700 dark:text-green-300'
                    : 'bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300'
                }`}
              >
                {testResult.valid ? (
                  <CheckCheck className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                ) : (
                  <X className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                )}
                <span>{testResult.message}</span>
              </div>
            )}
          </div>
        </div>
      );

    case 'UNIQUE':
      return (
        <div className="space-y-4">
          <div className="p-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
            <div className="flex items-start gap-2 text-xs">
              <CheckCircle className="w-4 h-4 text-green-600 flex-shrink-0 mt-0.5" />
              <div>
                <div className="font-medium text-foreground mb-1">Uniqueness Validation</div>
                <div className="text-muted-foreground">
                  Ensures all values in this field are unique (no duplicates). Duplicate values will be flagged.
                </div>
              </div>
            </div>
          </div>

          {/* Composite key option */}
          <div className="flex items-center justify-between py-2 px-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
            <div className="space-y-0.5">
              <Label htmlFor="unique-composite" className="text-xs font-medium text-foreground">
                Composite Uniqueness
              </Label>
              <p className="text-xs text-muted-foreground">
                Combine multiple fields for uniqueness check (coming soon)
              </p>
            </div>
            <Switch id="unique-composite" checked={false} disabled />
          </div>
        </div>
      );

    case 'CUSTOM':
      return (
        <div className="space-y-4">
          {/* Expression editor */}
          <div className="space-y-2">
            <Label htmlFor="custom-expression" className="text-xs font-medium text-foreground">
              Validation Expression
            </Label>
            <textarea
              id="custom-expression"
              placeholder="value !== null && value.length > 5"
              value={rule.params?.expression || ''}
              onChange={(e) => onUpdate({ params: { ...rule.params, expression: e.target.value } })}
              className="w-full min-h-[100px] px-3 py-2 text-xs font-mono border border-border rounded bg-white dark:bg-neutral-900 focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <p className="text-xs text-muted-foreground">
              JavaScript expression returning true/false. Available variable: <code className="px-1 py-0.5 bg-neutral-100 dark:bg-neutral-800 rounded font-mono">value</code>
            </p>
          </div>

          {/* Documentation */}
          <div className="p-3 bg-blue-50 dark:bg-blue-950/30 rounded border border-blue-200 dark:border-blue-800">
            <div className="text-xs space-y-2">
              <div className="font-medium text-blue-900 dark:text-blue-200">Available Context</div>
              <div className="space-y-1 text-blue-800 dark:text-blue-300">
                <div><code className="px-1 py-0.5 bg-blue-100 dark:bg-blue-900 rounded font-mono">value</code> - Current field value</div>
                <div><code className="px-1 py-0.5 bg-blue-100 dark:bg-blue-900 rounded font-mono">row</code> - Full row object (future)</div>
                <div><code className="px-1 py-0.5 bg-blue-100 dark:bg-blue-900 rounded font-mono">index</code> - Row index (future)</div>
              </div>
            </div>
          </div>

          {/* Test section */}
          <div className="space-y-2">
            <Label className="text-xs font-medium text-foreground">Test Expression</Label>
            <div className="flex gap-2">
              <Input
                type="text"
                placeholder="Enter test value..."
                value={testValue}
                onChange={(e) => setTestValue(e.target.value)}
                className="flex-1 h-8 text-xs"
              />
              <Button
                onClick={() => {
                  try {
                    if (!rule.params?.expression) {
                      setTestResult({ valid: false, message: 'No expression defined' });
                      return;
                    }
                    // eslint-disable-next-line no-new-func
                    const fn = new Function('value', `return (${rule.params.expression});`);
                    const result = fn(testValue);
                    setTestResult({
                      valid: !!result,
                      message: result ? 'Valid: Expression returned true' : 'Invalid: Expression returned false',
                    });
                  } catch (error: any) {
                    setTestResult({ valid: false, message: `Error: ${error.message}` });
                  }
                }}
                size="sm"
                variant="outline"
                className="h-8 text-xs"
              >
                <Play className="w-3 h-3 mr-1" />
                Test
              </Button>
            </div>
            {testResult && (
              <div
                className={`p-2 rounded text-xs flex items-start gap-2 ${
                  testResult.valid
                    ? 'bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800 text-green-700 dark:text-green-300'
                    : 'bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300'
                }`}
              >
                {testResult.valid ? (
                  <CheckCheck className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                ) : (
                  <X className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
                )}
                <span>{testResult.message}</span>
              </div>
            )}
          </div>
        </div>
      );

    default:
      return (
        <div className="p-4 text-center text-sm text-muted-foreground">
          Unknown rule type: {formatRuleTypeLabel(rule.rule_type)}
        </div>
      );
  }
}
