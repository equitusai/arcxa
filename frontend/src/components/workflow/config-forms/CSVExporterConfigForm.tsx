/**
 * CSV Exporter Configuration Form
 * Configure CSV file export settings
 */

import React from 'react';
import { FileDown, AlertCircle } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import type { CSVExporterConfig } from '@/lib/workflow-etl-config';

export interface CSVExporterConfigFormProps {
  config?: CSVExporterConfig;
  onUpdate: (updates: Partial<CSVExporterConfig>) => void;
  nodeId?: string;
}

export function CSVExporterConfigForm({ config, onUpdate }: CSVExporterConfigFormProps) {
  const outputPath = config?.output_path || '';
  const delimiter = config?.delimiter || ',';
  const includeHeader = config?.include_header ?? true;
  const encoding = config?.encoding || 'utf-8';

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border">
        <FileDown className="w-4 h-4 text-primary" />
        <h3 className="text-sm font-semibold text-foreground">CSV Exporter Configuration</h3>
      </div>

      {/* Output Path */}
      <div className="space-y-2">
        <Label htmlFor="output-path" className="text-xs font-medium text-foreground">
          Output File Path <span className="text-red-500">*</span>
        </Label>
        <Input
          id="output-path"
          type="text"
          placeholder="/output/customers_export.csv"
          value={outputPath}
          onChange={(e) => onUpdate({ output_path: e.target.value })}
          className="text-sm font-mono"
        />
        <p className="text-xs text-muted-foreground">
          Path where the CSV file will be saved
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

      {/* Include Header */}
      <div className="flex items-center justify-between py-2">
        <div className="space-y-0.5">
          <Label htmlFor="include-header" className="text-xs font-medium text-foreground">
            Include header row
          </Label>
          <p className="text-xs text-muted-foreground">
            Write column names as first row
          </p>
        </div>
        <Switch
          id="include-header"
          checked={includeHeader}
          onCheckedChange={(checked) => onUpdate({ include_header: checked })}
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

      {/* Validation Messages */}
      {!outputPath && (
        <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
          <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
          <div className="text-amber-800">
            Output file path is required to export CSV data
          </div>
        </div>
      )}

      {/* Configuration Summary */}
      {outputPath && (
        <div className="p-3 bg-background-secondary border border-border rounded text-xs space-y-1">
          <div className="font-medium text-foreground mb-1">Export Summary</div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Delimiter:</span>
            <span className="font-mono text-foreground">
              {delimiter === ',' ? 'Comma' : delimiter === '\t' ? 'Tab' : delimiter === ';' ? 'Semicolon' : 'Pipe'}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Header:</span>
            <span className="font-medium text-foreground">
              {includeHeader ? 'Yes' : 'No'}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground">Encoding:</span>
            <span className="font-mono text-foreground">{encoding}</span>
          </div>
        </div>
      )}
    </div>
  );
}
