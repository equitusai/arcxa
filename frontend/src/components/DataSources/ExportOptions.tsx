/**
 * ExportOptions.tsx - Export Schema in Multiple Formats
 *
 * Features:
 * - Export button with dropdown menu
 * - Formats: JSON, CSV, DDL (SQL), ERD (PNG/SVG)
 * - JSON Export: Full metadata download
 * - CSV Export: Table list with stats
 * - DDL Export: Call backend DDL generation endpoint
 * - ERD Export: Screenshot of ReactFlow canvas (html2canvas)
 */

import React, { useState } from 'react';
import { saveAs } from 'file-saver';
import html2canvas from 'html2canvas';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import { Card, CardContent } from '@/components/ui/card';
import {
  Download,
  FileJson,
  FileSpreadsheet,
  FileCode,
  Image,
  CheckCircle,
  Loader2,
  ChevronDown,
} from 'lucide-react';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';

interface TableMetadata {
  name: string;
  schema?: string;
  columns: Array<{
    name: string;
    type: string;
    nullable: boolean;
    primaryKey?: boolean;
    defaultValue?: string;
  }>;
  primary_keys?: string[];
  foreign_keys?: Array<{
    column: string;
    referenced_table: string;
    referenced_column: string;
  }>;
  row_count?: number;
  table_type?: 'TABLE' | 'VIEW';
}

interface SchemaMetadata {
  datasource_name: string;
  schema_name?: string;
  tables: TableMetadata[];
  discovered_at?: string;
}

interface ExportOptionsProps {
  schema: SchemaMetadata;
  graphElementId?: string;
  onDDLGenerate?: (dialect: string, options: DDLOptions) => Promise<string>;
  className?: string;
}

interface DDLOptions {
  include_primary_keys: boolean;
  include_foreign_keys: boolean;
  include_indexes: boolean;
  include_comments: boolean;
}

