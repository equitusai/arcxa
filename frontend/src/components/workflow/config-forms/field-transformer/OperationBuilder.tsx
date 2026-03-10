/**
 * Operation Builder Component
 *
 * Type-specific parameter configuration for each transformation operation
 * Provides tailored UI for TRIM, LOWER, UPPER, REGEX, CONCAT, SPLIT, CUSTOM
 */

import React, { useState } from 'react';
import { Check, Info, Code2, Plus, X } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Checkbox } from '@/components/ui/checkbox';
import { Badge } from '@/components/ui/badge';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

interface OperationBuilderProps {
  operationType: 'TRIM' | 'LOWER' | 'UPPER' | 'REGEX' | 'CONCAT' | 'SPLIT' | 'CUSTOM';
  params: Record<string, any>;
  onUpdate: (params: Record<string, any>) => void;
  upstreamSchema: Array<{ name: string; type: string; sample_values?: string[] }>;
  fieldName: string;
}

export function OperationBuilder({
  operationType,
  params,
  onUpdate,
  upstreamSchema,
  fieldName,
}: OperationBuilderProps) {
  switch (operationType) {
    case 'TRIM':
      return <TrimBuilder />;
    case 'LOWER':
      return <LowerBuilder />;
    case 'UPPER':
      return <UpperBuilder />;
    case 'REGEX':
      return <RegexBuilder params={params} onUpdate={onUpdate} />;
    case 'CONCAT':
      return <ConcatBuilder params={params} onUpdate={onUpdate} upstreamSchema={upstreamSchema} />;
    case 'SPLIT':
      return <SplitBuilder params={params} onUpdate={onUpdate} />;
    case 'CUSTOM':
      return <CustomBuilder params={params} onUpdate={onUpdate} fieldName={fieldName} />;
    default:
      return null;
  }
}

// TRIM: No parameters needed
function TrimBuilder() {
  return (
    <div className="space-y-2">
      <div className="flex items-start gap-2 p-2 bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded text-xs">
        <Info className="w-3.5 h-3.5 text-blue-600 flex-shrink-0 mt-0.5" />
        <div className="text-blue-800 dark:text-blue-300">
          Removes leading and trailing whitespace from the field value.
        </div>
      </div>
      <div className="p-2 bg-neutral-50 dark:bg-neutral-800 rounded text-xs font-mono">
        <span className="text-muted-foreground">Example:</span> " hello " → "hello"
      </div>
    </div>
  );
}

// LOWER: No parameters needed
function LowerBuilder() {
  return (
    <div className="space-y-2">
      <div className="flex items-start gap-2 p-2 bg-purple-50 dark:bg-purple-950/20 border border-purple-200 dark:border-purple-800 rounded text-xs">
        <Info className="w-3.5 h-3.5 text-purple-600 flex-shrink-0 mt-0.5" />
        <div className="text-purple-800 dark:text-purple-300">
          Converts all characters to lowercase.
        </div>
      </div>
      <div className="p-2 bg-neutral-50 dark:bg-neutral-800 rounded text-xs font-mono">
        <span className="text-muted-foreground">Example:</span> "Hello World" → "hello world"
      </div>
    </div>
  );
}

// UPPER: No parameters needed
function UpperBuilder() {
  return (
    <div className="space-y-2">
      <div className="flex items-start gap-2 p-2 bg-indigo-50 dark:bg-indigo-950/20 border border-indigo-200 dark:border-indigo-800 rounded text-xs">
        <Info className="w-3.5 h-3.5 text-indigo-600 flex-shrink-0 mt-0.5" />
        <div className="text-indigo-800 dark:text-indigo-300">
          Converts all characters to UPPERCASE.
        </div>
      </div>
      <div className="p-2 bg-neutral-50 dark:bg-neutral-800 rounded text-xs font-mono">
        <span className="text-muted-foreground">Example:</span> "Hello World" → "HELLO WORLD"
      </div>
    </div>
  );
}

