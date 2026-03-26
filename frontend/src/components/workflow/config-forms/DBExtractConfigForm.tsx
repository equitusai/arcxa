/**
 * Database Extract Configuration Form
 * Datasource-backed extraction with schema inference and preview.
 */

import React, { useEffect, useMemo, useState } from 'react';
import { AlertCircle, Database, Loader2, Play, Search } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import {
  getDatasourceReadinessMessage,
  isDatasourceReadyForOperation,
  inferDatasourceSchemaForWorkflow,
  previewDatasourceQuery,
  type DatasourceQueryPreview,
  type WorkflowDatasourceSchema,
  type WorkflowSchemaField,
} from '@/api/datasources';
import { useDatasources } from '@/hooks/useDatasources';
import type { DBExtractConfig, DetectedField } from '@/lib/workflow-etl-config';
import { buildDatasourcePreviewQuery } from './shared/sqlIdentifiers';

export interface DBExtractConfigFormProps {
  config?: DBExtractConfig;
  onUpdate: (updates: Partial<DBExtractConfig>) => void;
  nodeId?: string;
}

function fieldsMatch(left?: DetectedField[], right?: DetectedField[]): boolean {
  return JSON.stringify(left || []) === JSON.stringify(right || []);
}

function workflowFieldToDetectedField(field: WorkflowSchemaField): DetectedField {
  return {
    name: field.name,
    type: field.type,
    nullable: field.nullable,
    primary_key: field.primary_key,
    sample_values: [],
  };
}

function tableToDetectedFields(
  columns: WorkflowSchemaField[],
  selectedColumns?: string[]
): DetectedField[] {
  const selected = new Set(selectedColumns || []);

  return columns
    .filter((column) => selected.size === 0 || selected.has(column.name))
    .map(workflowFieldToDetectedField);
}

