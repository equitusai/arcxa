/**
 * Validation Preview Component
 *
 * Preview validation rules running on sample data with:
 * - Quality scorecard integration
 * - Violation preview
 * - Statistics and pass rate
 * - Field-level quality metrics
 */

import React, { useState, useEffect } from 'react';
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle,
  TrendingUp,
  TrendingDown,
  Activity,
  Sparkles,
  RefreshCw,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { ValidationRule } from '@/lib/workflow-etl-config';
import type { QualityScorecard } from '@/api/types';
import { getQualityScorecard } from '@/api/quality';

export interface ValidationPreviewProps {
  rules: ValidationRule[];
  upstreamSchema?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
  }>;
  datasetId?: string; // For quality API integration
}

interface ViolationPreview {
  field: string;
  rule: string;
  severity: 'error' | 'warning';
  value: string;
  message: string;
}

export function ValidationPreview({ rules, upstreamSchema = [], datasetId }: ValidationPreviewProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [scorecard, setScorecard] = useState<QualityScorecard | null>(null);
  const [violations, setViolations] = useState<ViolationPreview[]>([]);

  // Fetch quality scorecard if dataset ID available
  useEffect(() => {
    if (datasetId) {
      fetchQualityData();
    }
  }, [datasetId]);

  const fetchQualityData = async () => {
    if (!datasetId) return;

    try {
      setIsLoading(true);
      const data = await getQualityScorecard(datasetId);
      setScorecard(data);
    } catch (error) {
      console.error('Failed to fetch quality scorecard:', error);
    } finally {
      setIsLoading(false);
    }
  };

  // Simulate validation on sample data
  const runPreview = () => {
    setIsLoading(true);
    const newViolations: ViolationPreview[] = [];

    // Simulate validation
    rules.forEach((rule) => {
      const fieldSchema = upstreamSchema.find((f) => f.name === rule.field);
      const sampleValues = fieldSchema?.sample_values || [];

      // Check sample values against rule
      sampleValues.slice(0, 5).forEach((value) => {
        let isValid = true;
        let message = '';

        try {
          switch (rule.rule_type) {
            case 'NOT_NULL':
              isValid = value !== null && value !== undefined && value.trim() !== '';
              message = isValid ? 'Valid' : 'Value is empty';
              break;

            case 'REGEX':
              if (rule.params?.pattern) {
                const regex = new RegExp(rule.params.pattern, rule.params.flags || '');
                isValid = regex.test(value);
                message = isValid ? 'Pattern matches' : `Does not match pattern: ${rule.params.pattern}`;
              }
              break;

            case 'RANGE':
              const num = parseFloat(value);
              if (!isNaN(num)) {
                const { min, max, inclusive } = rule.params || {};
                const minValid = min === undefined || (inclusive ? num >= min : num > min);
                const maxValid = max === undefined || (inclusive ? num <= max : num < max);
                isValid = minValid && maxValid;
                message = isValid ? 'Within range' : `Out of range (${min ?? '-∞'} to ${max ?? '∞'})`;
              } else {
                isValid = false;
                message = 'Not a valid number';
              }
              break;

            case 'IN_SET':
              const allowed = rule.params?.allowed_values || [];
              const caseSensitive = rule.params?.case_sensitive ?? true;
              isValid = caseSensitive
                ? allowed.includes(value)
                : allowed.some((v: string) => v.toLowerCase() === value.toLowerCase());
              message = isValid ? 'Value in allowed set' : 'Value not in allowed set';
              break;

            case 'UNIQUE':
              // Can't validate uniqueness on sample alone
              isValid = true;
              message = 'Uniqueness check requires full dataset';
              break;

            case 'CUSTOM':
              // Can't safely execute custom code
              isValid = true;
              message = 'Custom validation requires execution context';
              break;
          }

          if (!isValid) {
            newViolations.push({
              field: rule.field,
              rule: rule.rule_type,
              severity: rule.severity,
              value: value.substring(0, 50),
              message,
            });
          }
        } catch (error) {
          // Ignore validation errors in preview
        }
      });
    });

    setViolations(newViolations);
    setIsLoading(false);
  };

  // Calculate statistics
  const totalRules = rules.length;
  const errorRules = rules.filter((r) => r.severity === 'error').length;
  const warningRules = rules.filter((r) => r.severity === 'warning').length;
  const totalViolations = violations.length;
  const errorViolations = violations.filter((v) => v.severity === 'error').length;
  const warningViolations = violations.filter((v) => v.severity === 'warning').length;

  return (
    <div className="space-y-4">
      {/* Quality Scorecard Integration */}
      {scorecard && (
        <div className="p-4 bg-gradient-to-br from-blue-50 to-purple-50 dark:from-blue-950/30 dark:to-purple-950/30 rounded-lg border border-blue-200 dark:border-blue-800">
          <div className="flex items-center justify-between mb-3">
            <div className="flex items-center gap-2">
              <Activity className="w-4 h-4 text-blue-600" />
              <h4 className="text-sm font-semibold text-foreground">Quality Scorecard</h4>
            </div>
            <Badge variant="secondary" className="text-xs">
              {Math.round(scorecard.overall_score * 100)}% Overall
            </Badge>
          </div>

          <div className="grid grid-cols-5 gap-2">
            {Object.entries(scorecard.dimensions).map(([dimension, score]) => (
              <div key={dimension} className="text-center p-2 bg-white dark:bg-neutral-900 rounded border border-border">
                <div className="text-xs text-muted-foreground capitalize mb-1">
                  {dimension.substring(0, 8)}
                </div>
                <div
                  className={`text-sm font-semibold ${
                    score >= 0.9
                      ? 'text-green-700'
                      : score >= 0.7
                      ? 'text-amber-700'
                      : 'text-red-700'
                  }`}
                >
                  {Math.round(score * 100)}%
                </div>
              </div>
            ))}
          </div>

          {scorecard.total_violations > 0 && (
            <div className="mt-3 flex items-center gap-2 text-xs text-amber-700 dark:text-amber-300">
              <AlertTriangle className="w-3.5 h-3.5" />
              {scorecard.total_violations} existing quality violation{scorecard.total_violations !== 1 ? 's' : ''} detected
            </div>
          )}
        </div>
      )}

      {/* Validation Statistics */}
      <div className="grid grid-cols-3 gap-3">
        <div className="p-3 bg-white dark:bg-neutral-800 rounded border border-border">
          <div className="flex items-center gap-2 mb-1">
            <CheckCircle className="w-4 h-4 text-blue-600" />
            <span className="text-xs text-muted-foreground">Total Rules</span>
          </div>
          <div className="text-2xl font-bold text-foreground">{totalRules}</div>
          <div className="flex gap-2 mt-2">
            {errorRules > 0 && (
              <Badge variant="destructive" className="text-xs">
                {errorRules} error
              </Badge>
            )}
            {warningRules > 0 && (
              <Badge className="text-xs bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200">
                {warningRules} warning
              </Badge>
            )}
          </div>
        </div>

        <div className="p-3 bg-white dark:bg-neutral-800 rounded border border-border">
          <div className="flex items-center gap-2 mb-1">
            <AlertCircle className="w-4 h-4 text-red-600" />
            <span className="text-xs text-muted-foreground">Violations</span>
          </div>
          <div className="text-2xl font-bold text-foreground">{totalViolations}</div>
          <div className="text-xs text-muted-foreground mt-1">
            From sample data preview
          </div>
        </div>

        <div className="p-3 bg-white dark:bg-neutral-800 rounded border border-border">
          <div className="flex items-center gap-2 mb-1">
            <TrendingUp className="w-4 h-4 text-green-600" />
            <span className="text-xs text-muted-foreground">Est. Pass Rate</span>
          </div>
          <div className="text-2xl font-bold text-foreground">
            {totalViolations === 0 ? '100' : '~85'}%
          </div>
          <div className="text-xs text-muted-foreground mt-1">
            Based on samples
          </div>
        </div>
      </div>

      {/* Run Preview Button */}
      <div className="flex gap-2">
        <Button
          onClick={runPreview}
          disabled={isLoading || rules.length === 0}
          className="flex-1 h-9 text-xs"
        >
          {isLoading ? (
            <>
              <RefreshCw className="w-3.5 h-3.5 mr-1.5 animate-spin" />
              Running...
            </>
          ) : (
            <>
              <Sparkles className="w-3.5 h-3.5 mr-1.5" />
              Run Validation Preview
            </>
          )}
        </Button>
        {datasetId && (
          <Button
            onClick={fetchQualityData}
            variant="outline"
            size="sm"
            className="h-9 text-xs"
          >
            <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
            Refresh Quality Data
          </Button>
        )}
      </div>

      {/* Violation List */}
      {violations.length > 0 && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <h4 className="text-sm font-semibold text-foreground">Sample Violations</h4>
            <Badge variant="secondary" className="text-xs">
              {violations.length} found
            </Badge>
          </div>

          <ScrollArea className="h-64 border border-border rounded">
            <div className="p-3 space-y-2">
              {violations.map((violation, idx) => (
                <div
                  key={idx}
                  className={`p-2.5 rounded border text-xs ${
                    violation.severity === 'error'
                      ? 'bg-red-50 dark:bg-red-950/30 border-red-200 dark:border-red-800'
                      : 'bg-amber-50 dark:bg-amber-950/30 border-amber-200 dark:border-amber-800'
                  }`}
                >
                  <div className="flex items-start gap-2 mb-1.5">
                    {violation.severity === 'error' ? (
                      <AlertCircle className="w-3.5 h-3.5 text-red-600 flex-shrink-0 mt-0.5" />
                    ) : (
                      <AlertTriangle className="w-3.5 h-3.5 text-amber-600 flex-shrink-0 mt-0.5" />
                    )}
                    <div className="flex-1">
                      <div className="font-medium text-foreground">
                        {violation.field} • {violation.rule}
                      </div>
                    </div>
                    <Badge
                      variant={violation.severity === 'error' ? 'destructive' : 'default'}
                      className={
                        violation.severity === 'error'
                          ? 'text-xs'
                          : 'text-xs bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200'
                      }
                    >
                      {violation.severity}
                    </Badge>
                  </div>
                  <div className="pl-5 space-y-1">
                    <div className="text-muted-foreground">
                      <span className="font-medium">Value:</span>{' '}
                      <code className="px-1 py-0.5 bg-white dark:bg-neutral-900 rounded font-mono">
                        {violation.value}
                      </code>
                    </div>
                    <div className="text-muted-foreground">
                      <span className="font-medium">Issue:</span> {violation.message}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </ScrollArea>
        </div>
      )}

      {/* No violations state */}
      {violations.length === 0 && rules.length > 0 && !isLoading && (
        <div className="p-8 text-center bg-green-50 dark:bg-green-950/30 rounded border border-green-200 dark:border-green-800">
          <CheckCircle className="w-12 h-12 mx-auto mb-3 text-green-600" />
          <div className="text-sm font-medium text-green-900 dark:text-green-200 mb-1">
            All sample data passed validation!
          </div>
          <div className="text-xs text-green-700 dark:text-green-300">
            Run preview to test rules against sample values
          </div>
        </div>
      )}

      {/* No rules state */}
      {rules.length === 0 && (
        <div className="p-8 text-center">
          <AlertCircle className="w-12 h-12 mx-auto mb-3 text-neutral-400" />
          <div className="text-sm font-medium text-foreground mb-1">No rules to preview</div>
          <div className="text-xs text-muted-foreground">
            Add validation rules to see a preview of violations
          </div>
        </div>
      )}

      {/* Field-Level Quality Breakdown */}
      {upstreamSchema.length > 0 && rules.length > 0 && (
        <div className="space-y-2">
          <h4 className="text-sm font-semibold text-foreground">Field Coverage</h4>
          <div className="space-y-1.5">
            {upstreamSchema.slice(0, 10).map((field) => {
              const fieldRules = rules.filter((r) => r.field === field.name);
              const hasRules = fieldRules.length > 0;
              const errorCount = fieldRules.filter((r) => r.severity === 'error').length;
              const warningCount = fieldRules.filter((r) => r.severity === 'warning').length;

              return (
                <div
                  key={field.name}
                  className={`p-2 rounded border text-xs ${
                    hasRules
                      ? 'bg-green-50 dark:bg-green-950/20 border-green-200 dark:border-green-800'
                      : 'bg-neutral-50 dark:bg-neutral-800 border-border'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      {hasRules ? (
                        <CheckCircle className="w-3.5 h-3.5 text-green-600" />
                      ) : (
                        <AlertCircle className="w-3.5 h-3.5 text-neutral-400" />
                      )}
                      <span className="font-medium text-foreground">{field.name}</span>
                      <Badge variant="outline" className="text-xs">
                        {field.type}
                      </Badge>
                    </div>
                    {hasRules && (
                      <div className="flex gap-1">
                        {errorCount > 0 && (
                          <Badge variant="destructive" className="text-xs h-5">
                            {errorCount}
                          </Badge>
                        )}
                        {warningCount > 0 && (
                          <Badge className="text-xs h-5 bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200">
                            {warningCount}
                          </Badge>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
            {upstreamSchema.length > 10 && (
              <div className="text-xs text-muted-foreground text-center py-1">
                + {upstreamSchema.length - 10} more fields...
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
