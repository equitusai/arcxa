/**
 * DataProfilingCharts.tsx - Data Profiling Visualizations
 *
 * Features:
 * - Bar chart: Column data types distribution
 * - Pie chart: Nullability breakdown
 * - Histogram: Cardinality distribution
 * - Top values chart (for categorical columns)
 */

import React, { useMemo } from 'react';
import {
  BarChart,
  Bar,
  PieChart,
  Pie,
  Cell,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  LineChart,
  Line,
} from 'recharts';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Badge } from '@/components/ui/badge';
import {
  BarChart3,
  PieChart as PieChartIcon,
  Activity,
  TrendingUp,
  Database,
} from 'lucide-react';

interface Column {
  name: string;
  type: string;
  nullable: boolean;
  cardinality?: number; // Distinct value count
  null_percentage?: number;
  top_values?: Array<{ value: string; count: number }>;
}

interface TableMetadata {
  name: string;
  columns: Column[];
  row_count?: number;
}

interface DataProfilingChartsProps {
  tables: TableMetadata[];
  selectedTable?: TableMetadata;
  className?: string;
}

const COLORS = [
  '#3b82f6', // blue
  '#10b981', // green
  '#f59e0b', // amber
  '#ef4444', // red
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#06b6d4', // cyan
  '#84cc16', // lime
];

