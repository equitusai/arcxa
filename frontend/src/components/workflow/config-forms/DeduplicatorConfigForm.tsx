/**
 * Deduplicator Configuration Form
 * Configure duplicate detection and removal
 */

import React from 'react';
import { Copy, AlertCircle } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Slider } from '@/components/ui/slider';
import type { DeduplicatorConfig } from '@/lib/workflow-etl-config';

export interface DeduplicatorConfigFormProps {
  config?: DeduplicatorConfig;
  onUpdate: (updates: Partial<DeduplicatorConfig>) => void;
  nodeId?: string;
}

export function DeduplicatorConfigForm({ config, onUpdate }: DeduplicatorConfigFormProps) {
  const method = config?.method || 'exact';
  const keyFields = config?.key_fields || [];
  const threshold = config?.threshold ?? 0.85;
  const keep = config?.keep || 'first';

  // Whether fuzzy/semantic methods require threshold
  const requiresThreshold = method === 'fuzzy' || method === 'semantic';

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Copy className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Deduplicator Configuration</h3>
      </div>

      {/* Deduplication Method */}
      <div className="space-y-2">
        <Label htmlFor="method" className="text-xs font-medium text-foreground">
          Detection Method <span className="text-red-500">*</span>
        </Label>
        <Select
          value={method}
          onValueChange={(value) => onUpdate({ method: value as 'exact' | 'fuzzy' | 'semantic' })}
        >
          <SelectTrigger id="method" className="text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="exact">Exact Match</SelectItem>
            <SelectItem value="fuzzy">Fuzzy Match</SelectItem>
            <SelectItem value="semantic">Semantic Match</SelectItem>
          </SelectContent>
        </Select>
        <div className="text-xs text-muted-foreground space-y-1 pl-3 border-l-2 border-border">
          {method === 'exact' && (
            <p>Exact string matching on key fields (fastest)</p>
          )}
          {method === 'fuzzy' && (
            <p>Fuzzy string matching using edit distance (good for typos)</p>
          )}
          {method === 'semantic' && (
            <p>Semantic similarity using embeddings (best for variations)</p>
          )}
        </div>
      </div>

      {/* Key Fields */}
      <div className="space-y-2">
        <Label htmlFor="key-fields" className="text-xs font-medium text-foreground">
          Key Fields <span className="text-red-500">*</span>
        </Label>
        <Input
          id="key-fields"
          type="text"
          placeholder="email, phone, name"
          value={keyFields.join(', ')}
          onChange={(e) => {
            const fields = e.target.value.split(',').map(f => f.trim()).filter(Boolean);
            onUpdate({ key_fields: fields });
          }}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Comma-separated list of fields to compare for duplicates
        </p>
      </div>

      {/* Threshold (for fuzzy/semantic) */}
      {requiresThreshold && (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <Label htmlFor="threshold" className="text-xs font-medium text-foreground">
              Match Threshold
            </Label>
            <span className="text-sm font-mono text-foreground">
              {(threshold * 100).toFixed(0)}%
            </span>
          </div>
          <Slider
            id="threshold"
            value={[threshold * 100]}
            onValueChange={(value) => onUpdate({ threshold: value[0] / 100 })}
            min={50}
            max={100}
            step={5}
            className="w-full"
          />
          <p className="text-xs text-muted-foreground">
            Minimum similarity score to consider records as duplicates (50-100%)
          </p>
        </div>
      )}

      {/* Keep Strategy */}
      <div className="space-y-2">
        <Label htmlFor="keep" className="text-xs font-medium text-foreground">
          Keep Strategy
        </Label>
        <Select
          value={keep}
          onValueChange={(value) => onUpdate({ keep: value as 'first' | 'last' | 'merge' })}
        >
          <SelectTrigger id="keep" className="text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="first">Keep First</SelectItem>
            <SelectItem value="last">Keep Last</SelectItem>
            <SelectItem value="merge">Merge Records</SelectItem>
          </SelectContent>
        </Select>
        <div className="text-xs text-muted-foreground space-y-1 pl-3 border-l-2 border-border">
          {keep === 'first' && (
            <p>Keep the first occurrence and discard duplicates</p>
          )}
          {keep === 'last' && (
            <p>Keep the last occurrence and discard duplicates</p>
          )}
          {keep === 'merge' && (
            <p>Merge all duplicate records into a single composite record</p>
          )}
        </div>
      </div>

      {/* Validation Messages */}
      {keyFields.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            At least one key field is required for deduplication
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      {keyFields.length > 0 && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Deduplication Summary</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Method:</span>
            <span className="font-medium text-foreground capitalize">{method}</span>
          </div>
          {requiresThreshold && (
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Threshold:</span>
              <span className="font-mono text-foreground">{(threshold * 100).toFixed(0)}%</span>
            </div>
          )}
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Keep:</span>
            <span className="font-medium text-foreground capitalize">{keep}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Key fields:</span>
            <span className="font-mono text-foreground text-right">{keyFields.length}</span>
          </div>
        </div>
      )}
    </div>
  );
}
