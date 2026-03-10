/**
 * TableDetailsPanel.tsx - Comprehensive Table Details Drawer
 *
 * Features:
 * - Ant Design Drawer (slides in from right)
 * - Tabs: Overview, Columns, Relationships, Sample Data, Recommendations
 * - Overview: Row count, table type, created date
 * - Columns: Table with column details, PK/FK badges
 * - Relationships: Foreign key list (incoming/outgoing)
 * - Sample Data: Preview first 10 rows
 * - Recommendations: Suggested indexes, missing PKs, data quality issues
 */

import React, { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import {
  Table as TableIcon,
  Key,
  Link,
  AlertCircle,
  CheckCircle,
  Info,
  ArrowRight,
  ArrowLeft,
  Lightbulb,
  Database,
  Calendar,
  Hash,
  FileText,
} from 'lucide-react';
import { cn } from '@/lib/utils';

interface Column {
  name: string;
  type: string;
  nullable: boolean;
  primaryKey?: boolean;
  defaultValue?: string;
}

interface ForeignKey {
  column: string;
  referenced_table: string;
  referenced_column: string;
}

interface TableDetails {
  name: string;
  schema?: string;
  columns: Column[];
  primary_keys?: string[];
  foreign_keys?: ForeignKey[];
  row_count?: number;
  table_type?: 'TABLE' | 'VIEW';
  created_at?: string;
  sample_data?: Record<string, any>[];
  recommendations?: Recommendation[];
}

interface Recommendation {
  type: 'index' | 'primary_key' | 'data_quality' | 'performance';
  severity: 'info' | 'warning' | 'error';
  title: string;
  description: string;
  action?: string;
}

interface TableDetailsPanelProps {
  table: TableDetails | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onApplyRecommendation?: (recommendation: Recommendation) => void;
}

export function TableDetailsPanel({
  table,
  open,
  onOpenChange,
  onApplyRecommendation,
}: TableDetailsPanelProps) {
  const [activeTab, setActiveTab] = useState('overview');

  // Reset tab when table changes
  useEffect(() => {
    if (open) {
      setActiveTab('overview');
    }
  }, [table, open]);

  if (!table) {
    return null;
  }

  const columns = table.columns || [];
  const primaryKeys = table.primary_keys || [];
  const foreignKeys = table.foreign_keys || [];
  const sampleData = table.sample_data || [];
  const recommendations = table.recommendations || generateRecommendations(table);

  // Find incoming foreign keys (reverse relationships)
  const incomingForeignKeys: Array<{ from_table: string; column: string; to_column: string }> = [];

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

  const getSeverityIcon = (severity: string) => {
    switch (severity) {
      case 'error':
        return <AlertCircle className="h-4 w-4 text-destructive" />;
      case 'warning':
        return <AlertCircle className="h-4 w-4 text-orange-500" />;
      case 'info':
        return <Info className="h-4 w-4 text-blue-500" />;
      default:
        return <Info className="h-4 w-4 text-muted-foreground" />;
    }
  };

  const getSeverityColor = (severity: string) => {
    switch (severity) {
      case 'error':
        return 'destructive';
      case 'warning':
        return 'default';
      case 'info':
        return 'secondary';
      default:
        return 'outline';
    }
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-full sm:max-w-2xl overflow-y-auto">
        <SheetHeader>
          <SheetTitle className="flex items-center gap-2">
            <TableIcon className="h-5 w-5 text-primary" />
            {table.name}
          </SheetTitle>
          <SheetDescription>
            {table.schema && `Schema: ${table.schema} • `}
            {columns.length} column{columns.length !== 1 ? 's' : ''}
            {table.row_count !== undefined && ` • ${table.row_count.toLocaleString()} rows`}
          </SheetDescription>
        </SheetHeader>

        <div className="mt-6">
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <TabsList className="grid w-full grid-cols-5">
              <TabsTrigger value="overview">Overview</TabsTrigger>
              <TabsTrigger value="columns">Columns</TabsTrigger>
              <TabsTrigger value="relationships">Relations</TabsTrigger>
              <TabsTrigger value="sample">Sample</TabsTrigger>
              <TabsTrigger value="recommendations">
                Tips
                {recommendations.length > 0 && (
                  <Badge variant="secondary" className="ml-1 px-1 text-xs">
                    {recommendations.length}
                  </Badge>
                )}
              </TabsTrigger>
            </TabsList>

            {/* Overview Tab */}
            <TabsContent value="overview" className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle className="text-base">Table Information</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-1">
                      <div className="text-sm text-muted-foreground">Table Name</div>
                      <div className="font-mono font-semibold">{table.name}</div>
                    </div>
                    {table.schema && (
                      <div className="space-y-1">
                        <div className="text-sm text-muted-foreground">Schema</div>
                        <div className="font-mono">{table.schema}</div>
                      </div>
                    )}
                    <div className="space-y-1">
                      <div className="text-sm text-muted-foreground flex items-center gap-1">
                        <Database className="h-3 w-3" />
                        Table Type
                      </div>
                      <Badge variant="outline">{table.table_type || 'TABLE'}</Badge>
                    </div>
                    {table.row_count !== undefined && (
                      <div className="space-y-1">
                        <div className="text-sm text-muted-foreground flex items-center gap-1">
                          <Hash className="h-3 w-3" />
                          Row Count
                        </div>
                        <div className="font-semibold">{table.row_count.toLocaleString()}</div>
                      </div>
                    )}
                    {table.created_at && (
                      <div className="space-y-1">
                        <div className="text-sm text-muted-foreground flex items-center gap-1">
                          <Calendar className="h-3 w-3" />
                          Created
                        </div>
                        <div className="text-sm">{new Date(table.created_at).toLocaleDateString()}</div>
                      </div>
                    )}
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle className="text-base">Schema Summary</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="grid grid-cols-3 gap-4 text-center">
                    <div>
                      <div className="text-2xl font-bold text-primary">{columns.length}</div>
                      <div className="text-xs text-muted-foreground">Columns</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-blue-600">{primaryKeys.length}</div>
                      <div className="text-xs text-muted-foreground">Primary Keys</div>
                    </div>
                    <div>
                      <div className="text-2xl font-bold text-green-600">{foreignKeys.length}</div>
                      <div className="text-xs text-muted-foreground">Foreign Keys</div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            </TabsContent>

            {/* Columns Tab */}
            <TabsContent value="columns">
              <Card>
                <CardContent className="p-0">
                  <ScrollArea className="h-[600px]">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead className="w-[40%]">Column Name</TableHead>
                          <TableHead className="w-[25%]">Data Type</TableHead>
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
                              <TableCell className="font-mono font-medium">{col.name}</TableCell>
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
                  </ScrollArea>
                </CardContent>
              </Card>
            </TabsContent>

            {/* Relationships Tab */}
            <TabsContent value="relationships" className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <ArrowRight className="h-4 w-4" />
                    Outgoing Foreign Keys
                  </CardTitle>
                  <CardDescription>
                    Columns in this table that reference other tables
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  {foreignKeys.length === 0 ? (
                    <div className="text-center py-6 text-sm text-muted-foreground">
                      No outgoing foreign keys
                    </div>
                  ) : (
                    <div className="space-y-2">
                      {foreignKeys.map((fk, idx) => (
                        <div key={idx} className="flex items-center gap-2 p-3 border rounded-lg bg-card">
                          <Link className="h-4 w-4 text-green-600 flex-shrink-0" />
                          <div className="flex-1 font-mono text-sm">
                            <span className="text-primary font-semibold">{fk.column}</span>
                            <ArrowRight className="inline h-3 w-3 mx-2" />
                            <span className="text-green-600">{fk.referenced_table}</span>
                            <span className="text-muted-foreground">.</span>
                            <span className="text-blue-600">{fk.referenced_column}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <ArrowLeft className="h-4 w-4" />
                    Incoming Foreign Keys
                  </CardTitle>
                  <CardDescription>
                    Other tables that reference this table
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  {incomingForeignKeys.length === 0 ? (
                    <div className="text-center py-6 text-sm text-muted-foreground">
                      No incoming foreign keys detected
                    </div>
                  ) : (
                    <div className="space-y-2">
                      {incomingForeignKeys.map((fk, idx) => (
                        <div key={idx} className="flex items-center gap-2 p-3 border rounded-lg bg-card">
                          <Link className="h-4 w-4 text-blue-600 flex-shrink-0" />
                          <div className="flex-1 font-mono text-sm">
                            <span className="text-green-600">{fk.from_table}</span>
                            <span className="text-muted-foreground">.</span>
                            <span className="text-primary">{fk.column}</span>
                            <ArrowRight className="inline h-3 w-3 mx-2" />
                            <span className="text-blue-600">{fk.to_column}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </CardContent>
              </Card>
            </TabsContent>

            {/* Sample Data Tab */}
            <TabsContent value="sample">
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <FileText className="h-4 w-4" />
                    Sample Data Preview
                  </CardTitle>
                  <CardDescription>First {sampleData.length} rows</CardDescription>
                </CardHeader>
                <CardContent className="p-0">
                  {sampleData.length === 0 ? (
                    <div className="text-center py-8 text-sm text-muted-foreground">
                      No sample data available
                    </div>
                  ) : (
                    <ScrollArea className="h-[600px]">
                      <div className="overflow-x-auto">
                        <Table>
                          <TableHeader>
                            <TableRow>
                              {columns.slice(0, 10).map((col, idx) => (
                                <TableHead key={idx} className="font-mono text-xs">
                                  {col.name}
                                </TableHead>
                              ))}
                            </TableRow>
                          </TableHeader>
                          <TableBody>
                            {sampleData.slice(0, 10).map((row, rowIdx) => (
                              <TableRow key={rowIdx}>
                                {columns.slice(0, 10).map((col, colIdx) => (
                                  <TableCell key={colIdx} className="font-mono text-xs">
                                    {row[col.name]?.toString() || <span className="text-muted-foreground">NULL</span>}
                                  </TableCell>
                                ))}
                              </TableRow>
                            ))}
                          </TableBody>
                        </Table>
                      </div>
                    </ScrollArea>
                  )}
                </CardContent>
              </Card>
            </TabsContent>

            {/* Recommendations Tab */}
            <TabsContent value="recommendations" className="space-y-3">
              {recommendations.length === 0 ? (
                <Card>
                  <CardContent className="py-8 text-center">
                    <CheckCircle className="h-12 w-12 mx-auto text-green-500 mb-3" />
                    <p className="text-sm text-muted-foreground">
                      No recommendations. Table looks good!
                    </p>
                  </CardContent>
                </Card>
              ) : (
                recommendations.map((rec, idx) => (
                  <motion.div
                    key={idx}
                    initial={{ opacity: 0, y: 10 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: idx * 0.05 }}
                  >
                    <Card>
                      <CardHeader className="pb-3">
                        <div className="flex items-start gap-3">
                          {getSeverityIcon(rec.severity)}
                          <div className="flex-1">
                            <CardTitle className="text-sm">{rec.title}</CardTitle>
                            <Badge variant={getSeverityColor(rec.severity)} className="text-xs mt-1">
                              {rec.type.replace('_', ' ').toUpperCase()}
                            </Badge>
                          </div>
                        </div>
                      </CardHeader>
                      <CardContent>
                        <p className="text-sm text-muted-foreground mb-3">{rec.description}</p>
                        {rec.action && onApplyRecommendation && (
                          <Button
                            size="sm"
                            variant="outline"
                            onClick={() => onApplyRecommendation(rec)}
                          >
                            <Lightbulb className="h-3 w-3 mr-1" />
                            {rec.action}
                          </Button>
                        )}
                      </CardContent>
                    </Card>
                  </motion.div>
                ))
              )}
            </TabsContent>
          </Tabs>
        </div>
      </SheetContent>
    </Sheet>
  );
}

// Generate smart recommendations based on table metadata
function generateRecommendations(table: TableDetails): Recommendation[] {
  const recommendations: Recommendation[] = [];

  // Check for missing primary key
  if (!table.primary_keys || table.primary_keys.length === 0) {
    recommendations.push({
      type: 'primary_key',
      severity: 'warning',
      title: 'Missing Primary Key',
      description: 'This table does not have a primary key defined. Consider adding one for data integrity and performance.',
      action: 'Add Primary Key',
    });
  }

  // Check for columns with high null percentage (would require sample data analysis)
  const nullableColumns = table.columns.filter((col) => col.nullable && !col.name.toLowerCase().includes('optional'));
  if (nullableColumns.length > table.columns.length * 0.7) {
    recommendations.push({
      type: 'data_quality',
      severity: 'info',
      title: 'High Nullable Column Count',
      description: `${nullableColumns.length} out of ${table.columns.length} columns are nullable. Review if this is intentional.`,
    });
  }

  // Suggest indexes on foreign key columns
  if (table.foreign_keys && table.foreign_keys.length > 0) {
    recommendations.push({
      type: 'index',
      severity: 'info',
      title: 'Index Foreign Key Columns',
      description: 'Consider adding indexes on foreign key columns for better join performance.',
      action: 'View Suggested Indexes',
    });
  }

  // Performance recommendation for large tables
  if (table.row_count && table.row_count > 1000000) {
    recommendations.push({
      type: 'performance',
      severity: 'warning',
      title: 'Large Table Detected',
      description: `This table has ${table.row_count.toLocaleString()} rows. Consider partitioning or archiving strategies.`,
      action: 'Learn More',
    });
  }

  return recommendations;
}
