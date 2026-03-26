/**
 * Dataset Import Wizard
 * Multi-step dialog to import datasets from connected datasources or file library
 */

import { useState, useEffect, useMemo } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Checkbox } from '@/components/ui/checkbox';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Progress } from '@/components/ui/progress';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Database,
  Table,
  CheckCircle,
  Loader2,
  AlertCircle,
  ArrowRight,
  ArrowLeft,
  Layers,
  FileText,
  FolderOpen
} from 'lucide-react';
import { useDatasources } from '@/hooks/useDatasources';
import { useFiles } from '@/hooks/useFileLibrary';
import { Datasource, FileMetadata } from '@/api/types';
import { discoverSchema } from '@/api/discovery';
import { importDatasourceTable, pollImportUntilComplete } from '@/api/imports';
import { useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { FieldMappingStep } from '@/components/field-mapping/FieldMappingStep';
import { formatFileSize, getFileIcon, getFileSchema, hasFileSchema, getSchemaFieldCount } from '@/api/fileLibrary';

interface DatasetImportWizardProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialDatasourceId?: string;
  initialTableName?: string;
}

interface DiscoveredTable {
  name: string;
  schema?: string;
  rowCount?: number;
  columnCount?: number;
  sizeBytes?: number;
  description?: string;
}

type WizardStep = 'select-datasource' | 'discover-tables' | 'field-mapping' | 'configure' | 'import';

