/**
 * Transformation Preview Component
 *
 * Real-time preview showing before/after transformation results
 * Uses sample data from upstream schema to demonstrate transformations
 */

import React, { useState, useMemo } from 'react';
import { Play, AlertCircle, CheckCircle, ArrowRight, Info } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import type { FieldTransformation } from '@/lib/workflow-etl-config';

interface TransformationPreviewProps {
  transformation: FieldTransformation;
  upstreamSchema: Array<{ name: string; type: string; sample_values?: string[] }>;
}

export function TransformationPreview({
  transformation,
  upstreamSchema,
}: TransformationPreviewProps) {
  const fieldSchema = upstreamSchema.find((f) => f.name === transformation.field);
  const sampleValues = fieldSchema?.sample_values || [];

  // Custom test value
  const [customTestValue, setCustomTestValue] = useState('');
  const [useCustomValue, setUseCustomValue] = useState(false);

  // Apply transformations to a value
  const applyTransformations = (value: string): { result: string | null; error?: string } => {
    let currentValue: any = value;

    try {
      for (const operation of transformation.operations) {
        switch (operation.type) {
          case 'TRIM':
            currentValue = String(currentValue).trim();
            break;

          case 'LOWER':
            currentValue = String(currentValue).toLowerCase();
            break;

          case 'UPPER':
            currentValue = String(currentValue).toUpperCase();
            break;

          case 'ROUND': {
            const decimals = Number(operation.params?.decimals ?? 0);
            const numericValue = Number(currentValue);

            if (!Number.isFinite(numericValue)) {
              throw new Error('ROUND: Value must be numeric');
            }

            currentValue = numericValue.toFixed(decimals);
            break;
          }

          case 'REGEX': {
            const pattern = operation.params?.pattern;
            const replacement = operation.params?.replacement ?? '';
            const flags = operation.params?.flags || {};

            if (!pattern) {
              throw new Error('REGEX: Pattern is required');
            }

            const flagStr = [
              flags.global && 'g',
              flags.caseInsensitive && 'i',
              flags.multiline && 'm',
            ]
              .filter(Boolean)
              .join('');

            const regex = new RegExp(pattern, flagStr);

            if (replacement) {
              currentValue = String(currentValue).replace(regex, replacement);
            } else {
              const matches = String(currentValue).match(regex);
              currentValue = matches ? matches[0] : '';
            }
            break;
          }

          case 'CONCAT': {
            const fields = operation.params?.fields || [];
            const separator = operation.params?.separator || ' ';

            if (fields.length === 0) {
              throw new Error('CONCAT: At least one field is required');
            }

            // For preview, we can't access other fields, so show placeholder
            currentValue = `{${fields.join(`}${separator}{`)}}`;
            break;
          }

          case 'SPLIT': {
            const delimiter = operation.params?.delimiter || ',';
            const index = operation.params?.index;

            const parts = String(currentValue).split(delimiter);

            if (index !== null && index !== undefined) {
              currentValue = parts[index] || '';
            } else {
              currentValue = JSON.stringify(parts);
            }
            break;
          }

          case 'CUSTOM': {
            const expression = operation.params?.expression;

            if (!expression) {
              throw new Error('CUSTOM: Expression is required');
            }

            // Create a safe evaluation context
            // eslint-disable-next-line no-new-func
            const evaluator = new Function('value', 'row', `return ${expression}`);
            currentValue = evaluator(currentValue, { [transformation.field]: currentValue });
            break;
          }

          default:
            throw new Error(`Unknown operation type: ${operation.type}`);
        }
      }

      return { result: String(currentValue) };
    } catch (error: any) {
      return { result: null, error: error.message };
    }
  };

  // Preview results for sample or custom value
  const previewResults = useMemo(() => {
    const valuesToTest = useCustomValue && customTestValue
      ? [customTestValue]
      : sampleValues.slice(0, 5);

    return valuesToTest.map((value) => ({
      input: value,
      ...applyTransformations(value),
    }));
  }, [transformation.operations, sampleValues, customTestValue, useCustomValue]);

  const hasOperations = transformation.operations.length > 0;

  return (
    <div className="space-y-4">
      {/* No Operations */}
      {!hasOperations && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 dark:bg-amber-950/20 border border-amber-200 dark:border-amber-800 rounded text-xs">
          <Info className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800 dark:text-amber-300">
            No operations configured yet. Add operations in the Configure tab to see a preview.
          </div>
        </div>
      )}

      {/* Test Input */}
      {hasOperations && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label className="text-xs font-medium text-foreground">Test Input</Label>
            <div className="flex gap-2">
              <Button
                variant={!useCustomValue ? 'default' : 'outline'}
                size="sm"
                onClick={() => setUseCustomValue(false)}
                className="h-6 px-2 text-xs"
              >
                Sample Data
              </Button>
              <Button
                variant={useCustomValue ? 'default' : 'outline'}
                size="sm"
                onClick={() => setUseCustomValue(true)}
                className="h-6 px-2 text-xs"
              >
                Custom
              </Button>
            </div>
          </div>

          {useCustomValue && (
            <Input
              type="text"
              placeholder="Enter test value..."
              value={customTestValue}
              onChange={(e) => setCustomTestValue(e.target.value)}
              className="text-xs h-8 font-mono"
            />
          )}
        </div>
      )}

      {/* Preview Results */}
      {hasOperations && (
        <div className="space-y-2">
          <Label className="text-xs font-medium text-foreground">
            Transformation Results
          </Label>

          <div className="border border-border rounded overflow-hidden">
            {/* Header */}
            <div className="grid grid-cols-2 gap-px bg-border">
              <div className="bg-neutral-50 dark:bg-neutral-900 px-3 py-2">
                <div className="text-xs font-medium text-muted-foreground">Input</div>
              </div>
              <div className="bg-neutral-50 dark:bg-neutral-900 px-3 py-2">
                <div className="text-xs font-medium text-muted-foreground">Output</div>
              </div>
            </div>

            {/* Rows */}
            <div className="divide-y divide-border">
              {previewResults.map((result, index) => (
                <div key={index} className="grid grid-cols-2 gap-px bg-border">
                  <div className="bg-white dark:bg-neutral-800 px-3 py-2.5">
                    <div className="text-xs font-mono text-foreground break-all">
                      {result.input || <span className="text-muted-foreground italic">empty</span>}
                    </div>
                  </div>
                  <div className="bg-white dark:bg-neutral-800 px-3 py-2.5">
                    {result.error ? (
                      <div className="flex items-start gap-1.5">
                        <AlertCircle className="w-3.5 h-3.5 text-red-600 flex-shrink-0 mt-0.5" />
                        <div className="text-xs text-red-600">{result.error}</div>
                      </div>
                    ) : (
                      <div className="flex items-start gap-1.5">
                        {result.result !== result.input && (
                          <CheckCircle className="w-3.5 h-3.5 text-green-600 flex-shrink-0 mt-0.5" />
                        )}
                        <div className="text-xs font-mono text-foreground break-all">
                          {result.result || (
                            <span className="text-muted-foreground italic">empty</span>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>

            {/* No sample data */}
            {previewResults.length === 0 && (
              <div className="bg-white dark:bg-neutral-800 px-3 py-6 text-center">
                <div className="text-xs text-muted-foreground">
                  {useCustomValue
                    ? 'Enter a test value above to see results'
                    : 'No sample data available. Switch to Custom to test manually.'}
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Operation Pipeline Visualization */}
      {hasOperations && (
        <div className="p-3 bg-neutral-50 dark:bg-neutral-800 rounded border border-border">
          <Label className="text-xs font-medium text-foreground mb-2 block">
            Applied Operations ({transformation.operations.length})
          </Label>
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <Badge variant="outline" className="px-2 py-0.5 font-mono bg-white dark:bg-neutral-900">
              Input
            </Badge>
            {transformation.operations.map((op, idx) => (
              <React.Fragment key={idx}>
                <ArrowRight className="w-3 h-3 text-muted-foreground" />
                <Badge variant="outline" className="px-2 py-0.5">
                  {op.type}
                </Badge>
              </React.Fragment>
            ))}
            <ArrowRight className="w-3 h-3 text-muted-foreground" />
            <Badge variant="default" className="px-2 py-0.5 bg-green-600 text-white">
              Output
            </Badge>
          </div>
        </div>
      )}
    </div>
  );
}
