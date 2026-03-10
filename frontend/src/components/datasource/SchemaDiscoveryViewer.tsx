/**
 * Schema Discovery Viewer
 * Visualizes discovered database schemas with tables, columns, types, and relationships
 */

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  Database,
  Table as TableIcon,
  Key,
  Link,
  Search,
  ChevronDown,
  ChevronRight,
  Copy,
  AlertCircle,
  FileCode,
  Network,
  BarChart3,
  Eye,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { SchemaInfo } from '@/api/types';
import { toast } from 'sonner';
import { SchemaGraph } from '@/components/DataSources/SchemaGraph';
import { TableDetailsPanel } from '@/components/DataSources/TableDetailsPanel';
import { DataProfilingCharts } from '@/components/DataSources/DataProfilingCharts';
import { ExportOptions } from '@/components/DataSources/ExportOptions';
import type { TableDefinition, ColumnDefinition, ForeignKeyDefinition } from '@/api/types';

interface SchemaDiscoveryViewerProps {
  schema: SchemaInfo | null;
  isLoading?: boolean;
  datasourceName?: string;
  onRefresh?: () => void;
}

interface ViewerColumn {
  name: string;
  type: string;
  nullable: boolean;
  primaryKey?: boolean;
}

interface ViewerForeignKey {
  column: string;
  referenced_table: string;
  referenced_column: string;
  referenced_schema?: string;
}

interface ViewerTable {
  name: string;
  schema?: string;
  columns: ViewerColumn[];
  primary_keys?: string[];
  foreign_keys?: ViewerForeignKey[];
  row_count?: number;
  table_type?: 'TABLE' | 'VIEW';
}

function mapColumn(table: TableDefinition, column: ColumnDefinition): ViewerColumn {
  return {
    name: column.name,
    type: column.data_type,
    nullable: column.nullable,
    primaryKey: column.is_primary_key || table.primary_keys?.includes(column.name),
  };
}

function mapForeignKey(foreignKey: ForeignKeyDefinition): ViewerForeignKey {
  return {
    column: foreignKey.column,
    referenced_table: foreignKey.referenced_table,
    referenced_column: foreignKey.referenced_column,
    referenced_schema: foreignKey.referenced_schema,
  };
}