export function DataProfilingCharts({
  tables,
  selectedTable,
  className,
}: DataProfilingChartsProps) {
  // Calculate data type distribution
  const dataTypeDistribution = useMemo(() => {
    const allColumns = selectedTable
      ? selectedTable.columns
      : tables.flatMap((t) => t.columns);

    const typeMap = new Map<string, number>();

    allColumns.forEach((col) => {
      const normalizedType = normalizeDataType(col.type);
      typeMap.set(normalizedType, (typeMap.get(normalizedType) || 0) + 1);
    });

    return Array.from(typeMap.entries())
      .map(([type, count]) => ({ type, count }))
      .sort((a, b) => b.count - a.count);
  }, [tables, selectedTable]);

  // Calculate nullability distribution
  const nullabilityDistribution = useMemo(() => {
    const allColumns = selectedTable
      ? selectedTable.columns
      : tables.flatMap((t) => t.columns);

    const nullable = allColumns.filter((col) => col.nullable).length;
    const notNullable = allColumns.length - nullable;

    return [
      { name: 'NOT NULL', value: notNullable, color: '#10b981' },
      { name: 'NULLABLE', value: nullable, color: '#f59e0b' },
    ];
  }, [tables, selectedTable]);

  // Calculate cardinality distribution (if data available)
  const cardinalityDistribution = useMemo(() => {
    const columns = selectedTable
      ? selectedTable.columns
      : tables.flatMap((t) => t.columns);

    const withCardinality = columns.filter((col) => col.cardinality !== undefined);

    if (withCardinality.length === 0) {
      return [];
    }

    // Group by cardinality ranges
    const ranges = [
      { label: '1-10', min: 1, max: 10, count: 0 },
      { label: '11-100', min: 11, max: 100, count: 0 },
      { label: '101-1K', min: 101, max: 1000, count: 0 },
      { label: '1K-10K', min: 1001, max: 10000, count: 0 },
      { label: '10K+', min: 10001, max: Infinity, count: 0 },
    ];

    withCardinality.forEach((col) => {
      const card = col.cardinality!;
      const range = ranges.find((r) => card >= r.min && card <= r.max);
      if (range) {
        range.count++;
      }
    });

    return ranges.filter((r) => r.count > 0);
  }, [tables, selectedTable]);

  // Top values for a specific column (if available)
  const topValuesData = useMemo(() => {
    if (!selectedTable) return null;

    // Find first column with top_values data
    const columnWithTopValues = selectedTable.columns.find(
      (col) => col.top_values && col.top_values.length > 0
    );

    return columnWithTopValues
      ? {
          columnName: columnWithTopValues.name,
          data: columnWithTopValues.top_values!.slice(0, 10),
        }
      : null;
  }, [selectedTable]);

  // Table-level statistics
  const tableStats = useMemo(() => {
    if (!selectedTable) return null;

    const totalColumns = selectedTable.columns.length;
    const nullableColumns = selectedTable.columns.filter((col) => col.nullable).length;
    const primaryKeyColumns = selectedTable.columns.filter((col) => col.type.includes('PK')).length;

    return {
      totalColumns,
      nullableColumns,
      primaryKeyColumns,
      nullablePercentage: ((nullableColumns / totalColumns) * 100).toFixed(1),
    };
  }, [selectedTable]);

  const CustomTooltip = ({ active, payload, label }: any) => {
    if (active && payload && payload.length) {
      return (
        <div className="bg-popover border rounded-lg shadow-lg p-3">
          <p className="font-semibold text-sm mb-1">{label}</p>
          {payload.map((entry: any, index: number) => (
            <p key={index} className="text-xs" style={{ color: entry.color }}>
              {entry.name}: <span className="font-semibold">{entry.value}</span>
            </p>
          ))}
        </div>
      );
    }
    return null;
  };

  return (
    <div className={className}>
      <Tabs defaultValue="types" className="w-full">
        <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="types">
            <BarChart3 className="h-4 w-4 mr-2" />
            Data Types
          </TabsTrigger>
          <TabsTrigger value="nullability">
            <PieChartIcon className="h-4 w-4 mr-2" />
            Nullability
          </TabsTrigger>
          <TabsTrigger value="cardinality" disabled={cardinalityDistribution.length === 0}>
            <Activity className="h-4 w-4 mr-2" />
            Cardinality
          </TabsTrigger>
          <TabsTrigger value="topvalues" disabled={!topValuesData}>
            <TrendingUp className="h-4 w-4 mr-2" />
            Top Values
          </TabsTrigger>
        </TabsList>

        {/* Data Types Distribution */}
        <TabsContent value="types">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <BarChart3 className="h-5 w-5 text-blue-600" />
                Data Type Distribution
              </CardTitle>
              <CardDescription>
                Distribution of column data types
                {selectedTable && ` in ${selectedTable.name}`}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {dataTypeDistribution.length === 0 ? (
                <div className="text-center py-12 text-muted-foreground">
                  No data available
                </div>
              ) : (
                <>
                  <ResponsiveContainer width="100%" height={300}>
                    <BarChart data={dataTypeDistribution}>
                      <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />
                      <XAxis
                        dataKey="type"
                        tick={{ fontSize: 12 }}
                        angle={-45}
                        textAnchor="end"
                        height={80}
                      />
                      <YAxis tick={{ fontSize: 12 }} />
                      <Tooltip content={<CustomTooltip />} />
                      <Legend />
                      <Bar dataKey="count" name="Column Count" fill="#3b82f6" radius={[4, 4, 0, 0]} />
                    </BarChart>
                  </ResponsiveContainer>

                  <div className="mt-4 flex flex-wrap gap-2">
                    {dataTypeDistribution.map((item, idx) => (
                      <Badge key={idx} variant="outline">
                        {item.type}: {item.count}
                      </Badge>
                    ))}
                  </div>
                </>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Nullability Breakdown */}
        <TabsContent value="nullability">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <PieChartIcon className="h-5 w-5 text-green-600" />
                Nullability Breakdown
              </CardTitle>
              <CardDescription>
                Proportion of nullable vs non-nullable columns
              </CardDescription>
            </CardHeader>
            <CardContent>
              {nullabilityDistribution.every((d) => d.value === 0) ? (
                <div className="text-center py-12 text-muted-foreground">
                  No data available
                </div>
              ) : (
                <>
                  <ResponsiveContainer width="100%" height={300}>
                    <PieChart>
                      <Pie
                        data={nullabilityDistribution}
                        cx="50%"
                        cy="50%"
                        labelLine={false}
                        label={({ name, percent }: any) => `${name} (${(percent * 100).toFixed(0)}%)`}
                        outerRadius={100}
                        fill="#8884d8"
                        dataKey="value"
                      >
                        {nullabilityDistribution.map((entry, index) => (
                          <Cell key={`cell-${index}`} fill={entry.color} />
                        ))}
                      </Pie>
                      <Tooltip content={<CustomTooltip />} />
                    </PieChart>
                  </ResponsiveContainer>

                  <div className="mt-4 grid grid-cols-2 gap-4 text-center">
                    {nullabilityDistribution.map((item, idx) => (
                      <div key={idx} className="p-3 border rounded-lg">
                        <div className="text-2xl font-bold" style={{ color: item.color }}>
                          {item.value}
                        </div>
                        <div className="text-xs text-muted-foreground">{item.name}</div>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Cardinality Distribution */}
        <TabsContent value="cardinality">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Activity className="h-5 w-5 text-purple-600" />
                Cardinality Distribution
              </CardTitle>
              <CardDescription>
                Distribution of distinct value counts across columns
              </CardDescription>
            </CardHeader>
            <CardContent>
              {cardinalityDistribution.length === 0 ? (
                <div className="text-center py-12 text-muted-foreground">
                  <Database className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p>Cardinality data not available</p>
                  <p className="text-xs mt-1">Run schema profiling to collect this data</p>
                </div>
              ) : (
                <ResponsiveContainer width="100%" height={300}>
                  <BarChart data={cardinalityDistribution}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />
                    <XAxis dataKey="label" tick={{ fontSize: 12 }} />
                    <YAxis tick={{ fontSize: 12 }} />
                    <Tooltip content={<CustomTooltip />} />
                    <Legend />
                    <Bar
                      dataKey="count"
                      name="Column Count"
                      fill="#8b5cf6"
                      radius={[4, 4, 0, 0]}
                    />
                  </BarChart>
                </ResponsiveContainer>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        {/* Top Values */}
        <TabsContent value="topvalues">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <TrendingUp className="h-5 w-5 text-orange-600" />
                Top Values
              </CardTitle>
              <CardDescription>
                Most frequent values
                {topValuesData && ` in column "${topValuesData.columnName}"`}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {!topValuesData ? (
                <div className="text-center py-12 text-muted-foreground">
                  <TrendingUp className="h-12 w-12 mx-auto mb-3 opacity-50" />
                  <p>Top values data not available</p>
                  <p className="text-xs mt-1">Select a table with profiled data</p>
                </div>
              ) : (
                <ResponsiveContainer width="100%" height={300}>
                  <BarChart data={topValuesData.data} layout="horizontal">
                    <CartesianGrid strokeDasharray="3 3" stroke="#e5e7eb" />
                    <XAxis type="number" tick={{ fontSize: 12 }} />
                    <YAxis
                      type="category"
                      dataKey="value"
                      tick={{ fontSize: 11 }}
                      width={100}
                    />
                    <Tooltip content={<CustomTooltip />} />
                    <Legend />
                    <Bar
                      dataKey="count"
                      name="Occurrences"
                      fill="#f59e0b"
                      radius={[0, 4, 4, 0]}
                    />
                  </BarChart>
                </ResponsiveContainer>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      {/* Statistics Summary */}
      {tableStats && (
        <Card className="mt-4">
          <CardHeader>
            <CardTitle className="text-base">Table Statistics</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-3 gap-4 text-center">
              <div>
                <div className="text-2xl font-bold text-primary">{tableStats.totalColumns}</div>
                <div className="text-xs text-muted-foreground">Total Columns</div>
              </div>
              <div>
                <div className="text-2xl font-bold text-amber-600">
                  {tableStats.nullableColumns}
                </div>
                <div className="text-xs text-muted-foreground">
                  Nullable ({tableStats.nullablePercentage}%)
                </div>
              </div>
              <div>
                <div className="text-2xl font-bold text-green-600">
                  {selectedTable?.row_count?.toLocaleString() || 'N/A'}
                </div>
                <div className="text-xs text-muted-foreground">Rows</div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

// Normalize data types into common categories
function normalizeDataType(type: string): string {
  const upperType = type.toUpperCase();

  if (upperType.includes('INT') || upperType.includes('SERIAL')) {
    return 'INTEGER';
  } else if (upperType.includes('NUMERIC') || upperType.includes('DECIMAL') || upperType.includes('FLOAT') || upperType.includes('DOUBLE')) {
    return 'NUMERIC';
  } else if (upperType.includes('VARCHAR') || upperType.includes('CHAR')) {
    return 'VARCHAR';
  } else if (upperType.includes('TEXT') || upperType.includes('CLOB')) {
    return 'TEXT';
  } else if (upperType.includes('DATE')) {
    return 'DATE';
  } else if (upperType.includes('TIME')) {
    return 'TIMESTAMP';
  } else if (upperType.includes('BOOL')) {
    return 'BOOLEAN';
  } else if (upperType.includes('JSON') || upperType.includes('JSONB')) {
    return 'JSON';
  } else if (upperType.includes('BLOB') || upperType.includes('BYTEA')) {
    return 'BINARY';
  }

  return type;
}
