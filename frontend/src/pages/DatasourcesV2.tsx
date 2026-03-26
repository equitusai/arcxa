/**
 * Datasources V2 - Enhanced Control Plane
 * Premium datasource management with modern UX patterns
 */

import { useState } from 'react';
import { motion } from 'framer-motion';
import {
  Database,
  Plus,
  Search,
  Filter,
  Loader2,
  Activity,
  AlertTriangle,
  CheckCircle2,
} from 'lucide-react';
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
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';

// Custom components
import { DatasourceCard } from '@/components/datasource/DatasourceCard';
import { DatasourceStatCard } from '@/components/datasource/DatasourceStatCard';
import { DatasourceHealthWidget } from '@/components/datasource/DatasourceHealthWidget';
import { DatasourceActivityFeed, DatasourceEvent } from '@/components/datasource/DatasourceActivityFeed';
import { DatasourceQuickActions } from '@/components/datasource/DatasourceQuickActions';
import { DatasourceWizardEnhanced as DatasourceWizard } from '@/components/datasource/DatasourceWizardEnhanced';

// Hooks
import {
  useDatasources,
  useDeleteDatasource,
  useTestConnection,
  useDatasourceHealth,
  useDatasourceSchema,
  useDatasourceStats,
} from '@/hooks/useDatasources';

// Types
import type { Datasource } from '@/api/types';

// Generate sparkline data (24 hours)
const generateSparklineData = (baseValue: number, variance: number = 0.15) => {
  return Array.from({ length: 24 }, (_, i) => {
    const trend = Math.sin(i / 4) * variance;
    const random = (Math.random() - 0.5) * variance;
    return Math.max(0, baseValue * (1 + trend + random));
  });
};

// Mock activity events (replace with real data)
const generateMockEvents = (datasources: Datasource[]): DatasourceEvent[] => {
  const events: DatasourceEvent[] = [];
  const eventTypes: DatasourceEvent['event_type'][] = [
    'connection',
    'test',
    'schema_refresh',
    'enable',
    'disable',
  ];

  for (let i = 0; i < 10; i++) {
    const ds = datasources[Math.floor(Math.random() * datasources.length)];
    if (!ds) continue;

    const eventType = eventTypes[Math.floor(Math.random() * eventTypes.length)];
    const isSuccess = Math.random() > 0.3;

    events.push({
      id: `event-${i}`,
      datasource_name: ds.name,
      event_type: eventType,
      message: getEventMessage(eventType, isSuccess),
      timestamp: new Date(Date.now() - Math.random() * 3600000 * 12), // Last 12 hours
      status: isSuccess ? 'success' : Math.random() > 0.5 ? 'warning' : 'error',
    });
  }

  return events.sort((a, b) => b.timestamp.getTime() - a.timestamp.getTime());
};

function getEventMessage(type: string, success: boolean): string {
  const messages: Record<string, { success: string; failure: string }> = {
    connection: {
      success: 'Successfully connected to data source',
      failure: 'Failed to establish connection',
    },
    test: {
      success: 'Connection test passed',
      failure: 'Connection test failed',
    },
    schema_refresh: {
      success: 'Schema refreshed successfully',
      failure: 'Schema refresh failed',
    },
    enable: {
      success: 'Data source enabled',
      failure: 'Failed to enable data source',
    },
    disable: {
      success: 'Data source disabled',
      failure: 'Failed to disable data source',
    },
  };

  return success ? messages[type]?.success : messages[type]?.failure;
}