export function SchemaDiscoveryViewer({
  schema,
  isLoading = false,
  datasourceName = 'Database',
  onRefresh,
}: SchemaDiscoveryViewerProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [expandedTables, setExpandedTables] = useState<Set<string>>(new Set());
  const [selectedView, setSelectedView] = useState<'visual' | 'json' | 'graph' | 'profiling'>('visual');
  const [selectedTableForDetails, setSelectedTableForDetails] = useState<ViewerTable | null>(null);
  const [showTableDetails, setShowTableDetails] = useState(false);

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-5 w-5" />
            Discovering Schema...
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center py-12">
            <div className="text-center">
              <motion.div
                animate={{ rotate: 360 }}
                transition={{ duration: 2, repeat: Infinity, ease: 'linear' }}
                className="inline-block"
              >
                <Database className="h-12 w-12 text-primary" />
              </motion.div>
              <p className="mt-4 text-sm text-muted-foreground">
                Analyzing database structure...
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!schema) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-5 w-5" />
            Schema Discovery
          </CardTitle>
          <CardDescription>
            No schema information available
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="text-center py-8">
            <AlertCircle className="h-12 w-12 mx-auto text-muted-foreground mb-3" />
            <p className="text-sm text-muted-foreground">
              Run schema discovery to view database structure
            </p>
            {onRefresh && (
              <Button onClick={onRefresh} className="mt-4" variant="outline">
                <Database className="h-4 w-4 mr-2" />
                Discover Schema
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    );
  }

  const tables: ViewerTable[] = schema.schemas.flatMap((schemaDefinition) =>
    schemaDefinition.tables.map((table) => ({
      name: table.name,
      schema: table.schema || schemaDefinition.name,
      columns: table.columns.map((column) => mapColumn(table, column)),
      primary_keys:
        table.primary_keys || table.columns.filter((column) => column.is_primary_key).map((column) => column.name),
      foreign_keys: table.foreign_keys?.map(mapForeignKey) || [],
      row_count: table.row_count,
    }))
  );

  const filteredTables = tables.filter((table) =>
    table.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    table.columns.some((column) => column.name.toLowerCase().includes(searchQuery.toLowerCase()))
  );

  const toggleTable = (tableName: string) => {
    const newExpanded = new Set(expandedTables);
    if (newExpanded.has(tableName)) {
      newExpanded.delete(tableName);
    } else {
      newExpanded.add(tableName);
    }
    setExpandedTables(newExpanded);
  };

  const expandAll = () => {
    setExpandedTables(new Set(filteredTables.map((table) => table.name)));
  };

  const collapseAll = () => {
    setExpandedTables(new Set());
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success('Copied to clipboard');
  };

  const getTotalColumns = () => {
    return tables.reduce((sum, table) => sum + table.columns.length, 0);
  };

  const handleViewTableDetails = (table: ViewerTable) => {
    setSelectedTableForDetails(table);
    setShowTableDetails(true);
  };

  const handleTableClickFromGraph = (tableName: string) => {
    const table = tables.find((candidate) => candidate.name === tableName);
    if (table) {
      handleViewTableDetails(table);
    }
  };

  const schemaMetadata = {
    datasource_name: datasourceName,
    schema_name: datasourceName,
    tables,
    discovered_at: new Date().toISOString(),
  };

  const getTypeColor = (type: string) => {
    const upperType = type.toUpperCase();
    if (upperType.includes('INT') || upperType.includes('NUMERIC') || upperType.includes('DECIMAL')) {
      return 'bg-blue-100 text-blue-700 border-blue-200';
    } else if (upperType.includes('VARCHAR') || upperType.includes('TEXT') || upperType.includes('CHAR')) {
      return 'bg-green-100 text-green-700 border-green-200';
    } else if (upperType.includes('DATE') || upperType.includes('TIME')) {
      return 'bg-purple-100 text-purple-700 border-purple-200';
    } else if (upperType.includes('BOOL')) {
      return 'bg-orange-100 text-orange-700 border-orange-200';
    }
    return 'bg-gray-100 text-gray-700 border-gray-200';
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2">
              <Database className="h-5 w-5" />
              Schema: {datasourceName}
            </CardTitle>
            <CardDescription className="mt-1">
              {tables.length} table{tables.length !== 1 ? 's' : ''} • {getTotalColumns()} columns
            </CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <Tabs
              value={selectedView}
              onValueChange={(value) =>
                setSelectedView(value as 'visual' | 'json' | 'graph' | 'profiling')
              }
            >
              <TabsList className="h-9">
                <TabsTrigger value="visual" className="text-xs">
                  <TableIcon className="h-3 w-3 mr-1" />
                  Tables
                </TabsTrigger>
                <TabsTrigger value="graph" className="text-xs">
                  <Network className="h-3 w-3 mr-1" />
                  Graph
                </TabsTrigger>
                <TabsTrigger value="profiling" className="text-xs">
                  <BarChart3 className="h-3 w-3 mr-1" />
                  Analytics
                </TabsTrigger>
                <TabsTrigger value="json" className="text-xs">
                  <FileCode className="h-3 w-3 mr-1" />
                  JSON
                </TabsTrigger>
              </TabsList>
            </Tabs>
            <ExportOptions schema={schemaMetadata} />
            {onRefresh && (
              <Button onClick={onRefresh} size="sm" variant="outline">
                <Database className="h-4 w-4 mr-2" />
                Refresh
              </Button>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent>
        {selectedView === 'graph' ? (
          <>
            {/* Schema Graph Visualization */}
            <div className="mt-4" id="schema-graph-container">
              <SchemaGraph
                tables={tables}
                onTableClick={handleTableClickFromGraph}
              />
            </div>
          </>
        ) : selectedView === 'profiling' ? (
          <>
            {/* Data Profiling Charts */}
            <div className="mt-4">
              <DataProfilingCharts tables={tables} />
            </div>
          </>
        ) : selectedView === 'visual' ? (
          <>
            {/* Search and Actions */}
            <div className="flex items-center gap-2 mb-4">
              <div className="relative flex-1">
                <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                <Input
                  type="search"
                  placeholder="Search tables and columns..."
                  className="pl-9"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                />
              </div>
              <Button size="sm" variant="outline" onClick={expandAll}>
                Expand All
              </Button>
              <Button size="sm" variant="outline" onClick={collapseAll}>
                Collapse All
              </Button>
            </div>

            {/* Tables List */}
            <ScrollArea className="h-[600px] border rounded-md">
              <div className="p-4 space-y-2">
                <AnimatePresence>
                  {filteredTables.length === 0 ? (
                    <div className="text-center py-8 text-muted-foreground">
                      No tables found matching "{searchQuery}"
                    </div>
                  ) : (
                    filteredTables.map((table) => (
                      <TableCard
                        key={table.name}
                        table={table}
                        isExpanded={expandedTables.has(table.name)}
                        onToggle={() => toggleTable(table.name)}
                        onCopy={copyToClipboard}
                        onViewDetails={handleViewTableDetails}
                        getTypeColor={getTypeColor}
                      />
                    ))
                  )}
                </AnimatePresence>
              </div>
            </ScrollArea>
          </>
        ) : (
          /* JSON View */
          <div className="relative">
            <Button
              size="sm"
              variant="outline"
              className="absolute top-2 right-2 z-10"
              onClick={() => copyToClipboard(JSON.stringify(schema, null, 2))}
            >
              <Copy className="h-4 w-4 mr-2" />
              Copy JSON
            </Button>
            <ScrollArea className="h-[700px]">
              <pre className="p-4 bg-muted rounded-md text-xs font-mono overflow-x-auto">
                {JSON.stringify(schema, null, 2)}
              </pre>
            </ScrollArea>
          </div>
        )}
      </CardContent>

      {/* Table Details Panel */}
      <TableDetailsPanel
        table={selectedTableForDetails}
        open={showTableDetails}
        onOpenChange={setShowTableDetails}
      />
    </Card>
  );
}

interface TableCardProps {
  table: ViewerTable;
  isExpanded: boolean;
  onToggle: () => void;
  onCopy: (text: string) => void;
  onViewDetails: (table: ViewerTable) => void;
  getTypeColor: (type: string) => string;
}

function TableCard({ table, isExpanded, onToggle, onCopy, onViewDetails, getTypeColor }: TableCardProps) {
  const columns = table.columns || [];
  const primaryKeys = table.primary_keys || [];
  const foreignKeys = table.foreign_keys || [];

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.15 }}
    >
      <Collapsible open={isExpanded} onOpenChange={onToggle}>
        <Card className="overflow-hidden">
          <CollapsibleTrigger asChild>
            <CardHeader className="p-4 cursor-pointer hover:bg-muted/50 transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  {isExpanded ? (
                    <ChevronDown className="h-4 w-4 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-muted-foreground" />
                  )}
                  <TableIcon className="h-5 w-5 text-primary" />
                  <div>
                    <h4 className="font-semibold">{table.name}</h4>
                    <p className="text-xs text-muted-foreground">
                      {columns.length} column{columns.length !== 1 ? 's' : ''}
                      {primaryKeys.length > 0 && ` • ${primaryKeys.length} PK`}
                      {foreignKeys.length > 0 && ` • ${foreignKeys.length} FK`}
                    </p>
                  </div>
                </div>
                <div className="flex gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={(e) => {
                      e.stopPropagation();
                      onViewDetails(table);
                    }}
                  >
                    <Eye className="h-3 w-3" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={(e) => {
                      e.stopPropagation();
                      onCopy(table.name);
                    }}
                  >
                    <Copy className="h-3 w-3" />
                  </Button>
                </div>
              </div>
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="p-0">
              <div className="border-t">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-[40%]">Column</TableHead>
                      <TableHead className="w-[25%]">Type</TableHead>
                      <TableHead className="w-[15%]">Nullable</TableHead>
                      <TableHead className="w-[20%]">Keys</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {columns.map((col, idx) => {
                      const isPrimaryKey = primaryKeys.includes(col.name);
                      const foreignKey = foreignKeys.find((fk) => fk.column === col.name);

                      return (
                        <TableRow key={idx}>
                          <TableCell className="font-medium font-mono text-sm">
                            {col.name}
                          </TableCell>
                          <TableCell>
                            <Badge variant="outline" className={cn('text-xs', getTypeColor(col.type))}>
                              {col.type}
                            </Badge>
                          </TableCell>
                          <TableCell>
                            {col.nullable ? (
                              <span className="text-xs text-muted-foreground">Yes</span>
                            ) : (
                              <Badge variant="secondary" className="text-xs">
                                NOT NULL
                              </Badge>
                            )}
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center gap-1">
                              {isPrimaryKey && (
                                <Badge variant="default" className="text-xs">
                                  <Key className="h-3 w-3 mr-1" />
                                  PK
                                </Badge>
                              )}
                              {foreignKey && (
                                <Badge variant="outline" className="text-xs">
                                  <Link className="h-3 w-3 mr-1" />
                                  FK
                                </Badge>
                              )}
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </div>

              {/* Foreign Keys Section */}
              {foreignKeys.length > 0 && (
                <div className="border-t bg-muted/20 p-4">
                  <h5 className="text-sm font-semibold mb-2 flex items-center gap-2">
                    <Link className="h-4 w-4" />
                    Foreign Keys
                  </h5>
                  <div className="space-y-1">
                    {foreignKeys.map((fk, idx) => (
                      <div key={idx} className="text-xs font-mono bg-background p-2 rounded border">
                        <span className="text-primary">{fk.column}</span>
                        {' → '}
                        <span className="text-green-600">{fk.referenced_table}</span>
                        {'.'}
                        <span className="text-blue-600">{fk.referenced_column}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>
    </motion.div>
  );
}
