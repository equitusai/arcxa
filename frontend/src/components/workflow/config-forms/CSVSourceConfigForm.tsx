/**
 * CSV Source Configuration Form
 * Configure CSV file import settings with auto-detection
 */

import React, { useState } from 'react';
import { FileText, Search, CheckCircle, AlertCircle, Loader2, FolderOpen, Link2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { useScanCSV } from '@/hooks/useETL';
import { FilePickerDialog } from '@/components/file-library/FilePickerDialog';
import { getFileSchema, hasFileSchema } from '@/api/fileLibrary';
import type { CSVSourceConfig } from '@/lib/workflow-etl-config';
import type { ScanCSVRequest } from '@/api/etl';
import type { FileMetadata } from '@/api/types';

export interface CSVSourceConfigFormProps {
  config?: CSVSourceConfig;
  onUpdate: (updates: Partial<CSVSourceConfig>) => void;
  nodeId?: string;
}

export function CSVSourceConfigForm({ config, onUpdate }: CSVSourceConfigFormProps) {
  const scanCSV = useScanCSV();
  const [isScanning, setIsScanning] = useState(false);
  const [showFilePicker, setShowFilePicker] = useState(false);

  // Local state for form values
  const filePath = config?.file_path || '';
  const delimiter = config?.delimiter || ',';
  const hasHeader = config?.has_header ?? true;
  const encoding = config?.encoding || 'utf-8';
  const skipRows = config?.skip_rows || 0;
  const maxRows = config?.max_rows;

  const handleFileSelect = (file: FileMetadata) => {
    // Store file_id and metadata (backend will use file_id to load file)
    onUpdate({
      file_id: file.file_id,
      file_name: file.filename,
      file_path: `/file-library/${file.file_id}`, // For backward compatibility
    });

    // Auto-detect delimiter from MIME type
    if (file.mime_type.includes('csv')) {
      onUpdate({ delimiter: ',' });
    } else if (file.mime_type.includes('tab-separated') || file.mime_type.includes('tsv')) {
      onUpdate({ delimiter: '\t' });
    }

    // If file has schema, auto-populate detected fields
    if (hasFileSchema(file)) {
      const schema = getFileSchema(file);
      if (schema?.fields) {
        onUpdate({
          detected_fields: schema.fields.map((field) => ({
            name: field.name,
            type: field.type || field.name, // Handle both old and new formats
            sample_values: field.sample_values || [],
          })),
        });
      }
    }

    setShowFilePicker(false);
  };

  const handleScanFile = async () => {
    if (!filePath) {
      return;
    }

    setIsScanning(true);
    try {
      const scanRequest: ScanCSVRequest = {
        file_path: filePath,
        delimiter,
        has_header: hasHeader,
        encoding,
        sample_rows: 100,
      };

      const result = await scanCSV.mutateAsync(scanRequest);

      // Update config with detected fields and ontology mappings
      onUpdate({
        detected_fields: result.detected_fields,
        ontology_mappings: result.ontology_mappings,
        last_scanned: result.scan_timestamp,
        delimiter: result.delimiter_detected || delimiter,
        encoding: result.encoding_detected || encoding,
      });
    } catch (error) {
      console.error('CSV scan failed:', error);
    } finally {
      setIsScanning(false);
    }
  };

  const detectedFieldCount = config?.detected_fields?.length || 0;
  const hasDetectedFields = detectedFieldCount > 0;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <FileText className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">CSV Source Configuration</h3>
      </div>

      {/* File Selection */}
      <div className="space-y-2">
        <Label className="text-xs font-medium text-foreground">
          Source File <span className="text-red-500">*</span>
        </Label>

        {config?.file_id ? (
          /* Selected File Display */
          <div className="border border-border rounded-md p-3 bg-muted/30">
            <div className="flex items-start justify-between gap-2 mb-2">
              <div className="flex items-center gap-2 flex-1 min-w-0">
                <FileText className="h-4 w-4 text-primary flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-foreground truncate">
                    {config.file_name || 'Selected file'}
                  </div>
                  <div className="text-xs text-muted-foreground font-mono">
                    ID: {config.file_id}
                  </div>
                </div>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowFilePicker(true)}
                className="flex-shrink-0"
              >
                Change
              </Button>
            </div>
            {hasDetectedFields && (
              <div className="flex items-center gap-2 pt-2 border-t border-border">
                <CheckCircle className="w-3.5 h-3.5 text-green-600" />
                <span className="text-xs text-muted-foreground">
                  {detectedFieldCount} fields detected
                </span>
              </div>
            )}
          </div>
        ) : (
          /* No File Selected */
          <Button
            variant="outline"
            onClick={() => setShowFilePicker(true)}
            className="w-full h-auto py-4 border-dashed border-2 hover:bg-muted"
          >
            <div className="flex flex-col items-center gap-2">
              <FolderOpen className="h-8 w-8 text-muted-foreground" />
              <div className="text-sm font-medium text-foreground">
                Select CSV File from Library
              </div>
              <div className="text-xs text-muted-foreground">
                Click to browse uploaded files
              </div>
            </div>
          </Button>
        )}

        <p className="text-xs text-muted-foreground">
          Select a CSV or TSV file that has been uploaded to the File Library
        </p>
      </div>

      {/* Delimiter */}
      <div className="space-y-2">
        <Label htmlFor="delimiter" className="text-xs font-medium text-foreground">
          Delimiter
        </Label>
        <Select
          value={delimiter}
          onValueChange={(value) => onUpdate({ delimiter: value })}
        >
          <SelectTrigger id="delimiter" className="text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value=",">Comma (,)</SelectItem>
            <SelectItem value="\t">Tab (\t)</SelectItem>
            <SelectItem value=";">Semicolon (;)</SelectItem>
            <SelectItem value="|">Pipe (|)</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Has Header Row */}
      <div className="flex items-center justify-between py-2">
        <div className="space-y-0.5">
          <Label htmlFor="has-header" className="text-xs font-medium text-foreground">
            First row is header
          </Label>
          <p className="text-xs text-muted-foreground">
            Use first row as column names
          </p>
        </div>
        <Switch
          id="has-header"
          checked={hasHeader}
          onCheckedChange={(checked) => onUpdate({ has_header: checked })}
        />
      </div>

      {/* Encoding */}
      <div className="space-y-2">
        <Label htmlFor="encoding" className="text-xs font-medium text-foreground">
          Encoding
        </Label>
        <Select
          value={encoding}
          onValueChange={(value) => onUpdate({ encoding: value })}
        >
          <SelectTrigger id="encoding" className="text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="utf-8">UTF-8</SelectItem>
            <SelectItem value="utf-16">UTF-16</SelectItem>
            <SelectItem value="latin1">Latin-1</SelectItem>
            <SelectItem value="ascii">ASCII</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Skip Rows */}
      <div className="space-y-2">
        <Label htmlFor="skip-rows" className="text-xs font-medium text-foreground">
          Skip Rows
        </Label>
        <Input
          id="skip-rows"
          type="number"
          min="0"
          placeholder="0"
          value={skipRows}
          onChange={(e) => onUpdate({ skip_rows: parseInt(e.target.value) || 0 })}
          className="text-sm"
        />
        <p className="text-xs text-muted-foreground">
          Number of rows to skip at start
        </p>
      </div>

      {/* Max Rows */}
      <div className="space-y-2">
        <Label htmlFor="max-rows" className="text-xs font-medium text-foreground">
          Max Rows (Optional)
        </Label>
        <Input
          id="max-rows"
          type="number"
          min="1"
          placeholder="All rows"
          value={maxRows || ''}
          onChange={(e) => {
            const value = e.target.value ? parseInt(e.target.value) : undefined;
            onUpdate({ max_rows: value });
          }}
          className="text-sm"
        />
        <p className="text-xs text-muted-foreground">
          Limit number of rows to import
        </p>
      </div>

      {/* Scan Button */}
      <div className="pt-2">
        <Button
          onClick={handleScanFile}
          disabled={!filePath || isScanning}
          className="w-full"
          variant="secondary"
        >
          {isScanning ? (
            <>
              <Loader2 className="w-4 h-4 mr-2 animate-spin" />
              Scanning...
            </>
          ) : (
            <>
              <Search className="w-4 h-4 mr-2" />
              Scan File
            </>
          )}
        </Button>
      </div>

      {/* Scan Results */}
      {hasDetectedFields && (
        <div className="pt-2 space-y-2 border-t border-border">
          <div className="flex items-center gap-2 text-xs">
            <CheckCircle className="w-3.5 h-3.5 text-green-600" />
            <span className="font-medium text-foreground">
              {detectedFieldCount} fields detected
            </span>
            {config?.last_scanned && (
              <span className="text-muted-foreground ml-auto">
                {new Date(config.last_scanned).toLocaleTimeString()}
              </span>
            )}
          </div>

          {/* Field List */}
          <div className="space-y-1 max-h-48 overflow-y-auto">
            {config?.detected_fields?.map((field, idx) => (
              <div
                key={idx}
                className="flex items-center justify-between p-2 bg-background-secondary rounded text-xs"
              >
                <div className="flex items-center gap-2">
                  <span className="font-mono font-medium text-foreground">
                    {field.name}
                  </span>
                  <span className="text-muted-foreground">·</span>
                  <span className="text-accent text-xs">{field.type}</span>
                </div>
              </div>
            ))}
          </div>

          {/* Sample Values Preview */}
          {config?.detected_fields?.[0]?.sample_values && (
            <div className="text-xs">
              <div className="font-medium text-foreground mb-1">Sample values:</div>
              <div className="font-mono text-muted-foreground bg-background-secondary p-2 rounded overflow-x-auto">
                {config.detected_fields[0].sample_values.slice(0, 3).join(', ')}
                {config.detected_fields[0].sample_values.length > 3 && ', ...'}
              </div>
            </div>
          )}

          {/* Ontology Mappings */}
          {config?.ontology_mappings && config.ontology_mappings.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-border">
              <div className="flex items-center gap-2 text-xs">
                <Link2 className="w-3.5 h-3.5 text-blue-600" />
                <span className="font-medium text-foreground">
                  {config.ontology_mappings.length} ontology {config.ontology_mappings.length === 1 ? 'mapping' : 'mappings'}
                </span>
                <span className="text-muted-foreground ml-auto">
                  {config.ontology_mappings[0].ontology_id}
                </span>
              </div>

              {/* Mapping List */}
              <div className="space-y-1 max-h-32 overflow-y-auto">
                {config.ontology_mappings.map((mapping, idx) => (
                  <div
                    key={idx}
                    className="flex items-center justify-between p-2 bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded text-xs"
                  >
                    <div className="flex items-center gap-2 flex-1 min-w-0">
                      <span className="font-mono font-medium text-foreground truncate">
                        {mapping.field_name}
                      </span>
                      <Link2 className="w-3 h-3 text-blue-600 flex-shrink-0" />
                      <span className="text-blue-600 font-medium truncate">
                        {mapping.concept_label}
                      </span>
                    </div>
                    <div className="flex items-center gap-2 flex-shrink-0 ml-2">
                      <span className="text-muted-foreground">
                        {(mapping.confidence * 100).toFixed(0)}%
                      </span>
                      <span className="text-xs px-1.5 py-0.5 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded">
                        {mapping.method}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Error State */}
      {scanCSV.isError && (
        <div className="flex items-start gap-2 p-3 bg-red-50 border border-red-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-red-600 flex-shrink-0 mt-0.5" />
          <div className="space-y-1">
            <div className="font-medium text-red-800">Scan failed</div>
            <div className="text-red-600">
              {scanCSV.error instanceof Error ? scanCSV.error.message : 'Unknown error'}
            </div>
          </div>
        </div>
      )}

      {/* Validation Messages */}
      {!config?.file_id && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Please select a CSV file from the File Library to continue
          </div>
        </div>
      )}

      {/* File Picker Dialog */}
      <FilePickerDialog
        open={showFilePicker}
        onOpenChange={setShowFilePicker}
        onSelectFile={handleFileSelect}
        allowedMimeTypes={['text/csv', 'text/tab-separated-values', 'text/tsv']}
        allowedExtensions={['.csv', '.tsv']}
        title="Select CSV File"
        description="Choose a CSV or TSV file from the library to use as a data source"
      />
    </div>
  );
}