// REGEX: Pattern, replacement, flags
function RegexBuilder({
  params,
  onUpdate,
}: {
  params: Record<string, any>;
  onUpdate: (params: Record<string, any>) => void;
}) {
  const pattern = params.pattern || '';
  const replacement = params.replacement || '';
  const flags = params.flags || { global: true, caseInsensitive: false, multiline: false };

  return (
    <div className="space-y-3">
      {/* Pattern */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">
          Pattern <span className="text-red-500">*</span>
        </Label>
        <Input
          type="text"
          placeholder="e.g., [0-9]+"
          value={pattern}
          onChange={(e) => onUpdate({ ...params, pattern: e.target.value })}
          className="font-mono text-xs h-8"
        />
        <p className="text-xs text-muted-foreground">
          Regular expression pattern to match
        </p>
      </div>

      {/* Replacement */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">Replacement</Label>
        <Input
          type="text"
          placeholder="e.g., ***"
          value={replacement}
          onChange={(e) => onUpdate({ ...params, replacement: e.target.value })}
          className="font-mono text-xs h-8"
        />
        <p className="text-xs text-muted-foreground">
          Text to replace matches with (leave empty to extract only)
        </p>
      </div>

      {/* Flags */}
      <div className="space-y-2">
        <Label className="text-xs font-medium text-foreground">Flags</Label>
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Checkbox
              id="global"
              checked={flags.global}
              onCheckedChange={(checked) =>
                onUpdate({ ...params, flags: { ...flags, global: checked } })
              }
            />
            <Label htmlFor="global" className="text-xs text-foreground cursor-pointer">
              Global (replace all occurrences)
            </Label>
          </div>
          <div className="flex items-center gap-2">
            <Checkbox
              id="caseInsensitive"
              checked={flags.caseInsensitive}
              onCheckedChange={(checked) =>
                onUpdate({ ...params, flags: { ...flags, caseInsensitive: checked } })
              }
            />
            <Label htmlFor="caseInsensitive" className="text-xs text-foreground cursor-pointer">
              Case insensitive
            </Label>
          </div>
          <div className="flex items-center gap-2">
            <Checkbox
              id="multiline"
              checked={flags.multiline}
              onCheckedChange={(checked) =>
                onUpdate({ ...params, flags: { ...flags, multiline: checked } })
              }
            />
            <Label htmlFor="multiline" className="text-xs text-foreground cursor-pointer">
              Multiline
            </Label>
          </div>
        </div>
      </div>

      {/* Example */}
      {pattern && (
        <div className="p-2 bg-neutral-50 dark:bg-neutral-800 rounded text-xs font-mono">
          <div className="text-muted-foreground mb-1">Example:</div>
          <div>
            Input: "Phone: 555-1234" → Output:{' '}
            {replacement ? `"Phone: ${replacement}"` : '"555-1234"'}
          </div>
        </div>
      )}
    </div>
  );
}

// CONCAT: Fields to combine, separator
function ConcatBuilder({
  params,
  onUpdate,
  upstreamSchema,
}: {
  params: Record<string, any>;
  onUpdate: (params: Record<string, any>) => void;
  upstreamSchema: Array<{ name: string; type: string }>;
}) {
  const fields = params.fields || [];
  const separator = params.separator || ' ';
  const [newField, setNewField] = useState('');

  const handleAddField = (fieldName: string) => {
    if (fieldName && !fields.includes(fieldName)) {
      onUpdate({ ...params, fields: [...fields, fieldName] });
      setNewField('');
    }
  };

  const handleRemoveField = (index: number) => {
    onUpdate({ ...params, fields: fields.filter((_: any, i: number) => i !== index) });
  };

  return (
    <div className="space-y-3">
      {/* Fields to concatenate */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">
          Fields to Concatenate <span className="text-red-500">*</span>
        </Label>
        {fields.length > 0 && (
          <div className="space-y-1 mb-2">
            {fields.map((field: string, index: number) => (
              <div
                key={index}
                className="flex items-center justify-between p-2 bg-neutral-50 dark:bg-neutral-800 rounded border border-border"
              >
                <span className="text-xs font-mono font-medium text-foreground">{field}</span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => handleRemoveField(index)}
                  className="h-5 w-5 p-0 text-red-600 hover:text-red-700"
                >
                  <X className="w-3 h-3" />
                </Button>
              </div>
            ))}
          </div>
        )}
        <div className="flex gap-2">
          <Select value={newField} onValueChange={handleAddField}>
            <SelectTrigger className="flex-1 h-8 text-xs">
              <SelectValue placeholder="Add field..." />
            </SelectTrigger>
            <SelectContent>
              {upstreamSchema
                .filter((f) => !fields.includes(f.name))
                .map((field) => (
                  <SelectItem key={field.name} value={field.name} className="text-xs">
                    {field.name} ({field.type})
                  </SelectItem>
                ))}
            </SelectContent>
          </Select>
        </div>
        <p className="text-xs text-muted-foreground">
          Select fields in the order they should be combined
        </p>
      </div>

      {/* Separator */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">Separator</Label>
        <div className="flex gap-2">
          <Input
            type="text"
            placeholder="e.g., ' ', ', ', '-'"
            value={separator}
            onChange={(e) => onUpdate({ ...params, separator: e.target.value })}
            className="flex-1 font-mono text-xs h-8"
          />
          <div className="flex gap-1">
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, separator: ' ' })}
              className="h-8 px-2 text-xs"
            >
              Space
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, separator: ', ' })}
              className="h-8 px-2 text-xs"
            >
              Comma
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, separator: '-' })}
              className="h-8 px-2 text-xs"
            >
              Dash
            </Button>
          </div>
        </div>
      </div>

      {/* Preview */}
      {fields.length > 0 && (
        <div className="p-2 bg-neutral-50 dark:bg-neutral-800 rounded text-xs font-mono">
          <div className="text-muted-foreground mb-1">Preview:</div>
          <div>
            {fields.map((f: string, i: number) => (
              <React.Fragment key={i}>
                {i > 0 && <span className="text-green-600">{separator}</span>}
                <span className="text-foreground">{`{${f}}`}</span>
              </React.Fragment>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// SPLIT: Delimiter, index
function SplitBuilder({
  params,
  onUpdate,
}: {
  params: Record<string, any>;
  onUpdate: (params: Record<string, any>) => void;
}) {
  const delimiter = params.delimiter || ',';
  const index = params.index ?? null;

  return (
    <div className="space-y-3">
      {/* Delimiter */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">
          Delimiter <span className="text-red-500">*</span>
        </Label>
        <div className="flex gap-2">
          <Input
            type="text"
            placeholder="e.g., ',', '|', ' '"
            value={delimiter}
            onChange={(e) => onUpdate({ ...params, delimiter: e.target.value })}
            className="flex-1 font-mono text-xs h-8"
          />
          <div className="flex gap-1">
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, delimiter: ',' })}
              className="h-8 px-2 text-xs"
            >
              Comma
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, delimiter: ' ' })}
              className="h-8 px-2 text-xs"
            >
              Space
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, delimiter: '|' })}
              className="h-8 px-2 text-xs"
            >
              Pipe
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => onUpdate({ ...params, delimiter: '\t' })}
              className="h-8 px-2 text-xs"
            >
              Tab
            </Button>
          </div>
        </div>
      </div>

      {/* Index */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">Extract Part (Optional)</Label>
        <Input
          type="number"
          placeholder="0 = first part, 1 = second part, etc."
          value={index ?? ''}
          onChange={(e) =>
            onUpdate({
              ...params,
              index: e.target.value === '' ? null : parseInt(e.target.value),
            })
          }
          className="text-xs h-8"
          min="0"
        />
        <p className="text-xs text-muted-foreground">
          Leave empty to return array of all parts, or specify index (0-based) to extract one
          part
        </p>
      </div>

      {/* Example */}
      <div className="p-2 bg-neutral-50 dark:bg-neutral-800 rounded text-xs font-mono">
        <div className="text-muted-foreground mb-1">Example:</div>
        <div>
          Input: "apple,banana,cherry" →{' '}
          {index !== null ? `"${['apple', 'banana', 'cherry'][index] || 'out of range'}"` : '["apple", "banana", "cherry"]'}
        </div>
      </div>
    </div>
  );
}

