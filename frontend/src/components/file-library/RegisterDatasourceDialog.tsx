/**
 * Register Datasource Dialog Component
 *
 * Allows users to promote a file to a datasource with configuration and validation.
 * Follows Oracle Redwood + Microsoft Fluent design principles for enterprise UX.
 */

import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Loader2,
  CheckCircle,
  AlertCircle,
  Database,
  FileText,
  AlertTriangle,
  ArrowRight,
  Info,
} from 'lucide-react';
import {
  useValidateFileForRegistration,
  useRegisterFileAsDatasource,
} from '@/hooks/useFileLibrary';
import type {
  FileMetadata,
  RegisterFileAsDatasourceRequest,
} from '@/api/types';
import { formatFileSize } from '@/api/fileLibrary';
import { cn } from '@/lib/utils';

interface RegisterDatasourceDialogProps {
  file: FileMetadata | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSuccess?: (datasourceId: string, datasetId?: string) => void;
}

export function RegisterDatasourceDialog({
  file,
  open,
  onOpenChange,
  onSuccess,
}: RegisterDatasourceDialogProps) {
  const navigate = useNavigate();

  // Form state
  const [datasourceName, setDatasourceName] = useState('');
  const [connectorType, setConnectorType] = useState<'CSVFile' | 'ExcelFile' | 'TSVFile'>('CSVFile');
  const [delimiter, setDelimiter] = useState(',');
  const [hasHeader, setHasHeader] = useState(true);
  const [encoding, setEncoding] = useState('utf-8');
  const [sheetName, setSheetName] = useState('');
  const [importToCatalogue, setImportToCatalogue] = useState(true);

  // Hooks
  const { data: validation, isLoading: validating } = useValidateFileForRegistration(
    file ? String(file.file_id || (file as any).id || '') : undefined
  );
  const registerMutation = useRegisterFileAsDatasource();

  // Auto-detect connector type from MIME type
  useEffect(() => {
    if (!file) return;

    const mimeType = String(file.mime_type || '');
    if (mimeType.includes('csv')) {
      setConnectorType('CSVFile');
      setDelimiter(',');
    } else if (mimeType.includes('excel') || mimeType.includes('spreadsheet')) {
      setConnectorType('ExcelFile');
    } else if (mimeType.includes('tab-separated') || mimeType.includes('tsv')) {
      setConnectorType('TSVFile');
      setDelimiter('\t');
    }

    // Auto-populate datasource name
    const filename = String(file.original_filename || 'file');
    const baseName = filename.replace(/\.[^/.]+$/, '');
    setDatasourceName(`${baseName} Datasource`);
  }, [file]);

  // Auto-populate from validation results
  useEffect(() => {
    if (!validation?.inferred_config) return;

    if (validation.inferred_config.delimiter) {
      setDelimiter(validation.inferred_config.delimiter);
    }
    if (validation.inferred_config.has_header !== undefined) {
      setHasHeader(validation.inferred_config.has_header);
    }
  }, [validation]);

  const handleRegister = async () => {
    if (!file) return;

    const request: RegisterFileAsDatasourceRequest = {
      datasource_name: datasourceName || `${String(file.original_filename || 'file')} Datasource`,
      connector_type: connectorType,
      parsing_config: {
        delimiter: connectorType === 'CSVFile' || connectorType === 'TSVFile' ? delimiter : undefined,
        has_header: hasHeader,
        encoding,
        sheet_name: connectorType === 'ExcelFile' ? sheetName || undefined : undefined,
      },
      import_to_catalogue: importToCatalogue,
    };

    try {
      const result = await registerMutation.mutateAsync({
        fileId: String(file.file_id || (file as any).id || ''),
        request,
      });

      // Call success callback
      onSuccess?.(result.datasource_id, result.dataset_id);

      // Close dialog
      onOpenChange(false);

      // Navigate to appropriate page
      if (result.dataset_id) {
        navigate(`/catalogue/${result.dataset_id}`);
      } else {
        navigate('/datasources');
      }
    } catch (error) {
      // Error already handled by mutation hook
    }
  };

  const canRegister = validation?.can_register && datasourceName.trim().length > 0;
  const isProcessing = registerMutation.isPending;

  if (!file) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="text-xl flex items-center gap-2">
            <Database className="h-5 w-5 text-primary" />
            Register File as Datasource
          </DialogTitle>
          <DialogDescription>
            Configure how this file should be registered as a datasource and optionally import to the catalogue
          </DialogDescription>
        </DialogHeader>

        <ScrollArea className="flex-1 pr-4">
          <div className="space-y-6 py-4">
            {/* Source File Information */}
            <Card className="border-2">
              <CardHeader className="pb-3">
                <CardTitle className="text-sm flex items-center gap-2">
                  <FileText className="h-4 w-4" />
                  Source File
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="flex items-center gap-3">
                  <div className="text-4xl">📄</div>
                  <div className="flex-1 min-w-0">
                    <div className="font-semibold text-base truncate" title={String(file.original_filename || 'Unknown')}>
                      {String(file.original_filename || 'Unknown')}
                    </div>
                    <div className="text-sm text-muted-foreground mt-1">
                      {formatFileSize(Number(file.size_bytes) || 0)} • Uploaded{' '}
                      {new Date(String(file.uploaded_at || Date.now())).toLocaleDateString()}
                    </div>
                  </div>
                  <Badge variant="outline" className="font-mono text-xs">
                    {String(file.mime_type || 'application/octet-stream').split('/')[1]?.toUpperCase() || 'FILE'}
                  </Badge>
                </div>
              </CardContent>
            </Card>

            {/* Validation Status */}
            {validating ? (
              <Card className="border-blue-200 bg-blue-50">
                <CardContent className="py-4">
                  <div className="flex items-center gap-3">
                    <Loader2 className="h-5 w-5 animate-spin text-blue-600" />
                    <div>
                      <div className="font-medium text-blue-900">Analyzing file structure...</div>
                      <div className="text-sm text-blue-700 mt-0.5">
                        Detecting schema, data types, and optimal parsing settings
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            ) : validation && !validation.can_register ? (
              <Card className="border-red-300 bg-red-50">
                <CardContent className="py-4">
                  <div className="flex items-start gap-3">
                    <AlertCircle className="h-5 w-5 text-red-600 mt-0.5 flex-shrink-0" />
                    <div className="flex-1">
                      <div className="font-semibold text-red-900 mb-2">
                        Cannot register this file
                      </div>
                      <ul className="space-y-1">
                        {validation.issues.map((issue, idx) => (
                          <li key={idx} className="text-sm text-red-800 flex items-start gap-2">
                            <span className="text-red-600 mt-0.5">•</span>
                            <span>{issue}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  </div>
                </CardContent>
              </Card>
            ) : validation ? (
              <Card className="border-green-300 bg-green-50">
                <CardContent className="py-4">
                  <div className="flex items-start gap-3">
                    <CheckCircle className="h-5 w-5 text-green-600 mt-0.5 flex-shrink-0" />
                    <div className="flex-1">
                      <div className="font-semibold text-green-900 mb-1">
                        File validated successfully
                      </div>
                      <div className="text-sm text-green-800">
                        Detected {validation.inferred_config.row_count?.toLocaleString() || 'unknown'} rows
                        {validation.inferred_config.column_count && (
                          <> and {validation.inferred_config.column_count} columns</>
                        )}
                      </div>
                      {validation.inferred_config.delimiter && (
                        <div className="text-sm text-green-700 mt-1">
                          Recommended delimiter: <code className="font-mono bg-green-100 px-1 rounded">
                            {validation.inferred_config.delimiter === '\t' ? '\\t (tab)' : validation.inferred_config.delimiter}
                          </code>
                        </div>
                      )}
                    </div>
                  </div>
                </CardContent>
              </Card>
            ) : null}

            {/* Configuration Form */}
            <div className="space-y-5">
              {/* Datasource Name */}
              <div>
                <Label htmlFor="datasource-name" className="text-sm font-semibold">
                  Datasource Name <span className="text-red-500">*</span>
                </Label>
                <Input
                  id="datasource-name"
                  value={datasourceName}
                  onChange={(e) => setDatasourceName(e.target.value)}
                  placeholder="Enter a descriptive name"
                  className="mt-2"
                  disabled={isProcessing}
                />
                <p className="text-xs text-muted-foreground mt-1.5">
                  This name will appear in the datasources list and workflows
                </p>
              </div>

              {/* Connector Type */}
              <div>
                <Label htmlFor="connector-type" className="text-sm font-semibold">
                  Connector Type
                </Label>
                <Select
                  value={connectorType}
                  onValueChange={(v: any) => setConnectorType(v)}
                  disabled={isProcessing}
                >
                  <SelectTrigger id="connector-type" className="mt-2">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="CSVFile">
                      <div className="flex items-center gap-2">
                        <span>📈</span>
                        <span>CSV File (Comma-separated values)</span>
                      </div>
                    </SelectItem>
                    <SelectItem value="TSVFile">
                      <div className="flex items-center gap-2">
                        <span>📊</span>
                        <span>TSV File (Tab-separated values)</span>
                      </div>
                    </SelectItem>
                    <SelectItem value="ExcelFile">
                      <div className="flex items-center gap-2">
                        <span>📊</span>
                        <span>Excel File (.xlsx, .xls)</span>
                      </div>
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {/* Parsing Configuration */}
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Parsing Configuration</CardTitle>
                </CardHeader>
                <CardContent className="space-y-4">
                  <div className="grid grid-cols-2 gap-4">
                    {/* Delimiter (for CSV/TSV) */}
                    {(connectorType === 'CSVFile' || connectorType === 'TSVFile') && (
                      <div>
                        <Label htmlFor="delimiter" className="text-sm">
                          Delimiter
                        </Label>
                        <Select value={delimiter} onValueChange={setDelimiter} disabled={isProcessing}>
                          <SelectTrigger id="delimiter" className="mt-1.5">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value=",">Comma (,)</SelectItem>
                            <SelectItem value=";">Semicolon (;)</SelectItem>
                            <SelectItem value="\t">Tab (\t)</SelectItem>
                            <SelectItem value="|">Pipe (|)</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                    )}

                    {/* Sheet Name (for Excel) */}
                    {connectorType === 'ExcelFile' && (
                      <div>
                        <Label htmlFor="sheet-name" className="text-sm">
                          Sheet Name (Optional)
                        </Label>
                        <Input
                          id="sheet-name"
                          value={sheetName}
                          onChange={(e) => setSheetName(e.target.value)}
                          placeholder="Leave empty for first sheet"
                          className="mt-1.5"
                          disabled={isProcessing}
                        />
                      </div>
                    )}

                    {/* Encoding */}
                    <div>
                      <Label htmlFor="encoding" className="text-sm">
                        Encoding
                      </Label>
                      <Select value={encoding} onValueChange={setEncoding} disabled={isProcessing}>
                        <SelectTrigger id="encoding" className="mt-1.5">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="utf-8">UTF-8 (Recommended)</SelectItem>
                          <SelectItem value="latin1">Latin-1 (ISO-8859-1)</SelectItem>
                          <SelectItem value="ascii">ASCII</SelectItem>
                          <SelectItem value="utf-16">UTF-16</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>
                  </div>

                  {/* Header Row Checkbox */}
                  <div className="flex items-center space-x-2 pt-2">
                    <Checkbox
                      id="has-header"
                      checked={hasHeader}
                      onCheckedChange={(checked) => setHasHeader(checked === true)}
                      disabled={isProcessing}
                    />
                    <Label
                      htmlFor="has-header"
                      className="text-sm cursor-pointer font-normal peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
                    >
                      First row contains column headers
                    </Label>
                  </div>

                  {!hasHeader && (
                    <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded-lg">
                      <AlertTriangle className="h-4 w-4 text-amber-600 mt-0.5 flex-shrink-0" />
                      <p className="text-sm text-amber-800">
                        Without headers, columns will be named sequentially (column_1, column_2, etc.)
                      </p>
                    </div>
                  )}
                </CardContent>
              </Card>

              {/* Import to Catalogue Option */}
              <Card className="border-2 border-primary/20">
                <CardContent className="py-4">
                  <div className="flex items-start gap-3">
                    <Checkbox
                      id="import-catalogue"
                      checked={importToCatalogue}
                      onCheckedChange={(checked) => setImportToCatalogue(checked === true)}
                      disabled={isProcessing}
                      className="mt-0.5"
                    />
                    <div className="flex-1">
                      <Label
                        htmlFor="import-catalogue"
                        className="text-sm font-semibold cursor-pointer"
                      >
                        Import to Data Catalogue after registration
                      </Label>
                      <p className="text-sm text-muted-foreground mt-1">
                        Automatically create a dataset entry in the catalogue with quality profiling and metadata
                      </p>
                    </div>
                  </div>
                </CardContent>
              </Card>

              {/* What Will Happen */}
              <Card className="bg-blue-50 border-blue-200">
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm flex items-center gap-2">
                    <Info className="h-4 w-4 text-blue-600" />
                    What will happen
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    <div className="flex items-start gap-2 text-sm">
                      <ArrowRight className="h-4 w-4 text-blue-600 mt-0.5 flex-shrink-0" />
                      <div>
                        <span className="font-medium">Data source created:</span>{' '}
                        <span className="text-muted-foreground">
                          "{datasourceName || 'Your data source'}" will appear in the Data Sources page
                        </span>
                      </div>
                    </div>
                    <div className="flex items-start gap-2 text-sm">
                      <ArrowRight className="h-4 w-4 text-blue-600 mt-0.5 flex-shrink-0" />
                      <div>
                        <span className="font-medium">File linked:</span>{' '}
                        <span className="text-muted-foreground">
                          This file will be linked to the datasource for lineage tracking
                        </span>
                      </div>
                    </div>
                    {importToCatalogue && (
                      <div className="flex items-start gap-2 text-sm">
                        <ArrowRight className="h-4 w-4 text-blue-600 mt-0.5 flex-shrink-0" />
                        <div>
                          <span className="font-medium">Dataset imported:</span>{' '}
                          <span className="text-muted-foreground">
                            Data will be profiled and added to the catalogue with quality metrics
                          </span>
                        </div>
                      </div>
                    )}
                    <div className="flex items-start gap-2 text-sm">
                      <ArrowRight className="h-4 w-4 text-blue-600 mt-0.5 flex-shrink-0" />
                      <div>
                        <span className="font-medium">Available in workflows:</span>{' '}
                        <span className="text-muted-foreground">
                          The datasource can be used in ETL workflows and transformations
                        </span>
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        </ScrollArea>

        {/* Actions Footer */}
        <div className="flex items-center justify-between gap-3 pt-4 border-t">
          <div className="text-sm text-muted-foreground">
            {!canRegister && !validating && validation && (
              <span className="text-amber-600 flex items-center gap-1">
                <AlertCircle className="h-3.5 w-3.5" />
                Please fix validation issues
              </span>
            )}
          </div>
          <div className="flex gap-2">
            <Button
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isProcessing}
            >
              Cancel
            </Button>
            <Button
              onClick={handleRegister}
              disabled={!canRegister || isProcessing || validating}
              className={cn(
                'min-w-[180px]',
                canRegister && 'bg-primary hover:bg-primary/90'
              )}
            >
              {isProcessing ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Registering...
                </>
              ) : (
                <>
                  <CheckCircle className="h-4 w-4 mr-2" />
                  Register & Continue
                </>
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
