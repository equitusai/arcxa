/**
 * Dataset Detail Inspector
 * Right-side slide-out panel with comprehensive dataset information
 */

import { useState } from 'react';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Progress } from '@/components/ui/progress';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Database,
  Table as TableIcon,
  Activity,
  GitBranch,
  TestTube,
  ExternalLink,
  RefreshCw,
  ArrowRight,
  CheckCircle,
  XCircle,
  AlertCircle,
  Calendar,
  FileText
} from 'lucide-react';
import { Dataset } from '@/api/types';
import { toast } from 'sonner';

interface DatasetDetailInspectorProps {
  dataset: Dataset | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function DatasetDetailInspector({ dataset, open, onOpenChange }: DatasetDetailInspectorProps) {
  const [activeTab, setActiveTab] = useState('overview');

  if (!dataset) return null;

  const datasetOriginLabel =
    dataset.source_name ||
    dataset.source ||
    (dataset.asset_kind === 'source_asset' ? 'Source asset' : 'Materialized dataset');

  // Mock data (replace with real API data)
  const mockSchemaColumns = dataset.schema?.fields || [
    { name: 'id', type: 'integer', nullable: false, description: 'Primary key' },
    { name: 'name', type: 'string', nullable: false, description: 'Full name' },
    { name: 'email', type: 'string', nullable: true, description: 'Email address' },
    { name: 'phone', type: 'string', nullable: true, description: 'Phone number' },
    { name: 'address', type: 'string', nullable: true, description: 'Street address' },
    { name: 'city', type: 'string', nullable: true, description: 'City' },
    { name: 'state', type: 'string', nullable: true, description: 'State/Province' },
    { name: 'country', type: 'string', nullable: true, description: 'Country' },
    { name: 'created_at', type: 'timestamp', nullable: false, description: 'Record creation timestamp' },
    { name: 'updated_at', type: 'timestamp', nullable: true, description: 'Last update timestamp' },
  ];

  const mockSampleData = [
    { id: 1, name: 'John Doe', email: 'john@example.com', phone: '+1-555-0101', city: 'New York', created_at: '2024-01-15' },
    { id: 2, name: 'Jane Smith', email: 'jane@example.com', phone: '+1-555-0102', city: 'Los Angeles', created_at: '2024-01-16' },
    { id: 3, name: 'Bob Johnson', email: 'bob@example.com', phone: '+1-555-0103', city: 'Chicago', created_at: '2024-01-17' },
    { id: 4, name: 'Alice Williams', email: 'alice@example.com', phone: '+1-555-0104', city: 'Houston', created_at: '2024-01-18' },
    { id: 5, name: 'Charlie Brown', email: 'charlie@example.com', phone: '+1-555-0105', city: 'Phoenix', created_at: '2024-01-19' },
  ];

  const mockLineage = {
    upstream: [
      { id: 'ds-1', name: 'CRM Raw Data', type: 'datasource' },
    ],
    downstream: [
      { id: 'wf-1', name: 'Customer MDM', type: 'workflow' },
      { id: 'ds-2', name: 'Analytics Warehouse', type: 'datasource' },
    ]
  };

  const mockQualityHistory = [
    { date: '2024-01-15', score: 82 },
    { date: '2024-01-16', score: 85 },
    { date: '2024-01-17', score: 83 },
    { date: '2024-01-18', score: 87 },
    { date: '2024-01-19', score: dataset.quality_score || 85 },
  ];

  const mockFusionHistory = [
    { date: '2024-01-19', workflow: 'Customer 360', entities: 1240, status: 'completed' },
    { date: '2024-01-18', workflow: 'Duplicate Detection', entities: 856, status: 'completed' },
    { date: '2024-01-17', workflow: 'Address Standardization', entities: 2100, status: 'failed' },
  ];

  const handleTestConnection = () => {
    toast.success('Connection test successful');
  };

  const handleRefreshSchema = () => {
    toast.success('Refreshing schema...');
  };

  const formatNumber = (num: number) => {
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
    return num.toString();
  };

  const formatRelativeTime = (dateStr: string) => {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffHours < 1) return 'Just now';
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${diffDays}d ago`;
  };

  const getHealthIcon = () => {
    const status = dataset.status;
    if (status === 'active') return <CheckCircle className="h-4 w-4 text-green-600" />;
    if (status === 'error') return <XCircle className="h-4 w-4 text-red-600" />;
    if (status === 'stale') return <AlertCircle className="h-4 w-4 text-yellow-600" />;
    return <Database className="h-4 w-4 text-muted-foreground" />;
  };

  const getQualityColor = (score: number) => {
    if (score >= 90) return 'text-green-600';
    if (score >= 70) return 'text-yellow-600';
    return 'text-red-600';
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full sm:max-w-2xl overflow-y-auto">
        <SheetHeader>
          <div className="flex items-start gap-2">
            <Database className="h-6 w-6 text-primary mt-1" />
            <div>
              <SheetTitle className="text-xl">{dataset.name}</SheetTitle>
              <SheetDescription className="flex items-center gap-2 mt-1">
                {getHealthIcon()}
                <span>{datasetOriginLabel}</span>
                {dataset.status && (
                  <>
                    <span>•</span>
                    <Badge variant="secondary" className="text-xs">{dataset.status}</Badge>
                  </>
                )}
                {dataset.dataset_type && (
                  <>
                    <span>•</span>
                    <Badge variant="outline" className="text-xs">{dataset.dataset_type}</Badge>
                  </>
                )}
              </SheetDescription>
            </div>
          </div>
        </SheetHeader>

        <Tabs value={activeTab} onValueChange={setActiveTab} className="mt-6">
          <TabsList className="grid w-full grid-cols-5">
            <TabsTrigger value="overview" className="text-xs">
              <FileText className="h-3.5 w-3.5 mr-1" />
              Overview
            </TabsTrigger>
            <TabsTrigger value="schema" className="text-xs">
              <TableIcon className="h-3.5 w-3.5 mr-1" />
              Schema
            </TabsTrigger>
            <TabsTrigger value="data" className="text-xs">
              <Database className="h-3.5 w-3.5 mr-1" />
              Data
            </TabsTrigger>
            <TabsTrigger value="lineage" className="text-xs">
              <GitBranch className="h-3.5 w-3.5 mr-1" />
              Lineage
            </TabsTrigger>
            <TabsTrigger value="quality" className="text-xs">
              <Activity className="h-3.5 w-3.5 mr-1" />
              Quality
            </TabsTrigger>
          </TabsList>

          {/* Overview Tab */}
          <TabsContent value="overview" className="space-y-4 mt-4">
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Key Metrics</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">Total Entities</div>
                    <div className="text-2xl font-bold">
                      {formatNumber(dataset.entity_count || dataset.record_count)}
                    </div>
                  </div>
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">Quality Score</div>
                    <div className={`text-2xl font-bold ${getQualityColor(dataset.quality_score || 0)}`}>
                      {dataset.quality_score || 0}%
                    </div>
                  </div>
                  {dataset.fusion_candidates !== undefined && (
                    <div>
                      <div className="text-xs text-muted-foreground mb-1">Fusion Candidates</div>
                      <div className="text-2xl font-bold text-primary">
                        {dataset.fusion_candidates}
                      </div>
                    </div>
                  )}
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">Last Updated</div>
                    <div className="text-sm font-medium">
                      {dataset.last_updated ? formatRelativeTime(dataset.last_updated) : 'Unknown'}
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>

            {dataset.description && (
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Description</CardTitle>
                </CardHeader>
                <CardContent>
                  <p className="text-sm text-muted-foreground">{dataset.description}</p>
                </CardContent>
              </Card>
            )}

            {/* Quality Breakdown */}
            {dataset.quality_breakdown && (
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Quality Breakdown</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  {Object.entries(dataset.quality_breakdown).map(([key, value]) => (
                    <div key={key} className="space-y-1">
                      <div className="flex justify-between text-xs">
                        <span className="capitalize">{key}</span>
                        <span className={`font-medium ${getQualityColor(value)}`}>{value}%</span>
                      </div>
                      <Progress value={value} className="h-2" />
                    </div>
                  ))}
                </CardContent>
              </Card>
            )}

            {/* Actions */}
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={handleTestConnection} className="flex-1">
                <TestTube className="h-3.5 w-3.5 mr-1.5" />
                Test Connection
              </Button>
              <Button variant="outline" size="sm" onClick={handleRefreshSchema} className="flex-1">
                <RefreshCw className="h-3.5 w-3.5 mr-1.5" />
                Refresh Schema
              </Button>
            </div>
          </TabsContent>

          {/* Schema Tab */}
          <TabsContent value="schema" className="mt-4">
            <Card>
              <CardHeader className="pb-3">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-sm">Columns ({mockSchemaColumns.length})</CardTitle>
                  <Button variant="ghost" size="sm" onClick={handleRefreshSchema}>
                    <RefreshCw className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </CardHeader>
              <CardContent>
                <ScrollArea className="h-[500px]">
                  <div className="space-y-3">
                    {mockSchemaColumns.map((col, index) => (
                      <div key={index} className="p-3 border rounded-lg">
                        <div className="flex items-center justify-between mb-2">
                          <span className="font-mono font-medium text-sm">{col.name}</span>
                          <div className="flex items-center gap-2">
                            <Badge variant="secondary" className="text-xs">
                              {col.type}
                            </Badge>
                            {col.nullable && (
                              <Badge variant="outline" className="text-xs">nullable</Badge>
                            )}
                          </div>
                        </div>
                        {col.description && (
                          <p className="text-xs text-muted-foreground">{col.description}</p>
                        )}
                      </div>
                    ))}
                  </div>
                </ScrollArea>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Data Tab */}
          <TabsContent value="data" className="mt-4">
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Sample Data (First 5 rows)</CardTitle>
              </CardHeader>
              <CardContent>
                <ScrollArea className="h-[500px]">
                  <div className="border rounded-lg overflow-hidden">
                    <table className="w-full text-xs">
                      <thead className="bg-muted">
                        <tr>
                          {Object.keys(mockSampleData[0] || {}).map((key) => (
                            <th key={key} className="px-3 py-2 text-left font-medium whitespace-nowrap">
                              {key}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {mockSampleData.map((row, index) => (
                          <tr key={index} className="border-t">
                            {Object.values(row).map((value, colIndex) => (
                              <td key={colIndex} className="px-3 py-2 whitespace-nowrap">
                                {value}
                              </td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </ScrollArea>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Lineage Tab */}
          <TabsContent value="lineage" className="mt-4">
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Data Lineage</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                {/* Upstream */}
                {mockLineage.upstream.length > 0 && (
                  <div>
                    <div className="text-xs font-medium text-muted-foreground mb-2">Upstream Sources</div>
                    <div className="space-y-2">
                      {mockLineage.upstream.map((item) => (
                        <div key={item.id} className="flex items-center gap-2 p-2 border rounded-lg">
                          <Database className="h-4 w-4 text-muted-foreground" />
                          <div className="flex-1">
                            <div className="text-sm font-medium">{item.name}</div>
                            <div className="text-xs text-muted-foreground">{item.type}</div>
                          </div>
                          <Button variant="ghost" size="sm">
                            <ExternalLink className="h-3.5 w-3.5" />
                          </Button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* Current Dataset */}
                <div className="flex items-center justify-center py-2">
                  <ArrowRight className="h-4 w-4 text-muted-foreground" />
                  <div className="mx-3 px-4 py-2 bg-primary/10 border border-primary rounded-lg">
                    <div className="text-sm font-medium">{dataset.name}</div>
                  </div>
                  <ArrowRight className="h-4 w-4 text-muted-foreground" />
                </div>

                {/* Downstream */}
                {mockLineage.downstream.length > 0 && (
                  <div>
                    <div className="text-xs font-medium text-muted-foreground mb-2">Downstream Consumers</div>
                    <div className="space-y-2">
                      {mockLineage.downstream.map((item) => (
                        <div key={item.id} className="flex items-center gap-2 p-2 border rounded-lg">
                          <Database className="h-4 w-4 text-muted-foreground" />
                          <div className="flex-1">
                            <div className="text-sm font-medium">{item.name}</div>
                            <div className="text-xs text-muted-foreground">{item.type}</div>
                          </div>
                          <Button variant="ghost" size="sm">
                            <ExternalLink className="h-3.5 w-3.5" />
                          </Button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>
          </TabsContent>

          {/* Quality Tab */}
          <TabsContent value="quality" className="mt-4 space-y-4">
            {/* Quality Trend */}
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Quality Trend (Last 5 Days)</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-2">
                  {mockQualityHistory.map((item, index) => (
                    <div key={index} className="flex items-center gap-3">
                      <div className="text-xs text-muted-foreground w-24">
                        <Calendar className="h-3 w-3 inline mr-1" />
                        {item.date}
                      </div>
                      <div className="flex-1">
                        <Progress value={item.score} className="h-2" />
                      </div>
                      <div className={`text-sm font-medium w-12 text-right ${getQualityColor(item.score)}`}>
                        {item.score}%
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>

            {/* Fusion History */}
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Recent Fusion Workflows</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-3">
                  {mockFusionHistory.map((item, index) => (
                    <div key={index} className="p-3 border rounded-lg">
                      <div className="flex items-center justify-between mb-1">
                        <span className="font-medium text-sm">{item.workflow}</span>
                        <Badge variant={item.status === 'completed' ? 'default' : 'destructive'} className="text-xs">
                          {item.status}
                        </Badge>
                      </div>
                      <div className="text-xs text-muted-foreground">
                        <Calendar className="h-3 w-3 inline mr-1" />
                        {item.date} • {formatNumber(item.entities)} entities
                      </div>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </SheetContent>
    </Sheet>
  );
}
