/**
 * Database Loader Configuration Form
 * Configure database data loading (INSERT/UPSERT/REPLACE)
 */

import React from 'react';
import { Upload, AlertCircle } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useDatasources } from '@/hooks/useDatasources';
import type { DBLoaderConfig } from '@/lib/workflow-etl-config';

export interface DBLoaderConfigFormProps {
  config?: DBLoaderConfig;
  onUpdate: (updates: Partial<DBLoaderConfig>) => void;
  nodeId?: string;
}

export function DBLoaderConfigForm({ config, onUpdate }: DBLoaderConfigFormProps) {
  const { data: datasources, isLoading } = useDatasources();

  const datasourceId = config?.datasource_id || '';
  const tableName = config?.table_name || '';
  const mode = config?.mode || 'insert';
  const keyFields = config?.key_fields || [];
  const batchSize = config?.batch_size || 1000;
  const selectedDatasource = datasources?.find((datasource) => datasource.id === datasourceId);
  const writableDatasources = (datasources || []).filter(
    (datasource) => datasource.instance_capabilities?.canWriteWorkflow ?? false
  );
  const selectedDatasourceSupported =
    !selectedDatasource || (selectedDatasource.instance_capabilities?.canWriteWorkflow ?? false);
  const datasourceOptions =
    selectedDatasource && !selectedDatasourceSupported
      ? [selectedDatasource, ...writableDatasources]
      : writableDatasources;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Upload className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Database Loader Configuration</h3>
      </div>

      {/* Datasource Selection */}
      <div className="space-y-2">
        <Label htmlFor="datasource" className="text-xs font-medium text-foreground">
          Target Datasource <span className="text-red-500">*</span>
        </Label>
        <Select
          value={datasourceId}
          onValueChange={(value) => onUpdate({ datasource_id: value })}
          disabled={isLoading}
        >
          <SelectTrigger id="datasource" className="text-sm">
            <SelectValue placeholder={isLoading ? "Loading..." : "Select datasource..."} />
          </SelectTrigger>
          <SelectContent>
            {datasourceOptions.map((ds) => (
              <SelectItem key={ds.id} value={ds.id}>
                {ds.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-xs text-muted-foreground">
          Only datasources marked by the coordinator as workflow-writable are available here
        </p>
      </div>

      {!isLoading && writableDatasources.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            No data sources currently support workflow loading. Add a writable target data source
            first.
          </div>
        </div>
      )}

      {!selectedDatasourceSupported && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            The selected datasource is no longer eligible for workflow loading.
          </div>
        </div>
      )}

      {/* Table Name */}
      <div className="space-y-2">
        <Label htmlFor="table-name" className="text-xs font-medium text-foreground">
          Table Name <span className="text-red-500">*</span>
        </Label>
        <Input
          id="table-name"
          type="text"
          placeholder="customers"
          value={tableName}
          onChange={(e) => onUpdate({ table_name: e.target.value })}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Target table to load data into
        </p>
      </div>

      {/* Load Mode */}
      <div className="space-y-2">
        <Label htmlFor="mode" className="text-xs font-medium text-foreground">
          Load Mode <span className="text-red-500">*</span>
        </Label>
        <Select
          value={mode}
          onValueChange={(value) => onUpdate({ mode: value as 'insert' | 'upsert' | 'replace' })}
        >
          <SelectTrigger id="mode" className="text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="insert">Insert - Add new records only</SelectItem>
            <SelectItem value="upsert">Upsert - Insert or update existing</SelectItem>
            <SelectItem value="replace">Replace - Truncate then insert</SelectItem>
          </SelectContent>
        </Select>
        <div className="text-xs text-muted-foreground space-y-1 pl-3 border-l-2 border-border">
          {mode === 'insert' && (
            <p>Inserts new records. Fails if key already exists.</p>
          )}
          {mode === 'upsert' && (
            <p>Updates existing records or inserts new ones. Requires key fields.</p>
          )}
          {mode === 'replace' && (
            <p><span className="text-red-600 font-medium">Warning:</span> Deletes all existing data before loading.</p>
          )}
        </div>
      </div>

      {/* Key Fields (for upsert mode) */}
      {mode === 'upsert' && (
        <div className="space-y-2">
          <Label htmlFor="key-fields" className="text-xs font-medium text-foreground">
            Key Fields <span className="text-red-500">*</span>
          </Label>
          <Input
            id="key-fields"
            type="text"
            placeholder="customer_id, email"
            value={keyFields.join(', ')}
            onChange={(e) => {
              const fields = e.target.value.split(',').map(f => f.trim()).filter(Boolean);
              onUpdate({ key_fields: fields });
            }}
            className="text-sm font-mono"
          />
          <p className="text-xs text-muted-foreground">
            Comma-separated list of fields to match on for updates
          </p>
        </div>
      )}

      {/* Batch Size */}
      <div className="space-y-2">
        <Label htmlFor="batch-size" className="text-xs font-medium text-foreground">
          Batch Size
        </Label>
        <Input
          id="batch-size"
          type="number"
          min="1"
          max="10000"
          value={batchSize}
          onChange={(e) => onUpdate({ batch_size: parseInt(e.target.value) || 1000 })}
          className="text-sm"
        />
        <p className="text-xs text-muted-foreground">
          Number of rows to insert per batch (1-10000)
        </p>
      </div>

      {/* Validation Messages */}
      {!datasourceId && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Target datasource is required
          </div>
        </div>
      )}

      {!tableName && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Table name is required
          </div>
        </div>
      )}

      {mode === 'upsert' && keyFields.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Key fields are required for upsert mode
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      {datasourceId && tableName && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Load Summary</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Mode:</span>
            <span className="font-medium text-foreground capitalize">{mode}</span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Batch size:</span>
            <span className="font-mono text-foreground">{batchSize.toLocaleString()}</span>
          </div>
          {mode === 'upsert' && keyFields.length > 0 && (
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Key fields:</span>
              <span className="font-mono text-foreground text-right">{keyFields.join(', ')}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