function previewToDetectedFields(preview: DatasourceQueryPreview): DetectedField[] {
  return preview.columns.map((column) => ({
    name: column.name,
    type: column.type,
    nullable: column.nullable,
    primary_key: column.primary_key,
    sample_values: preview.rows
      .map((row) => row[column.name])
      .filter((value) => value !== null && value !== undefined)
      .slice(0, 5)
      .map((value) => String(value)),
  }));
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

export function DBExtractConfigForm({ config, onUpdate }: DBExtractConfigFormProps) {
  const { data: datasources, isLoading } = useDatasources();
  const [schema, setSchema] = useState<WorkflowDatasourceSchema | null>(null);
  const [schemaLoading, setSchemaLoading] = useState(false);
  const [schemaError, setSchemaError] = useState<string | null>(null);
  const [preview, setPreview] = useState<DatasourceQueryPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [columnSearch, setColumnSearch] = useState('');

  const datasourceId = config?.datasource_id || '';
  const tableName = config?.table_name || '';
  const schemaTable = config?.schema_table || '';
  const query = config?.query || '';
  const incremental = config?.incremental ?? false;
  const incrementalColumn = config?.incremental_column || '';
  const lastValue = config?.last_value;
  const batchSize = config?.batch_size || 50000;
  const schemaSampleSize = config?.schema_sample_size || 1000;
  const includeSchema = config?.include_schema ?? true;
  const configuredColumns = config?.columns || [];

  const selectedDatasource = datasources?.find((datasource) => datasource.id === datasourceId);
  const readableDatasources = (datasources || []).filter(
    (datasource) => isDatasourceReadyForOperation(datasource, 'workflowRead')
  );
  const selectedDatasourceSupported =
    !selectedDatasource || isDatasourceReadyForOperation(selectedDatasource, 'workflowRead');
  const datasourceOptions =
    selectedDatasource && !selectedDatasourceSupported
      ? [selectedDatasource, ...readableDatasources]
      : readableDatasources;

  const capabilities = selectedDatasource?.instance_capabilities;
  const canInferSchema = selectedDatasource
    ? isDatasourceReadyForOperation(selectedDatasource, 'schemaInference')
    : false;
  const canQuery = selectedDatasource
    ? isDatasourceReadyForOperation(selectedDatasource, 'query')
    : false;
  const supportsIncremental = capabilities?.supportsIncremental ?? false;
  const useCustomQuery = Boolean(query && !tableName);
  const workflowReadinessMessage = selectedDatasource
    ? getDatasourceReadinessMessage(selectedDatasource, 'workflowRead')
    : null;
  const schemaReadinessMessage = selectedDatasource
    ? getDatasourceReadinessMessage(selectedDatasource, 'schemaInference')
    : null;
  const queryReadinessMessage = selectedDatasource
    ? getDatasourceReadinessMessage(selectedDatasource, 'query')
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

    inferDatasourceSchemaForWorkflow(datasourceId, { sampleSize: schemaSampleSize })
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
  }, [datasourceId, canInferSchema, schemaSampleSize]);

  const inferredTables = schema?.tables || [];
  const selectedTableSchema = useMemo(() => {
    const activeTable = schemaTable || tableName;
    if (!activeTable) {
      return undefined;
    }
    return inferredTables.find((table) => tableNameMatches(table.name, activeTable));
  }, [inferredTables, schemaTable, tableName]);

  const selectedColumnNames = useMemo(() => {
    if (configuredColumns.length > 0) {
      return configuredColumns;
    }
    return selectedTableSchema?.columns.map((column) => column.name) || [];
  }, [configuredColumns, selectedTableSchema]);

  const filteredColumns = useMemo(() => {
    if (!selectedTableSchema) {
      return [];
    }

    if (!columnSearch.trim()) {
      return selectedTableSchema.columns;
    }

    const queryValue = columnSearch.trim().toLowerCase();
    return selectedTableSchema.columns.filter((column) =>
      column.name.toLowerCase().includes(queryValue)
    );
  }, [selectedTableSchema, columnSearch]);

  useEffect(() => {
    if (useCustomQuery || !selectedTableSchema) {
      return;
    }

    const detectedFields = tableToDetectedFields(selectedTableSchema.columns, configuredColumns);

    const needsSchemaTableUpdate = schemaTable !== selectedTableSchema.name;
    const needsDetectedFieldsUpdate = !fieldsMatch(config?.detected_fields, detectedFields);

    if (needsSchemaTableUpdate || needsDetectedFieldsUpdate) {
      onUpdate({
        schema_table: selectedTableSchema.name,
        detected_fields: detectedFields,
      });
    }
  }, [
    config?.detected_fields,
    configuredColumns,
    onUpdate,
    schemaTable,
    selectedTableSchema,
    useCustomQuery,
  ]);

  const handleDatasourceChange = (value: string) => {
    setPreview(null);
    setPreviewError(null);
    onUpdate({
      datasource_id: value,
      table_name: undefined,
      schema_table: undefined,
      query: undefined,
      columns: undefined,
      detected_fields: undefined,
      incremental: false,
      incremental_column: undefined,
      last_value: undefined,
    });
  };

  const handleToggleQueryMode = (checked: boolean) => {
    setPreview(null);
    setPreviewError(null);

    if (checked) {
      onUpdate({
        table_name: undefined,
        schema_table: schemaTable || tableName || undefined,
        query: query || 'SELECT * FROM table_name',
        detected_fields: undefined,
        incremental: false,
        incremental_column: undefined,
        last_value: undefined,
      });
      return;
    }

    onUpdate({
      table_name: tableName || '',
      query: undefined,
      schema_table: schemaTable || tableName || undefined,
    });
  };

  const handleTableSelection = (value: string) => {
    setPreview(null);
    setPreviewError(null);
    onUpdate({
      table_name: value,
      schema_table: value,
      query: undefined,
    });
  };

  const handleColumnToggle = (columnName: string, checked: boolean) => {
    if (!selectedTableSchema) {
      return;
    }

    const currentColumns =
      configuredColumns.length > 0
        ? configuredColumns
        : selectedTableSchema.columns.map((column) => column.name);

    const nextColumns = checked
      ? Array.from(new Set([...currentColumns, columnName]))
      : currentColumns.filter((column) => column !== columnName);

    const normalizedColumns =
      nextColumns.length === selectedTableSchema.columns.length ? undefined : nextColumns;

    onUpdate({
      columns: normalizedColumns,
      detected_fields: tableToDetectedFields(selectedTableSchema.columns, normalizedColumns),
    });
  };

  const handlePreview = async () => {
    if (!datasourceId || !canQuery) {
      return;
    }

    const previewQuery = useCustomQuery
      ? query.trim()
      : tableName.trim()
      ? buildDatasourcePreviewQuery(
          schemaTable || tableName,
          configuredColumns,
          selectedDatasource?.source_type
        )
      : '';

    if (!previewQuery) {
      setPreviewError('Provide a source table or SQL query before previewing.');
      return;
    }

    setPreviewLoading(true);
    setPreviewError(null);

    try {
      const response = await previewDatasourceQuery(datasourceId, {
        query: previewQuery,
        limit: 25,
      });
      setPreview(response);

      if (response.columns.length > 0) {
        onUpdate({
          detected_fields: previewToDetectedFields(response),
          schema_table: useCustomQuery ? schemaTable || undefined : schemaTable || tableName,
        });
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Preview failed';
      setPreviewError(message);
      setPreview(null);
    } finally {
      setPreviewLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <Database className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">Database Extract Configuration</h3>
      </div>

      <div className="space-y-2">
        <Label htmlFor="datasource" className="text-xs font-medium text-foreground">
          Datasource <span className="text-red-500">*</span>
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
          Only datasources marked by the coordinator as workflow-readable are available here.
        </p>
      </div>

      {!isLoading && readableDatasources.length === 0 && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            No data sources currently support workflow extraction. Register a workflow-readable
            datasource first.
          </div>
        </div>
      )}

      {!selectedDatasourceSupported && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            {workflowReadinessMessage}
          </div>
        </div>
      )}

      {selectedDatasource && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Datasource Capabilities</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Schema inference</span>
            <span className="font-medium text-foreground">
              {canInferSchema ? 'Available' : 'Manual configuration only'}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Query preview</span>
            <span className="font-medium text-foreground">
              {canQuery ? 'Available' : 'Not supported'}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Incremental extraction</span>
            <span className="font-medium text-foreground">
              {supportsIncremental ? 'Available' : 'Not supported'}
            </span>
          </div>
        </div>
      )}

      <div className="flex items-center justify-between py-2 border-y border-border">
        <div className="space-y-0.5">
          <Label className="text-xs font-medium text-foreground">Use custom SQL query</Label>
          <p className="text-xs text-muted-foreground">
            Switch between table-based extraction and custom query mode.
          </p>
        </div>
        <Switch
          checked={useCustomQuery}
          onCheckedChange={handleToggleQueryMode}
          disabled={Boolean(selectedDatasource && !canQuery)}
        />
      </div>

      {selectedDatasource && !canQuery && (
        <div className="flex items-start gap-2 p-3 bg-background-secondary border border-border rounded text-xs">
          <AlertCircle className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
          <div className="text-muted-foreground">
            {queryReadinessMessage}
          </div>
        </div>
      )}

      {!useCustomQuery && (
        <div className="space-y-3">
          {canInferSchema && (
            <div className="space-y-2">
              <Label htmlFor="table-picker" className="text-xs font-medium text-foreground">
                Source Table
              </Label>
              <Select
                value={tableName}
                onValueChange={handleTableSelection}
                disabled={schemaLoading || inferredTables.length === 0}
              >
                <SelectTrigger id="table-picker" className="text-sm">
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
              placeholder="customers"
              value={tableName}
              onChange={(event) =>
                onUpdate({
                  table_name: event.target.value,
                  schema_table: event.target.value,
                })
              }
              className="text-sm font-mono"
            />
            <p className="text-xs text-muted-foreground">
              Use a discovered table or enter a table name manually when schema inference is not
              available.
            </p>
          </div>

          {schemaLoading && (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
              Loading datasource schema...
            </div>
          )}

          {schemaError && (
            <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
              <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
              <div className="text-amber-800">
                Schema inference failed for this datasource. Manual table entry is still available.
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

          {selectedTableSchema && (
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-xs font-medium text-foreground">Columns</Label>
                <span className="text-xs text-muted-foreground">
                  {selectedColumnNames.length} selected
                </span>
              </div>
              <div className="relative">
                <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-muted-foreground" />
                <Input
                  type="search"
                  value={columnSearch}
                  onChange={(event) => setColumnSearch(event.target.value)}
                  placeholder="Filter columns..."
                  className="pl-8 text-sm"
                />
              </div>
              <ScrollArea className="h-40 rounded border border-border p-2">
                <div className="space-y-2">
                  {filteredColumns.map((column) => {
                    const checked = selectedColumnNames.includes(column.name);
                    return (
                      <label
                        key={column.name}
                        className="flex items-center justify-between gap-3 rounded border border-transparent px-2 py-1.5 hover:border-border"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <Checkbox
                            checked={checked}
                            onCheckedChange={(value) =>
                              handleColumnToggle(column.name, Boolean(value))
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
                    );
                  })}
                </div>
              </ScrollArea>
            </div>
          )}
        </div>
      )}

      {useCustomQuery && (
        <div className="space-y-3">
          <div className="space-y-2">
            <Label htmlFor="query" className="text-xs font-medium text-foreground">
              SQL Query <span className="text-red-500">*</span>
            </Label>
            <Textarea
              id="query"
              placeholder="SELECT * FROM customers WHERE updated_at >= CURRENT_DATE - INTERVAL '7 days'"
              value={query}
              onChange={(event) => onUpdate({ query: event.target.value, table_name: undefined })}
              className="text-sm font-mono h-32"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="schema-table" className="text-xs font-medium text-foreground">
              Schema Table Context
            </Label>
            {canInferSchema && inferredTables.length > 0 ? (
              <Select
                value={schemaTable}
                onValueChange={(value) => onUpdate({ schema_table: value })}
              >
                <SelectTrigger id="schema-table" className="text-sm">
                  <SelectValue placeholder="Select a table for schema context..." />
                </SelectTrigger>
                <SelectContent>
                  {inferredTables.map((table) => (
                    <SelectItem key={table.name} value={table.name}>
                      {table.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                id="schema-table"
                type="text"
                placeholder="customers"
                value={schemaTable}
                onChange={(event) => onUpdate({ schema_table: event.target.value })}
                className="text-sm font-mono"
              />
            )}
            <p className="text-xs text-muted-foreground">
              Optional table hint used for schema-aware downstream mapping when query mode is
              selected.
            </p>
          </div>

          {!canQuery && selectedDatasource && (
            <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
              <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
              <div className="text-amber-800">
                {queryReadinessMessage}
              </div>
            </div>
          )}
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-2 border-t border-border">
        <div className="space-y-2">
          <Label htmlFor="batch-size" className="text-xs font-medium text-foreground">
            Batch Size
          </Label>
          <Input
            id="batch-size"
            type="number"
            min="1"
            value={batchSize}
            onChange={(event) =>
              onUpdate({ batch_size: Number.parseInt(event.target.value, 10) || 50000 })
            }
            className="text-sm"
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="schema-sample-size" className="text-xs font-medium text-foreground">
            Schema Sample Size
          </Label>
          <Input
            id="schema-sample-size"
            type="number"
            min="1"
            value={schemaSampleSize}
            onChange={(event) =>
              onUpdate({
                schema_sample_size: Number.parseInt(event.target.value, 10) || 1000,
              })
            }
            className="text-sm"
          />
        </div>
      </div>

      <div className="flex items-center justify-between rounded border border-border p-3">
        <div className="space-y-0.5">
          <Label htmlFor="include-schema" className="text-xs font-medium text-foreground">
            Include schema in workflow output
          </Label>
          <p className="text-xs text-muted-foreground">
            Preserve inferred field metadata for ontology mapping and downstream validation.
          </p>
        </div>
        <Switch
          id="include-schema"
          checked={includeSchema}
          onCheckedChange={(checked) => onUpdate({ include_schema: checked })}
        />
      </div>

      {supportsIncremental && !useCustomQuery && (
        <div className="space-y-3 pt-2 border-t border-border">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="incremental" className="text-xs font-medium text-foreground">
                Incremental extraction
              </Label>
              <p className="text-xs text-muted-foreground">
                Only extract newly changed records when the connector supports it.
              </p>
            </div>
            <Switch
              id="incremental"
              checked={incremental}
              onCheckedChange={(checked) =>
                onUpdate(
                  checked
                    ? { incremental: true }
                    : {
                        incremental: false,
                        incremental_column: undefined,
                        last_value: undefined,
                      }
                )
              }
            />
          </div>

          {incremental && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div className="space-y-2">
                <Label htmlFor="incremental-column" className="text-xs font-medium text-foreground">
                  Incremental Column <span className="text-red-500">*</span>
                </Label>
                <Input
                  id="incremental-column"
                  type="text"
                  placeholder="updated_at"
                  value={incrementalColumn}
                  onChange={(event) => onUpdate({ incremental_column: event.target.value })}
                  className="text-sm font-mono"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="last-value" className="text-xs font-medium text-foreground">
                  Last Value <span className="text-red-500">*</span>
                </Label>
                <Input
                  id="last-value"
                  type="text"
                  placeholder="2026-01-01T00:00:00Z"
                  value={lastValue === undefined || lastValue === null ? '' : String(lastValue)}
                  onChange={(event) =>
                    onUpdate({
                      last_value: event.target.value.trim() ? event.target.value : undefined,
                    })
                  }
                  className="text-sm font-mono"
                />
              </div>
            </div>
          )}
        </div>
      )}

      {supportsIncremental && useCustomQuery && selectedDatasource && (
        <div className="flex items-start gap-2 p-3 bg-background-secondary border border-border rounded text-xs">
          <AlertCircle className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
          <div className="text-muted-foreground">
            Incremental extraction is only available in table mode. Custom queries must encode
            their own filtering explicitly.
          </div>
        </div>
      )}

      {!supportsIncremental && selectedDatasource && (
        <div className="flex items-start gap-2 p-3 bg-background-secondary border border-border rounded text-xs">
          <AlertCircle className="w-4 h-4 text-muted-foreground flex-shrink-0 mt-0.5" />
          <div className="text-muted-foreground">
            Incremental extraction is not advertised for this datasource.
          </div>
        </div>
      )}

      <div className="space-y-2 pt-2 border-t border-border">
        <div className="flex items-center justify-between">
          <div>
            <Label className="text-xs font-medium text-foreground">Source Preview</Label>
            <p className="text-xs text-muted-foreground">
              Run a limited preview against the exact source configuration used by this step. The
              coordinator applies the row limit for the selected datasource dialect.
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={handlePreview}
            disabled={previewLoading || !datasourceId || !canQuery}
            className="gap-2"
          >
            {previewLoading ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                Previewing...
              </>
            ) : (
              <>
                <Play className="w-4 h-4" />
                Preview
              </>
            )}
          </Button>
        </div>

        {previewError && (
          <div className="flex items-start gap-2 p-3 bg-red-50 border border-red-200 rounded text-xs">
            <AlertCircle className="w-4 h-4 text-red-600 flex-shrink-0 mt-0.5" />
            <div className="text-red-800">{previewError}</div>
          </div>
        )}

        {preview && (
          <div className="rounded border border-border overflow-hidden">
            <div className="flex items-center justify-between px-3 py-2 bg-background-secondary text-xs">
              <span className="font-medium text-foreground">
                {preview.row_count} row{preview.row_count !== 1 ? 's' : ''} returned
              </span>
              <span className="text-muted-foreground">
                {preview.execution_time_ms}ms
                {preview.truncated ? ' • truncated' : ''}
              </span>
            </div>
            <ScrollArea className="h-40">
              <pre className="p-3 text-[11px] leading-5 text-foreground whitespace-pre-wrap">
                {JSON.stringify(preview.rows, null, 2)}
              </pre>
            </ScrollArea>
          </div>
        )}
      </div>

      {!datasourceId && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">Datasource selection is required.</div>
        </div>
      )}

      {datasourceId && !tableName && !query && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Configure either a source table or a SQL query before execution.
          </div>
        </div>
      )}

      {incremental && !incrementalColumn && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Incremental extraction requires an incremental column.
          </div>
        </div>
      )}

      {incremental && (lastValue === undefined || lastValue === null || String(lastValue).trim() === '') && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Incremental extraction requires a last value to avoid silently running a full extract.
          </div>
        </div>
      )}
    </div>
  );
}