export function DatasourcesV2() {
  // State
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedDatasource, setSelectedDatasource] = useState<Datasource | null>(null);
  const [showDetails, setShowDetails] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [datasourceToDelete, setDatasourceToDelete] = useState<string | null>(null);
  const [showWizard, setShowWizard] = useState(false);
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [typeFilter, setTypeFilter] = useState<string>('all');
  const statusFilterLabel =
    {
      connected: 'Connected',
      unverified: 'Unverified',
      disconnected: 'Disconnected',
      error: 'Error',
      disabled: 'Disabled',
    }[statusFilter] || statusFilter;

  // Data fetching
  const { data: datasources, isLoading } = useDatasources();
  const { data: stats } = useDatasourceStats();
  const deleteDatasource = useDeleteDatasource();
  const testConnection = useTestConnection();

  // Filter datasources
  const filteredDatasources = datasources?.filter((ds) => {
    const matchesSearch =
      ds.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      ds.plugin_name.toLowerCase().includes(searchQuery.toLowerCase());

    const matchesStatus =
      statusFilter === 'all' ||
      (statusFilter === 'connected' && ds.status === 'Connected') ||
      (statusFilter === 'unverified' && ds.status === 'Unverified') ||
      (statusFilter === 'disconnected' && ds.status === 'Disconnected') ||
      (statusFilter === 'error' && typeof ds.status === 'object' && 'Error' in ds.status) ||
      (statusFilter === 'disabled' && !ds.enabled);

    const dsType = typeof ds.metadata.datasource_type === 'string'
      ? ds.metadata.datasource_type
      : ds.metadata.datasource_type?.Custom || 'Unknown';
    const matchesType = typeFilter === 'all' || dsType === typeFilter;

    return matchesSearch && matchesStatus && matchesType;
  });

  // Generate stats with sparklines
  const statsData = [
    {
      title: 'Total Data Sources',
      value: stats?.total_datasources ?? 0,
      icon: Database,
      status: 'info' as const,
      sparklineData: generateSparklineData(stats?.total_datasources ?? 0, 0.1),
      trend: { value: 5.2, isPositive: true },
      action: {
        label: 'View All',
        onClick: () => {},
      },
    },
    {
      title: 'Connected',
      value: stats?.connected ?? 0,
      icon: CheckCircle2,
      status: 'success' as const,
      sparklineData: generateSparklineData(stats?.connected ?? 0, 0.15),
      trend: { value: 8.1, isPositive: true },
      action: {
        label: 'Test All',
        onClick: () => {},
      },
    },
    {
      title: 'Disconnected',
      value: stats?.disconnected ?? 0,
      icon: Activity,
      status: 'neutral' as const,
      sparklineData: generateSparklineData(stats?.disconnected ?? 0, 0.2),
      trend: { value: 2.3, isPositive: false },
      action: {
        label: 'Reconnect',
        onClick: () => {},
      },
    },
    {
      title: 'Errors',
      value: stats?.errors ?? 0,
      icon: AlertTriangle,
      status: stats && stats.errors > 0 ? ('error' as const) : ('neutral' as const),
      sparklineData: generateSparklineData(stats?.errors ?? 0, 0.3),
      trend: { value: 1.5, isPositive: false },
      action: {
        label: 'Diagnose',
        onClick: () => {},
      },
    },
  ];

  // Mock activity events
  const activityEvents = datasources ? generateMockEvents(datasources) : [];

  // Handlers
  const handleDelete = (id: string) => {
    setDatasourceToDelete(id);
    setShowDeleteDialog(true);
  };

  const confirmDelete = () => {
    if (datasourceToDelete) {
      deleteDatasource.mutate(datasourceToDelete);
      setShowDeleteDialog(false);
      setDatasourceToDelete(null);
    }
  };

  const handleTestConnection = (id: string) => {
    testConnection.mutate(id);
  };

  const handleViewDetails = (ds: Datasource) => {
    setSelectedDatasource(ds);
    setShowDetails(true);
  };

  // Get unique types for filter
  const datasourceTypes = Array.from(
    new Set(
      datasources?.map((ds) =>
        typeof ds.metadata.datasource_type === 'string'
          ? ds.metadata.datasource_type
          : ds.metadata.datasource_type?.Custom || 'Unknown'
      ) || []
    )
  );

  return (
    <div className="space-y-6 pb-8">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3 }}
        className="flex items-start justify-between pb-6 border-b border-border"
      >
        <div>
          <h1 className="text-3xl font-bold bg-gradient-to-r from-primary to-primary/60 bg-clip-text text-transparent">
            Data Sources
          </h1>
          <p className="text-sm text-muted-foreground mt-1">
            Manage trusted connections to the systems that feed ARCXA
          </p>
        </div>

        <Button className="gap-2" onClick={() => setShowWizard(true)}>
          <Plus className="h-4 w-4" />
          Add Data Source
        </Button>
      </motion.div>

      {/* Enhanced Stats Grid */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.4, delay: 0.1 }}
        className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4"
      >
        {statsData.map((stat, index) => (
          <motion.div
            key={stat.title}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3, delay: 0.1 + index * 0.05 }}
          >
            <DatasourceStatCard {...stat} />
          </motion.div>
        ))}
      </motion.div>

      {/* Main Content - 70/30 Split */}
      <div className="grid grid-cols-1 lg:grid-cols-10 gap-6">
        {/* Left Column - Datasource Grid + Search */}
        <div className="lg:col-span-7 space-y-6">
          {/* Search and Filters */}
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.3, delay: 0.3 }}
            className="flex flex-col sm:flex-row gap-3"
          >
            {/* Search */}
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder="Search data sources by name or type..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9"
              />
            </div>

            {/* Status Filter */}
            <Select value={statusFilter} onValueChange={setStatusFilter}>
              <SelectTrigger className="w-full sm:w-[160px]">
                <Filter className="h-4 w-4 mr-2" />
                <SelectValue placeholder="Status" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Status</SelectItem>
                <SelectItem value="connected">Connected</SelectItem>
                <SelectItem value="unverified">Unverified</SelectItem>
                <SelectItem value="disconnected">Disconnected</SelectItem>
                <SelectItem value="error">Error</SelectItem>
                <SelectItem value="disabled">Disabled</SelectItem>
              </SelectContent>
            </Select>

            {/* Type Filter */}
            <Select value={typeFilter} onValueChange={setTypeFilter}>
              <SelectTrigger className="w-full sm:w-[160px]">
                <Database className="h-4 w-4 mr-2" />
                <SelectValue placeholder="Type" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Types</SelectItem>
                {datasourceTypes.map((type) => (
                  <SelectItem key={type} value={type}>
                    {type}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </motion.div>

          {/* Active Filters */}
          {(statusFilter !== 'all' || typeFilter !== 'all' || searchQuery) && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              className="flex items-center gap-2 flex-wrap"
            >
              <span className="text-sm text-muted-foreground">Active filters:</span>
              {searchQuery && (
                <Badge variant="secondary" className="gap-1">
                  Search: {searchQuery}
                </Badge>
              )}
              {statusFilter !== 'all' && (
                <Badge variant="secondary" className="gap-1">
                  Status: {statusFilterLabel}
                </Badge>
              )}
              {typeFilter !== 'all' && (
                <Badge variant="secondary" className="gap-1">
                  Type: {typeFilter}
                </Badge>
              )}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setSearchQuery('');
                  setStatusFilter('all');
                  setTypeFilter('all');
                }}
                className="h-6 text-xs"
              >
                Clear all
              </Button>
            </motion.div>
          )}

          {/* Datasource Grid */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.3, delay: 0.4 }}
          >
            {isLoading ? (
              <div className="flex items-center justify-center py-12">
                <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
              </div>
            ) : filteredDatasources && filteredDatasources.length > 0 ? (
              <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
                {filteredDatasources.map((datasource, idx) => (
                  <motion.div
                    key={datasource.id}
                    initial={{ opacity: 0, scale: 0.95 }}
                    animate={{ opacity: 1, scale: 1 }}
                    transition={{ duration: 0.2, delay: idx * 0.03 }}
                  >
                    <DatasourceCard
                      datasource={datasource}
                      onViewDetails={() => handleViewDetails(datasource)}
                      onTest={() => handleTestConnection(datasource.id)}
                      onDelete={() => handleDelete(datasource.id)}
                    />
                  </motion.div>
                ))}
              </div>
            ) : (
              <motion.div
                initial={{ opacity: 0, scale: 0.95 }}
                animate={{ opacity: 1, scale: 1 }}
                className="text-center py-12 border-2 border-dashed border-border rounded-lg"
              >
                <Database className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <p className="text-sm text-muted-foreground">
                  {searchQuery || statusFilter !== 'all' || typeFilter !== 'all'
                    ? 'No data sources match your filters'
                    : 'No data sources configured yet'}
                </p>
                {!searchQuery && statusFilter === 'all' && typeFilter === 'all' && (
                  <Button className="mt-4 gap-2" onClick={() => setShowWizard(true)}>
                    <Plus className="h-4 w-4" />
                    Add Your First Data Source
                  </Button>
                )}
              </motion.div>
            )}
          </motion.div>
        </div>

        {/* Right Column - Health Widget + Activity + Quick Actions */}
        <div className="lg:col-span-3 space-y-6">
          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, delay: 0.3 }}
          >
            <DatasourceHealthWidget
              datasources={datasources || []}
              onTestAll={() => {
                datasources?.forEach((ds) => testConnection.mutate(ds.id));
              }}
              onRefreshAll={() => window.location.reload()}
            />
          </motion.div>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, delay: 0.4 }}
          >
            <DatasourceQuickActions
              onAddDatasource={() => setShowWizard(true)}
              onTestAll={() => {
                datasources?.forEach((ds) => testConnection.mutate(ds.id));
              }}
              onRefreshAll={() => window.location.reload()}
              recentDatasources={datasources?.slice(0, 3) || []}
            />
          </motion.div>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, delay: 0.5 }}
          >
            <DatasourceActivityFeed events={activityEvents} showLiveIndicator maxEvents={8} />
          </motion.div>
        </div>
      </div>

      {/* Datasource Details Sheet */}
      {selectedDatasource && (
        <DatasourceDetailsSheet
          datasource={selectedDatasource}
          open={showDetails}
          onOpenChange={setShowDetails}
        />
      )}

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Data Source</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete this data source? This action cannot be undone and
              may affect workflows that depend on it.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={confirmDelete}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Datasource Wizard */}
      <DatasourceWizard open={showWizard} onOpenChange={setShowWizard} />
    </div>
  );
}

