/**
 * SPARQL Results Table
 *
 * Sortable, filterable table view with confidence heatmap
 */

import React, { useState, useMemo } from 'react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ArrowUpDown, ArrowUp, ArrowDown, Download } from 'lucide-react';
import { cn } from '@/lib/utils';

interface ResultsTableProps {
  data: Record<string, any>[];
  onCellClick?: (rowIdx: number, colKey: string, value: any) => void;
  onExport?: (format: 'csv' | 'json') => void;
  density?: 'compact' | 'comfortable';
  className?: string;
}

type SortDirection = 'asc' | 'desc' | null;

export function ResultsTable({
  data,
  onCellClick,
  onExport,
  density = 'comfortable',
  className,
}: ResultsTableProps) {
  const [filter, setFilter] = useState('');
  const [sortColumn, setSortColumn] = useState<string | null>(null);
  const [sortDirection, setSortDirection] = useState<SortDirection>(null);
  const [currentPage, setCurrentPage] = useState(1);
  const rowsPerPage = 100;

  // Extract columns from first row
  const columns = useMemo(() => {
    if (data.length === 0) return [];
    return Object.keys(data[0]);
  }, [data]);

  // Filter data
  const filteredData = useMemo(() => {
    if (!filter) return data;

    const lowerFilter = filter.toLowerCase();
    return data.filter(row =>
      Object.values(row).some(val =>
        String(val).toLowerCase().includes(lowerFilter)
      )
    );
  }, [data, filter]);

  // Sort data
  const sortedData = useMemo(() => {
    if (!sortColumn || !sortDirection) return filteredData;

    return [...filteredData].sort((a, b) => {
      const aVal = a[sortColumn];
      const bVal = b[sortColumn];

      // Handle different types
      if (typeof aVal === 'number' && typeof bVal === 'number') {
        return sortDirection === 'asc' ? aVal - bVal : bVal - aVal;
      }

      const aStr = String(aVal);
      const bStr = String(bVal);
      const comparison = aStr.localeCompare(bStr);

      return sortDirection === 'asc' ? comparison : -comparison;
    });
  }, [filteredData, sortColumn, sortDirection]);

  // Paginate data
  const paginatedData = useMemo(() => {
    const start = (currentPage - 1) * rowsPerPage;
    const end = start + rowsPerPage;
    return sortedData.slice(start, end);
  }, [sortedData, currentPage]);

  const totalPages = Math.ceil(sortedData.length / rowsPerPage);

  const handleSort = (column: string) => {
    if (sortColumn === column) {
      // Cycle through: asc → desc → null
      if (sortDirection === 'asc') {
        setSortDirection('desc');
      } else if (sortDirection === 'desc') {
        setSortDirection(null);
        setSortColumn(null);
      }
    } else {
      setSortColumn(column);
      setSortDirection('asc');
    }
  };

  const getSortIcon = (column: string) => {
    if (sortColumn !== column || !sortDirection) {
      return <ArrowUpDown className="h-3 w-3 text-muted-foreground" />;
    }
    return sortDirection === 'asc' ? (
      <ArrowUp className="h-3 w-3 text-entity" />
    ) : (
      <ArrowDown className="h-3 w-3 text-entity" />
    );
  };

  if (data.length === 0) {
    return (
      <div className="flex items-center justify-center py-12 text-sm text-muted-foreground">
        No results to display
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col h-full', className)}>
      {/* Table Controls */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <Input
          placeholder="Filter results..."
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setCurrentPage(1); // Reset to page 1 on filter
          }}
          className="w-[240px] h-8"
        />
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">
            {sortedData.length} results
          </span>
          {onExport && (
            <Select onValueChange={(format: any) => onExport(format)}>
              <SelectTrigger className="w-[120px] h-8">
                <Download className="h-3 w-3 mr-2" />
                <SelectValue placeholder="Export" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="csv">CSV</SelectItem>
                <SelectItem value="json">JSON</SelectItem>
              </SelectContent>
            </Select>
          )}
        </div>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-sm border-collapse">
          <thead className="sticky top-0 bg-background-secondary border-b border-border z-10">
            <tr>
              {columns.map(col => (
                <th
                  key={col}
                  className="px-3 py-2 text-left font-semibold text-foreground-secondary cursor-pointer hover:bg-background-tertiary transition-colors"
                  onClick={() => handleSort(col)}
                >
                  <div className="flex items-center gap-2">
                    <span className="select-none">{formatColumnName(col)}</span>
                    {getSortIcon(col)}
                  </div>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {paginatedData.map((row, rowIdx) => (
              <tr
                key={rowIdx}
                className={cn(
                  'border-b border-border-subtle hover:bg-background-secondary/50 transition-colors',
                  density === 'compact' ? 'h-8' : 'h-10'
                )}
              >
                {columns.map(col => (
                  <td
                    key={col}
                    className="px-3 py-2 cursor-pointer"
                    onClick={() => onCellClick?.(rowIdx, col, row[col])}
                    style={getConfidenceColor(col, row[col])}
                  >
                    {formatCellValue(col, row[col])}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between px-3 py-2 border-t border-border">
          <span className="text-xs text-muted-foreground">
            Page {currentPage} of {totalPages}
          </span>
          <div className="flex gap-1">
            <Button
              size="sm"
              variant="outline"
              disabled={currentPage === 1}
              onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
            >
              Previous
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={currentPage === totalPages}
              onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
            >
              Next
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Helper Functions
// ============================================================================

function formatColumnName(col: string): string {
  // Remove leading ? if present (SPARQL variable)
  const cleaned = col.startsWith('?') ? col.slice(1) : col;
  // Convert camelCase or snake_case to Title Case
  return cleaned
    .replace(/([A-Z])/g, ' $1')
    .replace(/_/g, ' ')
    .replace(/^\w/, c => c.toUpperCase())
    .trim();
}

function formatCellValue(colName: string, value: any): React.ReactNode {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  // Handle URIs
  if (typeof value === 'string' && value.startsWith('http')) {
    const shortUri = value.split('/').pop() || value;
    return (
      <span className="font-mono text-xs text-entity" title={value}>
        {shortUri}
      </span>
    );
  }

  // Handle confidence scores
  if (colName.toLowerCase().includes('confidence') && typeof value === 'number') {
    return (
      <Badge variant={value > 0.8 ? 'success' : value > 0.6 ? 'warning' : 'destructive'}>
        {value.toFixed(2)}
      </Badge>
    );
  }

  // Handle timestamps
  if (colName.toLowerCase().includes('timestamp') || colName.toLowerCase().includes('time')) {
    try {
      const date = new Date(value);
      if (!isNaN(date.getTime())) {
        return (
          <span className="text-xs text-muted-foreground">
            {date.toLocaleString()}
          </span>
        );
      }
    } catch (e) {
      // Not a valid date, fall through
    }
  }

  // Handle numbers
  if (typeof value === 'number') {
    return value.toLocaleString();
  }

  // Handle booleans
  if (typeof value === 'boolean') {
    return value ? (
      <Badge variant="success">true</Badge>
    ) : (
      <Badge variant="outline">false</Badge>
    );
  }

  // Default: string
  return String(value);
}

function getConfidenceColor(colName: string, value: any): React.CSSProperties {
  if (colName.toLowerCase().includes('confidence') && typeof value === 'number') {
    // Heatmap: 0 = red (hue 0), 1 = green (hue 120)
    const hue = value * 120;
    return {
      backgroundColor: `hsla(${hue}, 70%, 95%, 0.6)`,
    };
  }
  return {};
}