export function DatasetImportWizard({
  open,
  onOpenChange,
  initialDatasourceId,
  initialTableName,
}: DatasetImportWizardProps) {
  const [step, setStep] = useState<WizardStep>('select-datasource');
  const [importSource, setImportSource] = useState<'datasource' | 'file-library'>('datasource');
  const [selectedDatasource, setSelectedDatasource] = useState<Datasource | null>(null);
  const [selectedFile, setSelectedFile] = useState<FileMetadata | null>(null);
  const [discoveredTables, setDiscoveredTables] = useState<DiscoveredTable[]>([]);
  const [selectedTables, setSelectedTables] = useState<Set<string>>(new Set());
  const [isDiscovering, setIsDiscovering] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [importProgress, setImportProgress] = useState(0);

  // Form data
  const [datasetName, setDatasetName] = useState('');
  const [datasetDescription, setDatasetDescription] = useState('');
  const [enableProfiling, setEnableProfiling] = useState(true);
  const [enableCdc, setEnableCdc] = useState(false);

  // Field mapping state
  const [, setMappingSessionId] = useState<string | null>(null);

  const { data: datasources, isLoading: loadingDatasources } = useDatasources();
  const { data: filesData, isLoading: loadingFiles } = useFiles({
    sort_by: 'uploaded_at',
    sort_order: 'desc',
    limit: 50,
  });
  const queryClient = useQueryClient();
  const importableDatasources = useMemo(
    () =>
      (datasources || []).filter(
        (datasource) =>
          (datasource.instance_capabilities?.canQuery ?? false) &&
          (datasource.instance_capabilities?.canInferSchema ?? false)
      ),
    [datasources]
  );

  // Filter files to show only registered datasources
  const registeredFiles = filesData?.files.filter(f => f.registration_status === 'registered') || [];

  // Handle pre-selection when dialog opens with initial values
  useEffect(() => {
    if (open && initialDatasourceId && importableDatasources.length > 0) {
      const datasource = importableDatasources.find((ds) => ds.id === initialDatasourceId);
      if (datasource && !selectedDatasource) {
        // Auto-select the datasource and discover tables
        setSelectedDatasource(datasource);
        setStep('discover-tables');
        discoverTables(datasource);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initialDatasourceId, importableDatasources]);

  const handleSelectDatasource = (datasource: Datasource) => {
    setSelectedDatasource(datasource);
    setImportSource('datasource');
    setStep('discover-tables');
    discoverTables(datasource);
  };

  const handleSelectFile = (file: FileMetadata) => {
    setSelectedFile(file);
    setImportSource('file-library');

    // Auto-populate dataset name from filename
    const baseName = file.original_filename.replace(/\.[^/.]+$/, '');
    setDatasetName(baseName.split('_').map(w =>
      w.charAt(0).toUpperCase() + w.slice(1)
    ).join(' '));

    // Skip directly to configure step (no need to discover tables for files)
    setStep('configure');
  };

  const discoverTables = async (datasource: Datasource) => {
    setIsDiscovering(true);
    setImportError(null);

    try {
      const response = await discoverSchema(datasource.id);

      const tables: DiscoveredTable[] = response.tables.map((table) => ({
        name: table.name,
        schema: response.name || undefined,
        rowCount: table.estimatedRows,
        columnCount: table.columns.length,
        // Estimate size based on row count (rough estimate)
        sizeBytes: table.estimatedRows ? table.estimatedRows * 100 : undefined,
        description: `Table from ${datasource.name}`,
      }));

      setDiscoveredTables(tables);

      // Auto-select initial table if provided, otherwise select first table
      if (tables.length > 0) {
        const tableToSelect = initialTableName
          ? tables.find(t => t.name === initialTableName)?.name || tables[0].name
          : tables[0].name;

        setSelectedTables(new Set([tableToSelect]));
        const selectedTable = tables.find(t => t.name === tableToSelect);
        if (selectedTable) {
          setDatasetName(selectedTable.name.split('_').map(w =>
            w.charAt(0).toUpperCase() + w.slice(1)
          ).join(' '));
        }
      } else {
        toast.info('No tables found in this datasource');
      }
    } catch (error) {
      setImportError(error instanceof Error ? error.message : 'Failed to discover tables');
      toast.error('Failed to discover tables from datasource');
    } finally {
      setIsDiscovering(false);
    }
  };

  const handleToggleTable = (tableName: string) => {
    const newSelected = new Set(selectedTables);
    if (newSelected.has(tableName)) {
      newSelected.delete(tableName);
    } else {
      newSelected.add(tableName);
    }
    setSelectedTables(newSelected);

    // Update dataset name if only one table selected
    if (newSelected.size === 1) {
      const table = discoveredTables.find(t => t.name === Array.from(newSelected)[0]);
      if (table) {
        setDatasetName(table.name.split('_').map(w =>
          w.charAt(0).toUpperCase() + w.slice(1)
        ).join(' '));
      }
    }
  };

  const handleNextToFieldMapping = () => {
    if (selectedTables.size === 0) {
      toast.error('Please select at least one table');
      return;
    }
    setStep('field-mapping');
  };

  const handleFieldMappingComplete = (sessionId: string | null) => {
    setMappingSessionId(sessionId);
    setStep('configure');
  };

  const handleSkipFieldMapping = () => {
    setMappingSessionId(null);
    setStep('configure');
  };

  const handleImport = async () => {
    if (!datasetName.trim()) {
      toast.error('Please enter a dataset name');
      return;
    }

    if (!selectedDatasource || selectedTables.size === 0) {
      toast.error('Please select a datasource and table');
      return;
    }

    setIsImporting(true);
    setImportError(null);
    setImportProgress(0);

    try {
      const tableNames = Array.from(selectedTables);
      const totalTables = tableNames.length;
      let completedTables = 0;
      const errors: string[] = [];

      // Import each selected table
      for (const tableName of tableNames) {
        const table = discoveredTables.find(t => t.name === tableName);

        if (!table) {
          errors.push(`Table ${tableName} not found`);
          continue;
        }

        try {
          // Generate unique dataset name for each table if importing multiple
          const currentDatasetName = totalTables > 1
            ? `${datasetName} - ${table.name.split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')}`
            : datasetName;

          // Call the real import API
          const response = await importDatasourceTable({
            datasource_id: selectedDatasource.id,
            table_name: tableName,
            schema: table.schema || undefined,
            dataset_name: currentDatasetName,
            description: datasetDescription,
            profile: enableProfiling,
            async_mode: false, // Let backend decide based on size
          });

          // Pending/processing responses indicate the backend moved the import to a background job.
          if (response.status === 'pending' || response.status === 'processing') {
            toast.info(`Importing ${tableName} in background...`);

            await pollImportUntilComplete(
              response.lineage.import_id,
              (progress) => {
                // Calculate overall progress across all tables
                const tableProgress = progress / totalTables;
                const overallProgress = Math.round((completedTables / totalTables) * 100 + tableProgress);
                setImportProgress(overallProgress);
              },
              1000, // Poll every second
              300000 // 5 minute timeout
            );
          }

          completedTables++;
          setImportProgress(Math.round((completedTables / totalTables) * 100));

          if (totalTables > 1) {
            toast.success(`Imported ${tableName} (${completedTables}/${totalTables})`);
          }
        } catch (tableError) {
          const errorMsg = tableError instanceof Error ? tableError.message : 'Unknown error';
          errors.push(`${tableName}: ${errorMsg}`);
          console.error(`Failed to import table ${tableName}:`, tableError);
        }
      }

      // Check results
      if (errors.length === totalTables) {
        // All imports failed
        throw new Error(`All imports failed:\n${errors.join('\n')}`);
      } else if (errors.length > 0) {
        // Some imports failed
        toast.warning(`Imported ${completedTables}/${totalTables} tables. ${errors.length} failed.`);
        setImportError(`Some imports failed:\n${errors.join('\n')}`);
      } else {
        // All imports succeeded
        const successMsg = totalTables === 1
          ? `Dataset "${datasetName}" imported successfully!`
          : `${totalTables} datasets imported successfully!`;
        toast.success(successMsg);
      }

      // Import succeeded (at least partially)
      setStep('import');

      // Refresh datasets list
      queryClient.invalidateQueries({ queryKey: ['datasets'] });

      // Close dialog after short delay if fully successful
      if (errors.length === 0) {
        setTimeout(() => {
          handleClose();
        }, 2000);
      } else {
        setIsImporting(false);
      }
    } catch (error) {
      setImportError(error instanceof Error ? error.message : 'Failed to import dataset');
      toast.error(error instanceof Error ? error.message : 'Failed to import dataset');
      setIsImporting(false);
    }
  };

  const handleClose = () => {
    setStep('select-datasource');
    setImportSource('datasource');
    setSelectedDatasource(null);
    setSelectedFile(null);
    setDiscoveredTables([]);
    setSelectedTables(new Set());
    setDatasetName('');
    setDatasetDescription('');
    setEnableProfiling(true);
    setEnableCdc(false);
    setImportError(null);
    setImportProgress(0);
    setIsImporting(false);
    onOpenChange(false);
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
  };

  const formatNumber = (num: number) => {
    return num.toLocaleString();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-hidden">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Layers className="h-5 w-5" />
            Import Dataset
          </DialogTitle>
          <DialogDescription>
            Import datasets from your connected datasources
          </DialogDescription>
        </DialogHeader>

        {/* Step Indicator */}
        <div className="flex items-center gap-2 pb-4 border-b">
          <StepIndicator
            step={1}
            label="Select"
            active={step === 'select-datasource'}
            completed={step !== 'select-datasource'}
          />
          <div className="flex-1 h-px bg-border" />
          <StepIndicator
            step={2}
            label="Discover"
            active={step === 'discover-tables'}
            completed={step === 'field-mapping' || step === 'configure' || step === 'import'}
          />
          <div className="flex-1 h-px bg-border" />
          <StepIndicator
            step={3}
            label="Map"
            active={step === 'field-mapping'}
            completed={step === 'configure' || step === 'import'}
          />
          <div className="flex-1 h-px bg-border" />
          <StepIndicator
            step={4}
            label="Configure"
            active={step === 'configure'}
            completed={step === 'import'}
          />
          <div className="flex-1 h-px bg-border" />
          <StepIndicator
            step={5}
            label="Import"
            active={step === 'import'}
            completed={false}
          />
        </div>

        {/* Step Content */}
        <ScrollArea className="max-h-[calc(90vh-250px)] pr-4">
          {step === 'select-datasource' && (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Select an import source
              </p>

              <Tabs defaultValue="datasource" className="w-full">
                <TabsList className="grid w-full grid-cols-2">
                  <TabsTrigger value="datasource" className="text-xs">
                    <Database className="h-3.5 w-3.5 mr-1.5" />
                    Connected Data Sources
                    {importableDatasources.length > 0 && (
                      <Badge variant="secondary" className="ml-1.5 text-xs">
                        {importableDatasources.length}
                      </Badge>
                    )}
                  </TabsTrigger>
                  <TabsTrigger value="file-library" className="text-xs">
                    <FolderOpen className="h-3.5 w-3.5 mr-1.5" />
                    File Library
                    {registeredFiles.length > 0 && (
                      <Badge variant="secondary" className="ml-1.5 text-xs">
                        {registeredFiles.length}
                      </Badge>
                    )}
                  </TabsTrigger>
                </TabsList>

                {/* Connected Data Sources Tab */}
                <TabsContent value="datasource" className="space-y-2 mt-4">
                  {loadingDatasources ? (
                    <div className="flex items-center justify-center py-8">
                      <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                    </div>
                  ) : importableDatasources.length > 0 ? (
                    <div className="space-y-2">
                      {importableDatasources.map((datasource) => (
                        <button
                          key={datasource.id}
                          onClick={() => handleSelectDatasource(datasource)}
                          className="w-full p-4 border rounded-lg hover:border-primary hover:bg-accent transition-colors text-left"
                        >
                          <div className="flex items-center justify-between">
                            <div className="flex items-center gap-3">
                              <Database className="h-5 w-5 text-primary" />
                              <div>
                                <div className="font-medium">{datasource.name}</div>
                                <div className="text-sm text-muted-foreground">
                                  {datasource.plugin_name} • {datasource.status === 'Connected' ? '🟢 Connected' : '🔴 Disconnected'}
                                </div>
                              </div>
                            </div>
                            <ArrowRight className="h-4 w-4 text-muted-foreground" />
                          </div>
                        </button>
                      ))}
                      <p className="text-xs text-muted-foreground pt-1">
                        Only datasources that support schema discovery and query execution are
                        available for dataset import.
                      </p>
                    </div>
                  ) : datasources && datasources.length > 0 ? (
                    <Alert>
                      <AlertCircle className="h-4 w-4" />
                      <AlertDescription>
                        No connected data sources currently support end-to-end dataset import.
                        Choose a datasource with schema discovery and query execution enabled.
                      </AlertDescription>
                    </Alert>
                  ) : (
                    <Alert>
                      <AlertCircle className="h-4 w-4" />
                      <AlertDescription>
                        No data sources connected. Please connect a data source first.
                      </AlertDescription>
                    </Alert>
                  )}
                </TabsContent>

                {/* File Library Tab */}
                <TabsContent value="file-library" className="space-y-2 mt-4">
                  {loadingFiles ? (
                    <div className="flex items-center justify-center py-8">
                      <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                    </div>
                  ) : registeredFiles.length > 0 ? (
                    <div className="space-y-2">
                      {registeredFiles.map((file) => {
                        const icon = getFileIcon(file.mime_type);
                        const uploadedDate = new Date(file.uploaded_at);
                        const formatRelativeTime = (date: Date) => {
                          const now = new Date();
                          const diffMs = now.getTime() - date.getTime();
                          const diffDays = Math.floor(diffMs / 86400000);
                          if (diffDays === 0) return 'Today';
                          if (diffDays === 1) return 'Yesterday';
                          if (diffDays < 7) return `${diffDays}d ago`;
                          return date.toLocaleDateString();
                        };

                        return (
                          <button
                            key={file.file_id}
                            onClick={() => handleSelectFile(file)}
                            className="w-full p-4 border rounded-lg hover:border-primary hover:bg-accent transition-colors text-left"
                          >
                            <div className="flex items-start gap-3">
                              <div className="text-2xl flex-shrink-0">{icon}</div>
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center justify-between gap-2 mb-1">
                                  <div className="font-medium truncate">{file.original_filename}</div>
                                  <ArrowRight className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                                </div>
                                <div className="flex items-center gap-2 text-xs text-muted-foreground flex-wrap">
                                  <span>{formatFileSize(file.size_bytes)}</span>
                                  <span>•</span>
                                  <span>{formatRelativeTime(uploadedDate)}</span>
                                  {hasFileSchema(file) && (() => {
                                    const schema = getFileSchema(file);
                                    const fieldCount = getSchemaFieldCount(file);
                                    return (
                                      <>
                                        <span>•</span>
                                        <span>{schema?.total_rows?.toLocaleString() || 'Unknown'} rows</span>
                                        <span>•</span>
                                        <span>{fieldCount} {fieldCount === 1 ? 'column' : 'columns'}</span>
                                      </>
                                    );
                                  })()}
                                </div>
                              </div>
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  ) : (
                    <Alert>
                      <AlertCircle className="h-4 w-4" />
                      <AlertDescription>
                        No registered file datasources found. Register files from the File Library to import them here.
                      </AlertDescription>
                    </Alert>
                  )}
                </TabsContent>
              </Tabs>
            </div>
          )}

          {step === 'discover-tables' && (
            <div className="space-y-4">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Database className="h-4 w-4" />
                <span>{selectedDatasource?.name}</span>
              </div>

              {isDiscovering ? (
                <div className="flex flex-col items-center justify-center py-12">
                  <Loader2 className="h-8 w-8 animate-spin text-primary mb-3" />
                  <p className="text-sm text-muted-foreground">Discovering tables...</p>
                </div>
              ) : importError ? (
                <Alert variant="destructive">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription>{importError}</AlertDescription>
                </Alert>
              ) : (
                <>
                  <p className="text-sm text-muted-foreground">
                    Found {discoveredTables.length} table{discoveredTables.length !== 1 ? 's' : ''}.
                    Select tables to import:
                  </p>

                  <div className="space-y-2">
                    {discoveredTables.map((table) => (
                      <div
                        key={table.name}
                        className={`p-4 border rounded-lg cursor-pointer transition-colors ${
                          selectedTables.has(table.name)
                            ? 'border-primary/50 bg-primary/5 ring-1 ring-primary/20'
                            : 'hover:border-border-emphasis'
                        }`}
                        onClick={() => handleToggleTable(table.name)}
                      >
                        <div className="flex items-start gap-3">
                          <Checkbox
                            checked={selectedTables.has(table.name)}
                            onCheckedChange={() => handleToggleTable(table.name)}
                            className="mt-1"
                          />
                          <div className="flex-1">
                            <div className="flex items-center gap-2 mb-1">
                              <Table className="h-4 w-4 text-primary" />
                              <span className="font-medium">{table.schema}.{table.name}</span>
                            </div>
                            {table.description && (
                              <p className="text-sm text-muted-foreground mb-2">
                                {table.description}
                              </p>
                            )}
                            <div className="flex gap-4 text-xs text-muted-foreground">
                              {table.rowCount && (
                                <span>{formatNumber(table.rowCount)} rows</span>
                              )}
                              {table.columnCount && (
                                <span>{table.columnCount} columns</span>
                              )}
                              {table.sizeBytes && (
                                <span>{formatBytes(table.sizeBytes)}</span>
                              )}
                            </div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}

          {step === 'field-mapping' && selectedDatasource && selectedTables.size > 0 && (
            <FieldMappingStep
              datasource={selectedDatasource}
              tableName={Array.from(selectedTables)[0]}
              onComplete={handleFieldMappingComplete}
              onSkip={handleSkipFieldMapping}
            />
          )}

          {step === 'configure' && (
            <div className="space-y-4">
              {/* Source Information */}
              <div className="flex items-center gap-2 text-sm text-muted-foreground mb-4">
                {importSource === 'datasource' && selectedDatasource ? (
                  <>
                    <Database className="h-4 w-4" />
                    <span>{selectedDatasource.name}</span>
                    <span>•</span>
                    <span>{selectedTables.size} table{selectedTables.size !== 1 ? 's' : ''} selected</span>
                  </>
                ) : importSource === 'file-library' && selectedFile ? (
                  <>
                    <FileText className="h-4 w-4" />
                    <span>{selectedFile.original_filename}</span>
                    <span>•</span>
                    <span>{formatFileSize(selectedFile.size_bytes)}</span>
                    {hasFileSchema(selectedFile) && (() => {
                      const schema = getFileSchema(selectedFile);
                      return (
                        <>
                          <span>•</span>
                          <span>{schema?.total_rows?.toLocaleString() || 'Unknown'} rows</span>
                        </>
                      );
                    })()}
                  </>
                ) : null}
              </div>

              <div className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="name">Dataset Name *</Label>
                  <Input
                    id="name"
                    value={datasetName}
                    onChange={(e) => setDatasetName(e.target.value)}
                    placeholder="Enter dataset name"
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="description">Description</Label>
                  <Textarea
                    id="description"
                    value={datasetDescription}
                    onChange={(e) => setDatasetDescription(e.target.value)}
                    placeholder="Describe this dataset..."
                    rows={3}
                  />
                </div>

                <div className="space-y-3 pt-2 border-t">
                  <Label>Import Options</Label>

                  <div className="flex items-start gap-3">
                    <Checkbox
                      id="profiling"
                      checked={enableProfiling}
                      onCheckedChange={(checked: boolean) => setEnableProfiling(checked)}
                    />
                    <div className="flex-1">
                      <label htmlFor="profiling" className="text-sm font-medium cursor-pointer">
                        Run Data Profiling
                      </label>
                      <p className="text-xs text-muted-foreground">
                        Analyze data quality and generate quality metrics
                      </p>
                    </div>
                  </div>

                  <div className="flex items-start gap-3">
                    <Checkbox
                      id="cdc"
                      checked={enableCdc}
                      onCheckedChange={(checked: boolean) => setEnableCdc(checked)}
                    />
                    <div className="flex-1">
                      <label htmlFor="cdc" className="text-sm font-medium cursor-pointer">
                        Enable CDC (Change Data Capture)
                      </label>
                      <p className="text-xs text-muted-foreground">
                        Track changes to this dataset in real-time
                      </p>
                    </div>
                  </div>
                </div>

                {/* Import Progress */}
                {isImporting && importProgress > 0 && (
                  <div className="space-y-2 pt-4 border-t">
                    <div className="flex justify-between text-sm">
                      <span className="text-muted-foreground">Importing dataset...</span>
                      <span className="font-medium">{importProgress}%</span>
                    </div>
                    <Progress value={importProgress} className="h-2" />
                    <p className="text-xs text-muted-foreground">
                      This may take a few moments for large tables
                    </p>
                  </div>
                )}

                {/* Import Error */}
                {importError && (
                  <Alert variant="destructive" className="mt-4">
                    <AlertCircle className="h-4 w-4" />
                    <AlertDescription>{importError}</AlertDescription>
                  </Alert>
                )}
              </div>
            </div>
          )}

          {step === 'import' && (
            <div className="flex flex-col items-center justify-center py-12">
              <CheckCircle className="h-16 w-16 text-green-600 mb-4" />
              <h3 className="text-lg font-semibold mb-2">Import Successful!</h3>
              <p className="text-sm text-muted-foreground text-center">
                {selectedTables.size === 1
                  ? `Dataset "${datasetName}" has been imported and is now available in your catalogue.`
                  : `${selectedTables.size} datasets have been imported and are now available in your catalogue.`
                }
              </p>
              {importError && (
                <Alert className="mt-4 max-w-md bg-yellow-50 border-yellow-200">
                  <AlertCircle className="h-4 w-4 text-yellow-600" />
                  <AlertDescription className="text-xs whitespace-pre-wrap text-yellow-800">
                    {importError}
                  </AlertDescription>
                </Alert>
              )}
            </div>
          )}
        </ScrollArea>

        {/* Actions */}
        <div className="flex justify-between pt-4 border-t">
          {step === 'select-datasource' && (
            <>
              <div />
              <Button variant="outline" onClick={handleClose}>
                Cancel
              </Button>
            </>
          )}

          {step === 'discover-tables' && (
            <>
              <Button variant="outline" onClick={() => setStep('select-datasource')}>
                <ArrowLeft className="h-4 w-4 mr-2" />
                Back
              </Button>
              <Button
                onClick={handleNextToFieldMapping}
                disabled={selectedTables.size === 0 || isDiscovering}
              >
                Next
                <ArrowRight className="h-4 w-4 ml-2" />
              </Button>
            </>
          )}

          {step === 'field-mapping' && (
            <>{/* Field mapping step has its own navigation buttons */}</>
          )}

          {step === 'configure' && (
            <>
              <Button
                variant="outline"
                onClick={() => {
                  // Go back to appropriate step based on import source
                  if (importSource === 'file-library') {
                    setStep('select-datasource');
                  } else {
                    setStep('field-mapping');
                  }
                }}
              >
                <ArrowLeft className="h-4 w-4 mr-2" />
                Back
              </Button>
              <Button onClick={handleImport} disabled={!datasetName.trim() || isImporting}>
                {isImporting ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Importing...
                  </>
                ) : (
                  <>
                    Import Dataset
                    <ArrowRight className="h-4 w-4 ml-2" />
                  </>
                )}
              </Button>
            </>
          )}

          {step === 'import' && (
            <>
              <div />
              <Button onClick={handleClose}>
                Done
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function StepIndicator({ step, label, active, completed }: {
  step: number;
  label: string;
  active: boolean;
  completed: boolean;
}) {
  return (
    <div className="flex flex-col items-center gap-1">
      <div
        className={`w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium transition-colors ${
          completed
            ? 'bg-primary text-primary-foreground'
            : active
            ? 'bg-primary text-primary-foreground'
            : 'bg-muted text-muted-foreground'
        }`}
      >
        {completed ? <CheckCircle className="h-4 w-4" /> : step}
      </div>
      <span className={`text-xs ${active ? 'text-foreground font-medium' : 'text-muted-foreground'}`}>
        {label}
      </span>
    </div>
  );
}
