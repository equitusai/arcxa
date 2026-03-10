/**
 * Discovery Results View
 * Displays the schema returned by the coordinator discovery API.
 */

import React, { useMemo, useState } from 'react';
import { motion } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
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
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  Database,
  Table as TableIcon,
  Search,
  ChevronDown,
  ChevronRight,
  Key,
  GitBranch,
  CheckCircle2,
  Brain,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { DiscoveryResult, DiscoveredTable } from '@/types/discovery';

interface DiscoveryResultsProps {
  result: DiscoveryResult;
  onStartMapping?: (table: DiscoveredTable) => void;
  onGenerateDDL?: () => void;
}

export function DiscoveryResults({
  result,
  onStartMapping,
  onGenerateDDL,
}: DiscoveryResultsProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [expandedTables, setExpandedTables] = useState<Set<string>>(new Set());

  const filteredTables = useMemo(
    () =>
      result.tables.filter((table) => {
        if (searchQuery === '') {
          return true;
        }

        const needle = searchQuery.toLowerCase();
        return (
          table.name.toLowerCase().includes(needle) ||
          table.columns.some((column) => column.name.toLowerCase().includes(needle))
        );
      }),
    [result.tables, searchQuery]
  );

  const totalColumns = useMemo(
    () => result.tables.reduce((sum, table) => sum + table.columns.length, 0),
    [result.tables]
  );

  const semanticColumns = useMemo(
    () =>
      result.tables.reduce(
        (sum, table) =>
          sum + table.columns.filter((column) => Boolean(column.semantic_type)).length,
        0
      ),
    [result.tables]
  );

  const toggleTable = (tableName: string) => {
    const next = new Set(expandedTables);
    if (next.has(tableName)) {
      next.delete(tableName);
    } else {
      next.add(tableName);
    }
    setExpandedTables(next);
  };

  const expandAll = () => {
    setExpandedTables(new Set(filteredTables.map((table) => table.name)));
  };

  const collapseAll = () => {
    setExpandedTables(new Set());
  };

  const getTypeColor = (type: string): string => {
    const upperType = type.toUpperCase();
    if (
      upperType.includes('INT') ||
      upperType.includes('NUMERIC') ||
      upperType.includes('DECIMAL')
    ) {
      return 'bg-blue-100 text-blue-700 border-blue-200';
    }
    if (
      upperType.includes('VARCHAR') ||
      upperType.includes('TEXT') ||
      upperType.includes('CHAR')
    ) {
      return 'bg-green-100 text-green-700 border-green-200';
    }
    if (upperType.includes('DATE') || upperType.includes('TIME')) {
      return 'bg-purple-100 text-purple-700 border-purple-200';
    }
    if (upperType.includes('BOOL')) {
      return 'bg-orange-100 text-orange-700 border-orange-200';
    }
    return 'bg-gray-100 text-gray-700 border-gray-200';
  };

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                <Database className="h-5 w-5" />
                Discovery Complete
              </CardTitle>
              <CardDescription className="mt-1">
                Cached at {new Date(result.cached_at).toLocaleString()}
              </CardDescription>
            </div>
            <Badge variant="outline" className="text-green-600 border-green-600">
              <CheckCircle2 className="h-3 w-3 mr-1" />
              Success
            </Badge>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
            <StatItem label="Tables" value={result.total} />
            <StatItem label="Loaded" value={result.tables.length} />
            <StatItem label="Columns" value={totalColumns} />
            <StatItem label="Semantic Hits" value={semanticColumns} />
          </div>

          <div className="flex flex-wrap gap-2">
            {onGenerateDDL && (
              <Button variant="outline" size="sm" onClick={onGenerateDDL}>
                Generate DDL
              </Button>
            )}
            {onStartMapping && (
              <Button
                size="sm"
                disabled={filteredTables.length === 0}
                onClick={() => {
                  if (filteredTables[0]) {
                    onStartMapping(filteredTables[0]);
                  }
                }}
              >
                <GitBranch className="h-4 w-4 mr-2" />
                Start Mapping
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Discovered Tables</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
              <Input
                type="search"
                placeholder="Search tables and columns..."
                className="pl-9"
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
              />
            </div>
            <Button size="sm" variant="outline" onClick={expandAll}>
              Expand All
            </Button>
            <Button size="sm" variant="outline" onClick={collapseAll}>
              Collapse All
            </Button>
          </div>

          <ScrollArea className="h-[600px] border rounded-md">
            <div className="p-4 space-y-2">
              {filteredTables.length === 0 ? (
                <div className="text-center py-8 text-muted-foreground">
                  No tables found matching your filters.
                </div>
              ) : (
                filteredTables.map((table) => (
                  <TableCard
                    key={table.name}
                    table={table}
                    isExpanded={expandedTables.has(table.name)}
                    onToggle={() => toggleTable(table.name)}
                    onStartMapping={onStartMapping}
                    getTypeColor={getTypeColor}
                  />
                ))
              )}
            </div>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}

interface StatItemProps {
  label: string;
  value: string | number;
}

function StatItem({ label, value }: StatItemProps) {
  return (
    <div className="border rounded-lg p-3 space-y-1">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="text-2xl font-bold">{value}</div>
    </div>
  );
}

interface TableCardProps {
  table: DiscoveredTable;
  isExpanded: boolean;
  onToggle: () => void;
  onStartMapping?: (table: DiscoveredTable) => void;
  getTypeColor: (type: string) => string;
}

function TableCard({
  table,
  isExpanded,
  onToggle,
  onStartMapping,
  getTypeColor,
}: TableCardProps) {
  const semanticColumns = table.columns.filter((column) => Boolean(column.semantic_type)).length;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.15 }}
    >
      <Collapsible open={isExpanded} onOpenChange={onToggle}>
        <Card className="overflow-hidden">
          <CollapsibleTrigger asChild>
            <CardHeader className="p-4 cursor-pointer hover:bg-muted/50 transition-colors">
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-3">
                  {isExpanded ? (
                    <ChevronDown className="h-4 w-4 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-muted-foreground" />
                  )}
                  <TableIcon className="h-5 w-5 text-primary" />
                  <div>
                    <h4 className="font-semibold">{table.name}</h4>
                    <div className="flex items-center gap-2 mt-1 flex-wrap">
                      <span className="text-xs text-muted-foreground">
                        {table.columns.length} columns
                      </span>
                      {typeof table.row_count === 'number' && (
                        <span className="text-xs text-muted-foreground">
                          • {table.row_count.toLocaleString()} rows
                        </span>
                      )}
                      {semanticColumns > 0 && (
                        <Badge variant="secondary" className="text-xs">
                          <Brain className="h-3 w-3 mr-1" />
                          {semanticColumns} semantic matches
                        </Badge>
                      )}
                    </div>
                  </div>
                </div>
                {onStartMapping && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={(event) => {
                      event.stopPropagation();
                      onStartMapping(table);
                    }}
                  >
                    <GitBranch className="h-3 w-3 mr-1" />
                    Map
                  </Button>
                )}
              </div>
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="p-0">
              <div className="border-t">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-[22%]">Column</TableHead>
                      <TableHead className="w-[18%]">Type</TableHead>
                      <TableHead className="w-[12%]">Nullable</TableHead>
                      <TableHead className="w-[15%]">Keys</TableHead>
                      <TableHead className="w-[18%]">Semantic Type</TableHead>
                      <TableHead className="w-[15%]">Sample Values</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {table.columns.map((column) => (
                      <TableRow key={column.name}>
                        <TableCell className="font-medium font-mono text-sm">
                          {column.name}
                        </TableCell>
                        <TableCell>
                          <Badge
                            variant="outline"
                            className={cn('text-xs', getTypeColor(column.data_type))}
                          >
                            {column.data_type}
                          </Badge>
                        </TableCell>
                        <TableCell>
                          {column.nullable ? (
                            <span className="text-xs text-muted-foreground">Yes</span>
                          ) : (
                            <Badge variant="secondary" className="text-xs">
                              NOT NULL
                            </Badge>
                          )}
                        </TableCell>
                        <TableCell>
                          {column.primary_key ? (
                            <Badge variant="default" className="text-xs">
                              <Key className="h-3 w-3 mr-1" />
                              PK
                            </Badge>
                          ) : (
                            <span className="text-xs text-muted-foreground">-</span>
                          )}
                        </TableCell>
                        <TableCell>
                          {column.semantic_type ? (
                            <div className="space-y-1">
                              <Badge variant="outline" className="text-xs">
                                {column.semantic_type}
                              </Badge>
                              <div className="text-[11px] text-muted-foreground">
                                {(column.confidence * 100).toFixed(0)}% confidence
                              </div>
                            </div>
                          ) : (
                            <span className="text-xs text-muted-foreground">-</span>
                          )}
                        </TableCell>
                        <TableCell>
                          {column.sample_values.length > 0 ? (
                            <div className="text-xs text-muted-foreground font-mono truncate max-w-[220px]">
                              {column.sample_values.slice(0, 3).join(', ')}
                            </div>
                          ) : (
                            <span className="text-xs text-muted-foreground">-</span>
                          )}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>
    </motion.div>
  );
}