// CUSTOM: JavaScript expression
function CustomBuilder({
  params,
  onUpdate,
  fieldName,
}: {
  params: Record<string, any>;
  onUpdate: (params: Record<string, any>) => void;
  fieldName: string;
}) {
  const expression = params.expression || '';

  return (
    <div className="space-y-3">
      {/* Info */}
      <div className="flex items-start gap-2 p-2 bg-yellow-50 dark:bg-yellow-950/20 border border-yellow-200 dark:border-yellow-800 rounded text-xs">
        <Info className="w-3.5 h-3.5 text-yellow-600 flex-shrink-0 mt-0.5" />
        <div className="space-y-1">
          <div className="font-medium text-yellow-900 dark:text-yellow-200">
            Write a JavaScript expression
          </div>
          <div className="text-yellow-800 dark:text-yellow-300">
            Available variables:
            <ul className="list-disc ml-4 mt-1">
              <li>
                <code className="bg-yellow-100 dark:bg-yellow-900/30 px-1 rounded">value</code> -
                current field value
              </li>
              <li>
                <code className="bg-yellow-100 dark:bg-yellow-900/30 px-1 rounded">row</code> -
                entire row object
              </li>
            </ul>
          </div>
        </div>
      </div>

      {/* Expression */}
      <div className="space-y-1.5">
        <Label className="text-xs font-medium text-foreground">
          Expression <span className="text-red-500">*</span>
        </Label>
        <Textarea
          placeholder={`e.g., value.split('@')[1]\ne.g., value * 1.1\ne.g., row.firstName + ' ' + row.lastName`}
          value={expression}
          onChange={(e) => onUpdate({ ...params, expression: e.target.value })}
          className="font-mono text-xs min-h-[80px]"
        />
        <p className="text-xs text-muted-foreground">
          The expression will be evaluated for each row. Return value becomes the new field value.
        </p>
      </div>

      {/* Examples */}
      <div className="space-y-2">
        <Label className="text-xs font-medium text-foreground">Common Examples</Label>
        <div className="grid grid-cols-1 gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() =>
              onUpdate({ ...params, expression: "value.split('@')[1]" })
            }
            className="justify-start h-auto p-2 text-xs font-mono hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            <Code2 className="w-3 h-3 mr-2 flex-shrink-0" />
            <span className="text-left">value.split('@')[1] - Extract domain from email</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() =>
              onUpdate({ ...params, expression: 'value * 1.1' })
            }
            className="justify-start h-auto p-2 text-xs font-mono hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            <Code2 className="w-3 h-3 mr-2 flex-shrink-0" />
            <span className="text-left">value * 1.1 - Increase by 10%</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() =>
              onUpdate({
                ...params,
                expression: 'new Date(value).toISOString().split("T")[0]',
              })
            }
            className="justify-start h-auto p-2 text-xs font-mono hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            <Code2 className="w-3 h-3 mr-2 flex-shrink-0" />
            <span className="text-left">
              new Date(value).toISOString().split("T")[0] - Format date
            </span>
          </Button>
        </div>
      </div>
    </div>
  );
}