export function ExportOptions({
  schema,
  graphElementId,
  onDDLGenerate,
  className,
}: ExportOptionsProps) {
  const [showDDLDialog, setShowDDLDialog] = useState(false);
  const [showERDDialog, setShowERDDialog] = useState(false);
  const [ddlDialect, setDDLDialect] = useState<'postgresql' | 'db2' | 'oracle' | 'mysql'>('postgresql');
  const [ddlOptions, setDDLOptions] = useState<DDLOptions>({
    include_primary_keys: true,
    include_foreign_keys: true,
    include_indexes: true,
    include_comments: true,
  });
  const [erdFormat, setERDFormat] = useState<'png' | 'svg'>('png');
  const [isExporting, setIsExporting] = useState(false);

  // Export as JSON
  const exportJSON = () => {
    try {
      const jsonData = JSON.stringify(schema, null, 2);
      const blob = new Blob([jsonData], { type: 'application/json' });
      saveAs(blob, `${schema.datasource_name}_schema.json`);
      toast.success('Schema exported as JSON');
    } catch (error) {
      console.error('JSON export error:', error);
      toast.error('Failed to export JSON');
    }
  };

  // Export as CSV
  const exportCSV = () => {
    try {
      // CSV header
      const headers = [
        'Table Name',
        'Schema',
        'Type',
        'Columns',
        'Primary Keys',
        'Foreign Keys',
        'Row Count',
      ];
      const csvRows = [headers.join(',')];

      // CSV rows
      schema.tables.forEach((table) => {
        const row = [
          `"${table.name}"`,
          `"${table.schema || ''}"`,
          `"${table.table_type || 'TABLE'}"`,
          table.columns.length,
          (table.primary_keys?.length || 0),
          (table.foreign_keys?.length || 0),
          (table.row_count || 0),
        ];
        csvRows.push(row.join(','));
      });

      const csvContent = csvRows.join('\n');
      const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
      saveAs(blob, `${schema.datasource_name}_tables.csv`);
      toast.success('Table list exported as CSV');
    } catch (error) {
      console.error('CSV export error:', error);
      toast.error('Failed to export CSV');
    }
  };

  // Export as DDL
  const exportDDL = async () => {
    setIsExporting(true);
    try {
      let ddlContent: string;

      if (onDDLGenerate) {
        // Call backend to generate DDL
        ddlContent = await onDDLGenerate(ddlDialect, ddlOptions);
      } else {
        // Generate DDL locally (simplified)
        ddlContent = generateDDLLocally(schema, ddlDialect, ddlOptions);
      }

      const blob = new Blob([ddlContent], { type: 'text/plain;charset=utf-8;' });
      saveAs(blob, `${schema.datasource_name}_schema_${ddlDialect}.sql`);
      toast.success(`DDL exported for ${ddlDialect.toUpperCase()}`);
      setShowDDLDialog(false);
    } catch (error) {
      console.error('DDL export error:', error);
      toast.error('Failed to generate DDL');
    } finally {
      setIsExporting(false);
    }
  };

  // Export ERD as image
  const exportERD = async () => {
    if (!graphElementId) {
      toast.error('Graph element not found');
      return;
    }

    setIsExporting(true);
    try {
      const graphElement = document.getElementById(graphElementId);
      if (!graphElement) {
        throw new Error('Graph element not found in DOM');
      }

      if (erdFormat === 'png') {
        const canvas = await html2canvas(graphElement, {
          backgroundColor: '#ffffff',
          scale: 2,
        });
        canvas.toBlob((blob) => {
          if (blob) {
            saveAs(blob, `${schema.datasource_name}_erd.png`);
            toast.success('ERD exported as PNG');
          }
        });
      } else {
        // SVG export would require additional logic
        toast.info('SVG export coming soon');
      }

      setShowERDDialog(false);
    } catch (error) {
      console.error('ERD export error:', error);
      toast.error('Failed to export ERD');
    } finally {
      setIsExporting(false);
    }
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" className={cn('gap-2', className)}>
            <Download className="h-4 w-4" />
            Export
            <ChevronDown className="h-3 w-3 opacity-50" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <DropdownMenuLabel>Export Format</DropdownMenuLabel>
          <DropdownMenuSeparator />

          <DropdownMenuItem onClick={exportJSON}>
            <FileJson className="h-4 w-4 mr-2 text-blue-600" />
            <span>JSON (Full Metadata)</span>
          </DropdownMenuItem>

          <DropdownMenuItem onClick={exportCSV}>
            <FileSpreadsheet className="h-4 w-4 mr-2 text-green-600" />
            <span>CSV (Table List)</span>
          </DropdownMenuItem>

          <DropdownMenuItem onClick={() => setShowDDLDialog(true)}>
            <FileCode className="h-4 w-4 mr-2 text-purple-600" />
            <span>DDL (SQL Scripts)</span>
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={() => setShowERDDialog(true)}
            disabled={!graphElementId}
          >
            <Image className="h-4 w-4 mr-2 text-orange-600" />
            <span>ERD (Image)</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* DDL Export Dialog */}
      <Dialog open={showDDLDialog} onOpenChange={setShowDDLDialog}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <FileCode className="h-5 w-5 text-purple-600" />
              Export DDL (SQL)
            </DialogTitle>
            <DialogDescription>
              Generate CREATE TABLE statements for your target database
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label>Database Dialect</Label>
              <Select value={ddlDialect} onValueChange={(v: any) => setDDLDialect(v)}>
                <SelectTrigger>
                  <SelectValue placeholder="Select database dialect" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="postgresql">PostgreSQL</SelectItem>
                  <SelectItem value="db2">IBM DB2</SelectItem>
                  <SelectItem value="oracle">Oracle Database</SelectItem>
                  <SelectItem value="mysql">MySQL / MariaDB</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <Label>Include Options</Label>
              <div className="space-y-2">
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="pk"
                    checked={ddlOptions.include_primary_keys}
                    onCheckedChange={(checked) =>
                      setDDLOptions({ ...ddlOptions, include_primary_keys: !!checked })
                    }
                  />
                  <Label htmlFor="pk" className="font-normal cursor-pointer">
                    Primary Keys
                  </Label>
                </div>
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="fk"
                    checked={ddlOptions.include_foreign_keys}
                    onCheckedChange={(checked) =>
                      setDDLOptions({ ...ddlOptions, include_foreign_keys: !!checked })
                    }
                  />
                  <Label htmlFor="fk" className="font-normal cursor-pointer">
                    Foreign Keys
                  </Label>
                </div>
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="indexes"
                    checked={ddlOptions.include_indexes}
                    onCheckedChange={(checked) =>
                      setDDLOptions({ ...ddlOptions, include_indexes: !!checked })
                    }
                  />
                  <Label htmlFor="indexes" className="font-normal cursor-pointer">
                    Indexes
                  </Label>
                </div>
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="comments"
                    checked={ddlOptions.include_comments}
                    onCheckedChange={(checked) =>
                      setDDLOptions({ ...ddlOptions, include_comments: !!checked })
                    }
                  />
                  <Label htmlFor="comments" className="font-normal cursor-pointer">
                    Column Comments
                  </Label>
                </div>
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDDLDialog(false)}>
              Cancel
            </Button>
            <Button onClick={exportDDL} disabled={isExporting}>
              {isExporting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Generating...
                </>
              ) : (
                <>
                  <Download className="h-4 w-4 mr-2" />
                  Generate DDL
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* ERD Export Dialog */}
      <Dialog open={showERDDialog} onOpenChange={setShowERDDialog}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Image className="h-5 w-5 text-orange-600" />
              Export ERD Diagram
            </DialogTitle>
            <DialogDescription>
              Export the schema visualization as an image
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label>Image Format</Label>
              <Select value={erdFormat} onValueChange={(v: any) => setERDFormat(v)}>
                <SelectTrigger>
                  <SelectValue placeholder="Select image format" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="png">PNG (High Quality)</SelectItem>
                  <SelectItem value="svg" disabled>SVG (Vector - Coming Soon)</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <Card className="bg-muted/50">
              <CardContent className="p-4 text-sm text-muted-foreground">
                The diagram will be exported at 2x resolution for better quality.
                Make sure the graph is positioned as desired before exporting.
              </CardContent>
            </Card>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setShowERDDialog(false)}>
              Cancel
            </Button>
            <Button onClick={exportERD} disabled={isExporting || !graphElementId}>
              {isExporting ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Exporting...
                </>
              ) : (
                <>
                  <Download className="h-4 w-4 mr-2" />
                  Export Image
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

// Generate DDL locally (simplified version)
function generateDDLLocally(
  schema: SchemaMetadata,
  dialect: string,
  options: DDLOptions
): string {
  const lines: string[] = [];

  // Header
  lines.push(`-- Generated DDL for ${schema.datasource_name}`);
  lines.push(`-- Target Dialect: ${dialect.toUpperCase()}`);
  lines.push(`-- Generated: ${new Date().toISOString()}`);
  lines.push('');

  // Generate CREATE TABLE statements
  schema.tables.forEach((table) => {
    lines.push(`-- Table: ${table.name}`);
    lines.push(`CREATE TABLE ${table.schema ? `${table.schema}.` : ''}${table.name} (`);

    // Columns
    const columnDefs = table.columns.map((col) => {
      let def = `    ${col.name} ${mapDataType(col.type, dialect)}`;
      if (!col.nullable) {
        def += ' NOT NULL';
      }
      if (col.defaultValue) {
        def += ` DEFAULT ${col.defaultValue}`;
      }
      return def;
    });

    lines.push(columnDefs.join(',\n'));

    // Primary keys
    if (options.include_primary_keys && table.primary_keys && table.primary_keys.length > 0) {
      lines.push(`,    PRIMARY KEY (${table.primary_keys.join(', ')})`);
    }

    lines.push(');');
    lines.push('');

    // Foreign keys
    if (options.include_foreign_keys && table.foreign_keys && table.foreign_keys.length > 0) {
      table.foreign_keys.forEach((fk) => {
        lines.push(
          `ALTER TABLE ${table.name} ADD CONSTRAINT fk_${table.name}_${fk.column} ` +
          `FOREIGN KEY (${fk.column}) REFERENCES ${fk.referenced_table}(${fk.referenced_column});`
        );
      });
      lines.push('');
    }

    // Comments
    if (options.include_comments && dialect === 'postgresql') {
      lines.push(`COMMENT ON TABLE ${table.name} IS 'Table with ${table.columns.length} columns';`);
      lines.push('');
    }
  });

  return lines.join('\n');
}

// Map data types to target dialect
function mapDataType(sourceType: string, dialect: string): string {
  const upperType = sourceType.toUpperCase();

  // Basic mapping (would be more comprehensive in production)
  if (upperType.includes('VARCHAR')) {
    return sourceType;
  } else if (upperType.includes('INT')) {
    return dialect === 'oracle' ? 'NUMBER(10)' : sourceType;
  } else if (upperType.includes('TEXT')) {
    return dialect === 'oracle' ? 'CLOB' : sourceType;
  } else if (upperType.includes('TIMESTAMP')) {
    return dialect === 'oracle' ? 'TIMESTAMP' : sourceType;
  }

  return sourceType;
}
