/**
 * CSV Upload Form
 * Form for uploading and configuring CSV file discovery
 */

import React from 'react';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Upload, FileText, CheckCircle2, XCircle, Loader2 } from 'lucide-react';
import type { CSVConnectionConfig } from '@/types/discovery';
import { cn } from '@/lib/utils';

interface CSVUploadFormProps {
  config: Partial<CSVConnectionConfig>;
  onChange: (config: Partial<CSVConnectionConfig>) => void;
  onTest?: () => void;
  testStatus?: 'idle' | 'testing' | 'success' | 'error';
  testError?: string;
}

export function CSVUploadForm({
  config,
  onChange,
  onTest,
  testStatus = 'idle',
  testError,
}: CSVUploadFormProps) {
  const [isDragging, setIsDragging] = React.useState(false);
  const fileInputRef = React.useRef<HTMLInputElement>(null);

  const handleChange = (field: keyof CSVConnectionConfig, value: any) => {
    onChange({ ...config, [field]: value });
  };

  const handleFileSelect = (file: File | null) => {
    if (file) {
      handleChange('file', file);
    }
  };

  const handleFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      handleFileSelect(file);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);

    const file = e.dataTransfer.files?.[0];
    if (file && file.name.endsWith('.csv')) {
      handleFileSelect(file);
    } else {
      alert('Please upload a CSV file');
    }
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };

  const handleDragLeave = () => {
    setIsDragging(false);
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const isValid = Boolean(config.file);

  return (
    <div className="space-y-4">
      {/* File Upload Area */}
      <div className="space-y-2">
        <Label>
          CSV File <span className="text-destructive">*</span>
        </Label>

        <div
          className={cn(
            'relative border-2 border-dashed rounded-lg p-8 text-center transition-colors cursor-pointer',
            isDragging
              ? 'border-primary bg-primary/5'
              : 'border-muted-foreground/25 hover:border-primary/50',
            config.file && 'border-green-500 bg-green-50'
          )}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onClick={() => fileInputRef.current?.click()}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".csv"
            onChange={handleFileInputChange}
            className="hidden"
          />

          {config.file ? (
            <div className="space-y-2">
              <FileText className="h-12 w-12 mx-auto text-green-600" />
              <div>
                <p className="font-medium text-green-900">{config.file.name}</p>
                <p className="text-sm text-green-700">
                  {formatFileSize(config.file.size)}
                </p>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={(e) => {
                  e.stopPropagation();
                  handleChange('file', undefined);
                }}
              >
                Remove File
              </Button>
            </div>
          ) : (
            <div className="space-y-2">
              <Upload className="h-12 w-12 mx-auto text-muted-foreground" />
              <div>
                <p className="font-medium">
                  {isDragging ? 'Drop CSV file here' : 'Upload CSV file'}
                </p>
                <p className="text-sm text-muted-foreground">
                  Click to browse or drag and drop
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  Maximum file size: 100 MB
                </p>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* CSV Options */}
      <div className="space-y-4 border-t pt-4">
        <h4 className="font-semibold text-sm">CSV Options</h4>

        {/* Header Row Toggle */}
        <div className="flex items-center justify-between space-x-2">
          <div className="space-y-0.5">
            <Label htmlFor="csv-has-header">First row contains headers</Label>
            <p className="text-xs text-muted-foreground">
              Enable if the first row contains column names
            </p>
          </div>
          <Switch
            id="csv-has-header"
            checked={config.has_header ?? true}
            onCheckedChange={(checked) => handleChange('has_header', checked)}
          />
        </div>

        {/* Delimiter */}
        <div className="space-y-2">
          <Label htmlFor="csv-delimiter">Delimiter</Label>
          <Select
            value={config.delimiter || ','}
            onValueChange={(value) => handleChange('delimiter', value)}
          >
            <SelectTrigger id="csv-delimiter">
              <SelectValue placeholder="Select delimiter" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value=",">Comma (,)</SelectItem>
              <SelectItem value=";">Semicolon (;)</SelectItem>
              <SelectItem value="\t">Tab (\t)</SelectItem>
              <SelectItem value="|">Pipe (|)</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            Character that separates columns
          </p>
        </div>

        {/* Encoding */}
        <div className="space-y-2">
          <Label htmlFor="csv-encoding">File Encoding (Optional)</Label>
          <Select
            value={config.encoding || 'utf-8'}
            onValueChange={(value) => handleChange('encoding', value)}
          >
            <SelectTrigger id="csv-encoding">
              <SelectValue placeholder="Select encoding" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="utf-8">UTF-8</SelectItem>
              <SelectItem value="utf-16">UTF-16</SelectItem>
              <SelectItem value="iso-8859-1">ISO-8859-1 (Latin-1)</SelectItem>
              <SelectItem value="windows-1252">Windows-1252</SelectItem>
            </SelectContent>
          </Select>
          <p className="text-xs text-muted-foreground">
            Leave as UTF-8 if unsure
          </p>
        </div>
      </div>

      {/* File Preview */}
      {config.file && (
        <Alert>
          <AlertDescription className="text-xs space-y-1">
            <div className="flex items-center justify-between">
              <span className="font-medium">Configuration Summary:</span>
            </div>
            <div className="font-mono text-xs">
              <div>• Headers: {config.has_header ? 'Yes' : 'No'}</div>
              <div>
                • Delimiter:{' '}
                {config.delimiter === '\t' ? 'Tab' : config.delimiter || ','}
              </div>
              <div>• Encoding: {config.encoding || 'UTF-8'}</div>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {/* Test/Analyze Button */}
      {onTest && (
        <div className="space-y-2">
          <Button
            type="button"
            variant="outline"
            onClick={onTest}
            disabled={!isValid || testStatus === 'testing'}
            className="w-full"
          >
            {testStatus === 'testing' && (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            )}
            {testStatus === 'success' && (
              <CheckCircle2 className="mr-2 h-4 w-4 text-green-600" />
            )}
            {testStatus === 'error' && (
              <XCircle className="mr-2 h-4 w-4 text-destructive" />
            )}
            Analyze CSV Structure
          </Button>

          {testStatus === 'success' && (
            <Alert className="border-green-200 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-800">
                CSV file analyzed successfully! Ready to discover schema.
              </AlertDescription>
            </Alert>
          )}

          {testStatus === 'error' && testError && (
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertDescription>{testError}</AlertDescription>
            </Alert>
          )}
        </div>
      )}

      {/* Help Text */}
      <Alert>
        <AlertDescription className="text-xs">
          <strong>Tip:</strong> The discovery process will analyze the CSV file
          to detect column types, patterns, and sample data. Ensure the file is
          properly formatted and not corrupted.
        </AlertDescription>
      </Alert>
    </div>
  );
}
