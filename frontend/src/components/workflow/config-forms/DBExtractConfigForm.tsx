/**
 * Database Extract Configuration Form
 * Configure database data extraction
 */

import React from 'react';
import { Database, AlertCircle } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { useDatasources } from '@/hooks/useDatasources';
import type { DBExtractConfig } from '@/lib/workflow-etl-config';

export interface DBExtractConfigFormProps {
  config?: DBExtractConfig;
  onUpdate: (updates: Partial<DBExtractConfig>) => void;
  nodeId?: string;
}

export function DBExtractConfigForm({ config, onUpdate }: DBExtractConfigFormProps) {
  const { data: datasources, isLoading } = useDatasources();

  const datasourceId = config?.datasource_id || '';
  const tableName = config?.table_name || '';
  const query = config?.query || '';
  const incremental = config?.incremental ?? false;
  const incrementalColumn = config?.incremental_column || '';
  const selectedDatasource = datasources?.find((datasource) => datasource.id === datasourceId);
  const readableDatasources = (datasources || []).filter(
    (datasource) => datasource.instance_capabilities?.canReadWorkflow ?? false
  );
  const selectedDatasourceSupported =
    !selectedDatasource || (selectedDatasource.instance_capabilities?.canReadWorkflow ?? false);
  const datasourceOptions =
    selectedDatasource && !selectedDatasourceSupported
      ? [selectedDatasource, ...readableDatasources]
      : readableDatasources;

  // Use either table_name or custom query
  const useCustomQuery = Boolean(query && !tableName);

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Database className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Database Extract Configuration</h3>
      </div>

      {/* Datasource Selection */}
      <div className="space-y-2">
        <Label htmlFor="datasource" className="text-xs font-medium text-foreground">
          Datasource <span className="text-red-500">*</span>
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
          Only datasources marked by the coordinator as workflow-readable are available here
        </p>
      </div>

      {!isLoading && readableDatasources.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            No data sources currently support workflow extraction. Add a data source with
            workflow read capability first.
          </div>
        </div>
      )}

      {!selectedDatasourceSupported && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            The selected datasource is no longer eligible for workflow extraction.
          </div>
        </div>
      )}

      {/* Table Name or Custom Query Toggle */}
      <div className="flex items-center justify-between py-2 border-y border-border">
        <div className="space-y-0.5">
          <Label className="text-xs font-medium text-foreground">
            Use custom SQL query
          </Label>
          <p className="text-xs text-muted-foreground">
            Write custom SQL instead of table name
          </p>
        </div>
        <Switch
          checked={useCustomQuery}
          onCheckedChange={(checked) => {
            if (checked) {
              onUpdate({ query: 'SELECT * FROM table_name', table_name: undefined });
            } else {
              onUpdate({ table_name: '', query: undefined });
            }
          }}
        />
      </div>

      {/* Table Name */}
      {!useCustomQuery && (
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
            Name of the table to extract data from
          </p>
        </div>
      )}

      {/* Custom Query */}
      {useCustomQuery && (
        <div className="space-y-2">
          <Label htmlFor="query" className="text-xs font-medium text-foreground">
            SQL Query <span className="text-red-500">*</span>
          </Label>
          <Textarea
            id="query"
            placeholder="SELECT * FROM customers WHERE created_at > '2024-01-01'"
            value={query}
            onChange={(e) => onUpdate({ query: e.target.value })}
            className="text-sm font-mono h-32"
          />
          <p className="text-xs text-muted-foreground">
            Custom SQL query to extract data
          </p>
        </div>
      )}

      {/* Incremental Loading */}
      <div className="space-y-3 pt-2 border-t border-border">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label htmlFor="incremental" className="text-xs font-medium text-foreground">
              Incremental loading
            </Label>
            <p className="text-xs text-muted-foreground">
              Only extract new/changed records
            </p>
          </div>
          <Switch
            id="incremental"
            checked={incremental}
            onCheckedChange={(checked) => onUpdate({ incremental: checked })}
          />
        </div>

        {/* Incremental Column */}
        {incremental && (
          <div className="space-y-2">
            <Label htmlFor="incremental-column" className="text-xs font-medium text-foreground">
              Incremental Column <span className="text-red-500">*</span>
            </Label>
            <Input
              id="incremental-column"
              type="text"
              placeholder="updated_at"
              value={incrementalColumn}
              onChange={(e) => onUpdate({ incremental_column: e.target.value })}
              className="text-sm font-mono"
            />
            <p className="text-xs text-muted-foreground">
              Column to track changes (e.g., timestamp, sequence ID)
            </p>
          </div>
        )}
      </div>

      {/* Validation Messages */}
      {!datasourceId && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Datasource selection is required to extract data
          </div>
        </div>
      )}

      {datasourceId && !tableName && !query && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Either table name or SQL query is required
          </div>
        </div>
      )}

      {incremental && !incrementalColumn && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Incremental column is required when incremental loading is enabled
          </div>
        </div>
      )}
    </div>
  );
}
