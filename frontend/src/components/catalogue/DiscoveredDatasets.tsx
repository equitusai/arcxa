/**
 * Discovered Datasets Component
 * Shows automatically discovered tables from connected datasources
 */

import { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Sparkles, Database, Table, Plus, Loader2, RefreshCw } from 'lucide-react';
import { useDatasources } from '@/hooks/useDatasources';
import { discoverSchema } from '@/api/discovery';
import { toast } from 'sonner';

interface DiscoveredTable {
  datasourceId: string;
  datasourceName: string;
  tableName: string;
  schema: string;
  rowCount?: number;
  columnCount?: number;
  sizeBytes?: number;
  lastSeen: string;
}

interface DiscoveredDatasetsProps {
  onImport: (datasourceId?: string, tableName?: string) => void;
}

export function DiscoveredDatasets({ onImport }: DiscoveredDatasetsProps) {
  const [discoveredTables, setDiscoveredTables] = useState<DiscoveredTable[]>([]);
  const [isDiscovering, setIsDiscovering] = useState(false);
  const { data: datasources } = useDatasources();

  useEffect(() => {
    if (datasources && datasources.length > 0) {
      discoverTables();
    }
  }, [datasources]);

  const discoverTables = async () => {
    if (!datasources || datasources.length === 0) return;

    setIsDiscovering(true);

    try {
      const discovered: DiscoveredTable[] = [];

      // Discover tables from each datasource in parallel
      const discoveryPromises = datasources.map(async (ds) => {
        try {
          const response = await discoverSchema(ds.id);

          // Transform each table to our DiscoveredTable format
          return response.tables.map((table) => ({
            datasourceId: ds.id,
            datasourceName: ds.name,
            tableName: table.name,
            schema: response.name, // schema name from response
            rowCount: table.estimatedRows,
            columnCount: table.columns.length,
            // Estimate size: avg 100 bytes per row * row count (rough estimate)
            sizeBytes: table.estimatedRows ? table.estimatedRows * 100 : undefined,
            lastSeen: response.inferredAt,
          }));
        } catch (error) {
          console.error(`Failed to discover tables from ${ds.name}:`, error);
          return [];
        }
      });

      const results = await Promise.all(discoveryPromises);
      const allDiscovered = results.flat();

      setDiscoveredTables(allDiscovered);

      if (allDiscovered.length === 0) {
        toast.info('No tables found in connected datasources');
      }
    } catch (error) {
      console.error('Failed to discover tables:', error);
      toast.error('Failed to discover tables');
    } finally {
      setIsDiscovering(false);
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
  };

  const formatNumber = (num: number) => {
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
    return num.toString();
  };

  const formatRelativeTime = (isoString: string) => {
    const date = new Date(isoString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  };

  if (!datasources || datasources.length === 0) {
    return null;
  }

  if (discoveredTables.length === 0 && !isDiscovering) {
    return null;
  }

  return (
    <Card className="border-primary/20 bg-gradient-to-r from-primary/5 to-transparent">
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            <div>
              <CardTitle className="text-lg">Discovered Datasets</CardTitle>
              <CardDescription>
                {isDiscovering ? (
                  'Scanning datasources for available tables...'
                ) : (
                  <>Found {discoveredTables.length} table{discoveredTables.length !== 1 ? 's' : ''} ready to import</>
                )}
              </CardDescription>
            </div>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={discoverTables}
            disabled={isDiscovering}
          >
            {isDiscovering ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
          </Button>
        </div>
      </CardHeader>

      {discoveredTables.length > 0 && (
        <CardContent>
          <div className="space-y-2">
            {discoveredTables.map((table, index) => (
              <div
                key={`${table.datasourceId}-${table.tableName}`}
                className="flex items-center justify-between p-3 rounded-lg border bg-background hover:bg-muted/50 transition-colors"
              >
                <div className="flex items-center gap-3 flex-1">
                  <div className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/10 text-primary">
                    <Table className="h-4 w-4" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-medium">
                        {table.datasourceName} › {table.tableName}
                      </span>
                      <Badge variant="secondary" className="text-xs">
                        New
                      </Badge>
                    </div>
                    <div className="flex gap-3 text-xs text-muted-foreground">
                      {table.rowCount && (
                        <span>{formatNumber(table.rowCount)} rows</span>
                      )}
                      {table.columnCount && (
                        <span>{table.columnCount} columns</span>
                      )}
                      {table.sizeBytes && (
                        <span>{formatBytes(table.sizeBytes)}</span>
                      )}
                      <span>•</span>
                      <span>Updated {formatRelativeTime(table.lastSeen)}</span>
                    </div>
                  </div>
                </div>
                <Button
                  size="sm"
                  onClick={() => onImport(table.datasourceId, table.tableName)}
                >
                  <Plus className="h-3.5 w-3.5 mr-1.5" />
                  Import
                </Button>
              </div>
            ))}
          </div>

          <div className="mt-4 pt-4 border-t">
            <Button
              variant="outline"
              className="w-full"
              onClick={() => onImport()}
            >
              <Plus className="h-4 w-4 mr-2" />
              Import All Discovered Tables
            </Button>
          </div>
        </CardContent>
      )}
    </Card>
  );
}
