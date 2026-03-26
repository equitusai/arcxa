/**
 * Database Loader Configuration Form
 * Schema-aware target configuration for INSERT/UPSERT/REPLACE.
 */

import React, { useEffect, useMemo, useState } from 'react';
import { AlertCircle, Loader2, Search, Upload } from 'lucide-react';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import {
  getDatasourceReadinessMessage,
  isDatasourceReadyForOperation,
  inferDatasourceSchemaForWorkflow,
  type WorkflowDatasourceSchema,
} from '@/api/datasources';
import { useDatasources } from '@/hooks/useDatasources';
import type { DBLoaderConfig, DetectedField } from '@/lib/workflow-etl-config';

export interface DBLoaderConfigFormProps {
  config?: DBLoaderConfig;
  onUpdate: (updates: Partial<DBLoaderConfig>) => void;
  nodeId?: string;
  upstreamSchema?: DetectedField[];
}

function qualifiedIdentifierVariants(value: string): string[] {
  const segments = value
    .split('.')
    .map((segment) => segment.trim().replace(/^[`"]+|[`"]+$/g, '').toLowerCase())
    .filter(Boolean);

  const variants: string[] = [];
  for (let start = 0; start < segments.length; start += 1) {
    variants.push(segments.slice(start).join('.'));
  }

  return Array.from(new Set(variants));
}

function tableNameMatches(left: string, right: string): boolean {
  const leftVariants = qualifiedIdentifierVariants(left);
  const rightVariants = qualifiedIdentifierVariants(right);
  return leftVariants.some((variant) => rightVariants.includes(variant));
}

export function DBLoaderConfigForm({
  config,
  onUpdate,
  upstreamSchema = [],
}: DBLoaderConfigFormProps) {
  const { data: datasources, isLoading } = useDatasources();
  const [schema, setSchema] = useState<WorkflowDatasourceSchema | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(false);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [columnSearch, setColumnSearch] = useState('');

  const datasourceId = config?.datasource_id || '';
  const tableName = config?.table_name || '';
  const mode = config?.mode || 'insert';
  const keyFields = config?.key_fields || [];
  const batchSize = config?.batch_size || 1000;

  const selectedDatasource = datasources?.find((datasource) => datasource.id === datasourceId);
  const writableDatasources = (datasources || []).filter(
    (datasource) => isDatasourceReadyForOperation(datasource, 'workflowWrite')
  );
  const selectedDatasourceSupported =
    !selectedDatasource || isDatasourceReadyForOperation(selectedDatasource, 'workflowWrite');
  const datasourceOptions =
    selectedDatasource && !selectedDatasourceSupported
      ? [selectedDatasource, ...writableDatasources]
      : writableDatasources;

  const canInferSchema = selectedDatasource
    ? isDatasourceReadyForOperation(selectedDatasource, 'schemaInference')
    : false;
  const workflowWriteReadinessMessage = selectedDatasource
    ? getDatasourceReadinessMessage(selectedDatasource, 'workflowWrite')
    : null;
  const schemaReadinessMessage = selectedDatasource
    ? getDatasourceReadinessMessage(selectedDatasource, 'schemaInference')
    : null;

  useEffect(() => {
    let cancelled = false;

    if (!datasourceId || !canInferSchema) {
      setSchema(null);
      setSchemaError(null);
      setSchemaLoading(false);
      return;
    }

    setSchemaLoading(true);
    setSchemaError(null);

    inferDatasourceSchemaForWorkflow(datasourceId)
      .then((response) => {
        if (!cancelled) {
          setSchema(response);
        }
      })
      .catch((error: Error) => {
        if (!cancelled) {
          setSchema(null);
          setSchemaError(error.message || 'Schema inference failed');
        }
      })
      .finally(() => {
        if (!cancelled) {
          setSchemaLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [datasourceId, canInferSchema]);

  const inferredTables = schema?.tables || [];
  const selectedTableSchema = useMemo(
    () => inferredTables.find((table) => tableNameMatches(table.name, tableName)),
    [inferredTables, tableName]
  );

  const filteredTargetColumns = useMemo(() => {
    if (!selectedTableSchema) {
      return [];
    }

    if (!columnSearch.trim()) {
      return selectedTableSchema.columns;
    }

    const query = columnSearch.trim().toLowerCase();
    return selectedTableSchema.columns.filter((column) =>
      column.name.toLowerCase().includes(query)
    );
  }, [selectedTableSchema, columnSearch]);

  const targetColumnNames = useMemo(
    () => selectedTableSchema?.columns.map((column) => column.name) || [],
    [selectedTableSchema]
  );
  const upstreamFieldNames = useMemo(
    () => upstreamSchema.map((field) => field.name),
    [upstreamSchema]
  );

  const matchedColumns = useMemo(
    () => upstreamFieldNames.filter((field) => targetColumnNames.includes(field)),
    [targetColumnNames, upstreamFieldNames]
  );
  const upstreamOnlyColumns = useMemo(
    () => upstreamFieldNames.filter((field) => !targetColumnNames.includes(field)),
    [targetColumnNames, upstreamFieldNames]
  );
  const targetRequiredColumns = useMemo(
    () =>
      selectedTableSchema?.columns
        .filter((column) => column.nullable === false && !upstreamFieldNames.includes(column.name))
        .map((column) => column.name) || [],
    [selectedTableSchema, upstreamFieldNames]
  );
  const invalidKeyFields = useMemo(
    () => keyFields.filter((field) => !targetColumnNames.includes(field)),
    [keyFields, targetColumnNames]
  );

  useEffect(() => {
    if (mode !== 'upsert' || !selectedTableSchema) {
      return;
    }

    if (invalidKeyFields.length > 0) {
      onUpdate({
        key_fields: keyFields.filter((field) => targetColumnNames.includes(field)),
      });
    }
  }, [invalidKeyFields, keyFields, mode, onUpdate, selectedTableSchema, targetColumnNames]);

  const handleDatasourceChange = (value: string) => {
    onUpdate({
      datasource_id: value,
      table_name: '',
      key_fields: [],
    });
  };

  const handleTargetTableSelection = (value: string) => {
    onUpdate({
      table_name: value,
      key_fields: keyFields,
    });
  };

  const handleKeyFieldToggle = (fieldName: string, checked: boolean) => {
    const nextFields = checked
      ? Array.from(new Set([...keyFields, fieldName]))
      : keyFields.filter((field) => field !== fieldName);

    onUpdate({ key_fields: nextFields });
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Upload className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Database Loader Configuration</h3>
      </div>

      <div className="space-y-2">
        <Label htmlFor="datasource" className="text-xs font-medium text-foreground">
          Target Datasource <span className="text-red-500">*</span>
        </Label>
        <Select
          value={datasourceId}
          onValueChange={handleDatasourceChange}
          disabled={isLoading}
        >
          <SelectTrigger id="datasource" className="text-sm">
            <SelectValue placeholder={isLoading ? 'Loading...' : 'Select datasource...'} />
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
          Only datasources marked by the coordinator as workflow-writable are available here.
        </p>
      </div>

      {!isLoading && writableDatasources.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            No data sources currently support workflow loading. Register a writable target data
            source first.
          </div>
        </div>
      )}

      {!selectedDatasourceSupported && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            {workflowWriteReadinessMessage}
          </div>
        </div>
      )}

      {selectedDatasource && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Target Capability Summary</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Schema inference</span>
            <span className="font-medium text-foreground">
              {canInferSchema ? 'Available' : 'Manual table entry only'}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Workflow write</span>
            <span className="font-medium text-foreground">
              {isDatasourceReadyForOperation(selectedDatasource, 'workflowWrite')
                ? 'Enabled'
                : 'Blocked'}
            </span>
          </div>
        </div>
      )}

      {canInferSchema && (
        <div className="space-y-2">
          <Label htmlFor="target-table-picker" className="text-xs font-medium text-foreground">
            Target Table
          </Label>
          <Select
            value={tableName}
            onValueChange={handleTargetTableSelection}
            disabled={schemaLoading || inferredTables.length === 0}
          >
            <SelectTrigger id="target-table-picker" className="text-sm">
              <SelectValue
                placeholder={
                  schemaLoading
                    ? 'Loading tables...'
                    : inferredTables.length > 0
                    ? 'Select table...'
                    : 'No inferred tables'
                }
              />
            </SelectTrigger>
            <SelectContent>
              {inferredTables.map((table) => (
                <SelectItem key={table.name} value={table.name}>
                  {table.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      <div className="space-y-2">
        <Label htmlFor="table-name" className="text-xs font-medium text-foreground">
          Table Name <span className="text-red-500">*</span>
        </Label>
        <Input
          id="table-name"
          type="text"
          placeholder="customers_curated"
          value={tableName}
          onChange={(event) => onUpdate({ table_name: event.target.value })}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Use a discovered table when available, or enter the target table manually.
        </p>
      </div>

      {schemaLoading && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
          Loading target schema...
        </div>
      )}

      {schemaError && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Target schema inference failed. Manual target configuration is still available.
          </div>
        </div>
      )}

      {!schemaLoading && !canInferSchema && selectedDatasource && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            {schemaReadinessMessage}
          </div>
        </div>
      )}

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
            <SelectItem value="insert">Insert</SelectItem>
            <SelectItem value="upsert">Upsert</SelectItem>
            <SelectItem value="replace">Replace</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {mode === 'upsert' && (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label className="text-xs font-medium text-foreground">
              Key Fields <span className="text-red-500">*</span>
            </Label>
            <span className="text-xs text-muted-foreground">{keyFields.length} selected</span>
          </div>

          {selectedTableSchema ? (
            <>
              <div className="relative">
                <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-muted-foreground" />
                <Input
                  type="search"
                  value={columnSearch}
                  onChange={(event) => setColumnSearch(event.target.value)}
                  placeholder="Filter target columns..."
                  className="pl-8 text-sm"
                />
              </div>
              <ScrollArea className="h-40 rounded border border-border p-2">
                <div className="space-y-2">
                  {filteredTargetColumns.map((column) => (
                    <label
                      key={column.name}
                      className="flex items-center justify-between gap-3 rounded border border-transparent px-2 py-1.5 hover:border-border"
                    >
                      <div className="flex items-center gap-2 min-w-0">
                        <Checkbox
                          checked={keyFields.includes(column.name)}
                          onCheckedChange={(value) =>
                            handleKeyFieldToggle(column.name, Boolean(value))
                          }
                        />
                        <div className="min-w-0">
                          <div className="text-xs font-medium text-foreground truncate">
                            {column.name}
                          </div>
                          <div className="text-[11px] text-muted-foreground">
                            {column.type}
                            {column.primary_key ? ' • PK' : ''}
                            {column.nullable === false ? ' • required' : ''}
                          </div>
                        </div>
                      </div>
                    </label>
                  ))}
                </div>
              </ScrollArea>
            </>
          ) : (
            <Input
              id="key-fields"
              type="text"
              placeholder="customer_id, external_id"
              value={keyFields.join(', ')}
              onChange={(event) => {
                const fields = event.target.value
                  .split(',')
                  .map((field) => field.trim())
                  .filter(Boolean);
                onUpdate({ key_fields: fields });
              }}
              className="text-sm font-mono"
            />
          )}

          <p className="text-xs text-muted-foreground">
            Select the key fields used to match existing rows during upsert.
          </p>
        </div>
      )}

      <div className="space-y-2">
        <Label htmlFor="batch-size" className="text-xs font-medium text-foreground">
          Batch Size
        </Label>
        <Input
          id="batch-size"
          type="number"
          min="1"
          max="50000"
          value={batchSize}
          onChange={(event) =>
            onUpdate({ batch_size: Number.parseInt(event.target.value, 10) || 1000 })
          }
          className="text-sm"
        />
      </div>

      <div className="space-y-2 pt-2 border-t border-border">
        <Label className="text-xs font-medium text-foreground">Target Compatibility</Label>

        {upstreamSchema.length === 0 ? (
          <div className="flex items-start gap-2 p-3 bg-background-secondary border border-border rounded text-xs">
            <AlertCircle className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
            <div className="text-muted-foreground">
              Connect an upstream source or transformation to compare the output schema against the
              target table.
            </div>
          </div>
        ) : (
          <div className="rounded border border-border overflow-hidden text-xs">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-px bg-border">
              <div className="bg-background p-3 space-y-2">
                <div className="font-medium text-foreground">Matched Columns</div>
                <div className="text-muted-foreground">
                  {matchedColumns.length > 0 ? matchedColumns.join(', ') : 'None yet'}
                </div>
              </div>
              <div className="bg-background p-3 space-y-2">
                <div className="font-medium text-foreground">Upstream Only</div>
                <div className="text-muted-foreground">
                  {upstreamOnlyColumns.length > 0 ? upstreamOnlyColumns.join(', ') : 'None'}
                </div>
              </div>
              <div className="bg-background p-3 space-y-2">
                <div className="font-medium text-foreground">Target Required Only</div>
                <div className="text-muted-foreground">
                  {targetRequiredColumns.length > 0 ? targetRequiredColumns.join(', ') : 'None'}
                </div>
              </div>
              <div className="bg-background p-3 space-y-2">
                <div className="font-medium text-foreground">Invalid Key Fields</div>
                <div className="text-muted-foreground">
                  {invalidKeyFields.length > 0 ? invalidKeyFields.join(', ') : 'None'}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      {!datasourceId && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">Target datasource is required.</div>
        </div>
      )}

      {!tableName && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">Target table name is required.</div>
        </div>
      )}

      {mode === 'upsert' && keyFields.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Upsert mode requires one or more key fields.
          </div>
        </div>
      )}
    </div>
  );
}
