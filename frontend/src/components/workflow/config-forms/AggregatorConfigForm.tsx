/**
 * Aggregator Configuration Form
 * Configure GROUP BY and aggregation functions
 */

import React from 'react';
import { Sigma, AlertCircle } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import type { AggregatorConfig, DetectedField } from '@/lib/workflow-etl-config';

export interface AggregatorConfigFormProps {
  config?: AggregatorConfig;
  onUpdate: (updates: Partial<AggregatorConfig>) => void;
  nodeId?: string;
  upstreamSchema?: DetectedField[];
}

export function AggregatorConfigForm({
  config,
  onUpdate,
  upstreamSchema = [],
}: AggregatorConfigFormProps) {
  const groupBy = config?.group_by || [];
  const aggregations = config?.aggregations || [];
  const upstreamFields = upstreamSchema.map((field) => field.name);
  const invalidGroupByFields = groupBy.filter(
    (field) => upstreamFields.length > 0 && !upstreamFields.includes(field)
  );

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Sigma className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Aggregator Configuration</h3>
      </div>

      {/* Group By Fields */}
      <div className="space-y-2">
        <Label htmlFor="group-by" className="text-xs font-medium text-foreground">
          Group By Fields <span className="text-red-500">*</span>
        </Label>
        <Input
          id="group-by"
          type="text"
          placeholder="region, category"
          value={groupBy.join(', ')}
          onChange={(e) => {
            const fields = e.target.value.split(',').map(f => f.trim()).filter(Boolean);
            onUpdate({ group_by: fields });
          }}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Comma-separated list of fields to group by
        </p>
      </div>

      {upstreamFields.length > 0 && (
        <div className="space-y-2">
          <Label className="text-xs font-medium text-foreground">Available Fields</Label>
          <div className="p-3 bg-background-secondary border border-border rounded text-xs text-muted-foreground">
            {upstreamFields.join(', ')}
          </div>
        </div>
      )}

      {/* Aggregation Functions */}
      <div className="space-y-2">
        <Label className="text-xs font-medium text-foreground">
          Aggregation Functions
        </Label>
        <div className="p-4 bg-blue-50 border border-blue-200 rounded text-sm">
          <div className="font-medium text-blue-900 mb-2">Advanced Configuration UI</div>
          <p className="text-blue-800 text-xs mb-2">
            A visual interface for configuring aggregations is coming soon. Supported functions:
          </p>
          <ul className="list-disc ml-5 text-xs text-blue-800 space-y-1">
            <li><code className="font-mono bg-blue-100 px-1">SUM</code> - Sum of values</li>
            <li><code className="font-mono bg-blue-100 px-1">AVG</code> - Average of values</li>
            <li><code className="font-mono bg-blue-100 px-1">COUNT</code> - Count of rows</li>
            <li><code className="font-mono bg-blue-100 px-1">MIN/MAX</code> - Min/max values</li>
            <li><code className="font-mono bg-blue-100 px-1">STDDEV</code> - Standard deviation</li>
          </ul>
        </div>
      </div>

      {/* Validation Messages */}
      {groupBy.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            At least one group-by field is required
          </div>
        </div>
      )}

      {invalidGroupByFields.length > 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Group-by fields not found in the upstream schema: {invalidGroupByFields.join(', ')}
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      {groupBy.length > 0 && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Aggregation Summary</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Group by:</span>
            <span className="font-mono text-foreground">{groupBy.length} field{groupBy.length !== 1 ? 's' : ''}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Aggregations:</span>
            <span className="font-mono text-foreground">{aggregations.length}</span>
          </div>
          <div className="pt-1 border-t border-border mt-1">
            <div className="font-mono text-foreground text-xs">
              {groupBy.join(', ')}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