/**
 * Datasource Details Sheet Component
 */
function DatasourceDetailsSheet({
  datasource,
  open,
  onOpenChange,
}: {
  datasource: Datasource;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { data: health } = useDatasourceHealth(datasource.id);
  const { data: schema } = useDatasourceSchema(datasource.id);

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[600px] sm:w-[700px] overflow-y-auto">
        <SheetHeader>
          <SheetTitle>{datasource.name}</SheetTitle>
          <SheetDescription>{datasource.metadata.description}</SheetDescription>
        </SheetHeader>

        <div className="mt-6 space-y-6">
          {/* Overview */}
          <div>
            <h3 className="text-sm font-semibold mb-3">Overview</h3>
            <div className="space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-muted-foreground">Plugin:</span>
                <span className="font-medium">{datasource.metadata.name}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Version:</span>
                <span className="font-medium">{datasource.metadata.version}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Type:</span>
                <span className="font-medium">
                  {typeof datasource.metadata.datasource_type === 'string'
                    ? datasource.metadata.datasource_type
                    : datasource.metadata.datasource_type?.Custom || 'Unknown'}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Status:</span>
                <span>{String(datasource.status)}</span>
              </div>
            </div>
          </div>

          {/* Health Status */}
          {health && (
            <div>
              <h3 className="text-sm font-semibold mb-3">Health Status</h3>
              <div className="p-3 bg-muted rounded-sm">
                <div className="flex justify-between mb-2">
                  <span className="text-sm">Status:</span>
                  <span className="text-sm font-medium">{String(health.status)}</span>
                </div>
                {health.issues && health.issues.length > 0 && (
                  <div className="mt-2">
                    <span className="text-xs text-muted-foreground">Issues:</span>
                    <ul className="text-xs mt-1 space-y-1">
                      {health.issues.map((issue, idx) => (
                        <li key={idx} className="text-red-600">
                          • {issue}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Schema Info */}
          {schema && (
            <div>
              <h3 className="text-sm font-semibold mb-3">Schema</h3>
              <div className="text-sm space-y-1">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Tables:</span>
                  <span className="font-medium">{schema.total_tables}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Columns:</span>
                  <span className="font-medium">{schema.total_columns}</span>
                </div>
              </div>
            </div>
          )}

          {/* Capabilities */}
          <div>
            <h3 className="text-sm font-semibold mb-3">Capabilities</h3>
            <div className="grid grid-cols-2 gap-2 text-xs">
              <CapabilityBadge label="CDC" enabled={datasource.capabilities.cdc} />
              <CapabilityBadge label="Batch Read" enabled={datasource.capabilities.batch_read} />
              <CapabilityBadge label="Batch Write" enabled={datasource.capabilities.batch_write} />
              <CapabilityBadge label="Profiling" enabled={datasource.capabilities.profiling} />
              <CapabilityBadge label="Lineage" enabled={datasource.capabilities.lineage_discovery} />
              <CapabilityBadge label="Transactions" enabled={datasource.capabilities.transactions} />
            </div>
          </div>

          {/* Configuration */}
          <div>
            <h3 className="text-sm font-semibold mb-3">Configuration</h3>
            <pre className="text-xs bg-muted p-3 rounded-sm overflow-auto max-h-64">
              {JSON.stringify(datasource.config, null, 2)}
            </pre>
          </div>
        </div>
      </SheetContent>
    </Sheet>
  );
}

function CapabilityBadge({ label, enabled }: { label: string; enabled: boolean }) {
  return (
    <div
      className={`px-2 py-1 rounded-sm border ${
        enabled
          ? 'bg-green-50 border-green-200 text-green-700'
          : 'bg-gray-50 border-gray-200 text-gray-500'
      }`}
    >
      {label}: {enabled ? 'Yes' : 'No'}
    </div>
  );
}
