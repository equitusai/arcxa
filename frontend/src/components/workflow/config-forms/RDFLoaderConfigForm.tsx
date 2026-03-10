/**
 * RDF Loader Configuration Form
 * Configure RDF triple store loading with lineage capture
 */

import React from 'react';
import { Save, AlertCircle } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import type { RDFLoaderConfig } from '@/lib/workflow-etl-config';

export interface RDFLoaderConfigFormProps {
  config?: RDFLoaderConfig;
  onUpdate: (updates: Partial<RDFLoaderConfig>) => void;
  nodeId?: string;
}

export function RDFLoaderConfigForm({ config, onUpdate }: RDFLoaderConfigFormProps) {
  const targetGraph = config?.target_graph || '';
  const entityType = config?.entity_type || '';
  const idField = config?.id_field || '';
  const batchSize = config?.batch_size || 100;
  const captureLineage = config?.capture_lineage ?? true;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Save className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">RDF Loader Configuration</h3>
      </div>

      {/* Target Graph */}
      <div className="space-y-2">
        <Label htmlFor="target-graph" className="text-xs font-medium text-foreground">
          Target Graph (Optional)
        </Label>
        <Input
          id="target-graph"
          type="text"
          placeholder="http://example.org/graph/customers"
          value={targetGraph}
          onChange={(e) => onUpdate({ target_graph: e.target.value })}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Named graph URI (uses default graph if empty)
        </p>
      </div>

      {/* Entity Type */}
      <div className="space-y-2">
        <Label htmlFor="entity-type" className="text-xs font-medium text-foreground">
          Entity Type <span className="text-red-500">*</span>
        </Label>
        <Input
          id="entity-type"
          type="text"
          placeholder="schema:Person"
          value={entityType}
          onChange={(e) => onUpdate({ entity_type: e.target.value })}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          RDF type for entities (e.g., schema:Person, org:Organization)
        </p>
      </div>

      {/* ID Field */}
      <div className="space-y-2">
        <Label htmlFor="id-field" className="text-xs font-medium text-foreground">
          ID Field <span className="text-red-500">*</span>
        </Label>
        <Input
          id="id-field"
          type="text"
          placeholder="customer_id"
          value={idField}
          onChange={(e) => onUpdate({ id_field: e.target.value })}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Field to use as entity identifier/URI
        </p>
      </div>

      {/* Batch Size */}
      <div className="space-y-2">
        <Label htmlFor="batch-size" className="text-xs font-medium text-foreground">
          Batch Size
        </Label>
        <Input
          id="batch-size"
          type="number"
          min="1"
          max="1000"
          value={batchSize}
          onChange={(e) => onUpdate({ batch_size: parseInt(e.target.value) || 100 })}
          className="text-sm"
        />
        <p className="text-xs text-muted-foreground">
          Number of entities to load per batch (1-1000)
        </p>
      </div>

      {/* Capture Lineage */}
      <div className="flex items-center justify-between py-2 border-t border-border">
        <div className="space-y-0.5">
          <Label htmlFor="capture-lineage" className="text-xs font-medium text-foreground">
            Capture lineage
          </Label>
          <p className="text-xs text-muted-foreground">
            Track data provenance and transformation history
          </p>
        </div>
        <Switch
          id="capture-lineage"
          checked={captureLineage}
          onCheckedChange={(checked) => onUpdate({ capture_lineage: checked })}
        />
      </div>

      {/* Validation Messages */}
      {!entityType && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Entity type is required for RDF loading
          </div>
        </div>
      )}

      {!idField && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            ID field is required to generate entity URIs
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      {entityType && idField && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">RDF Load Summary</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Entity type:</span>
            <span className="font-mono text-foreground">{entityType}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">ID field:</span>
            <span className="font-mono text-foreground">{idField}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Batch size:</span>
            <span className="font-mono text-foreground">{batchSize}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Lineage:</span>
            <span className="font-medium text-foreground">{captureLineage ? 'Enabled' : 'Disabled'}</span>
          </div>
          {targetGraph && (
            <div className="pt-1 border-t border-border mt-1">
              <span className="text-muted-foreground">Graph:</span>
              <div className="font-mono text-foreground break-all text-xs mt-0.5">{targetGraph}</div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
