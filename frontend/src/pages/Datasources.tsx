/**
 * Datasources Page
 * Enterprise data catalog for datasource management
 */

import React, { useState } from 'react';
import { motion } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
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
  Database,
  Plus,
  Search,
  Loader2,
  Eye,
  Trash2,
  Activity,
} from 'lucide-react';
import {
  useDatasources,
  useDeleteDatasource,
  useTestConnection,
  useDatasourceHealth,
  useDatasourceSchema,
  useDatasourceStats,
} from '@/hooks/useDatasources';
import { DatasourceWizardEnhanced as DatasourceWizard } from '@/components/datasource/DatasourceWizardEnhanced';
import type { Datasource, ConnectionStatus } from '@/api/types';

export function Datasources() {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedDatasource, setSelectedDatasource] = useState<Datasource | null>(null);
  const [showDetails, setShowDetails] = useState(false);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [datasourceToDelete, setDatasourceToDelete] = useState<string | null>(null);
  const [showConfigDialog, setShowConfigDialog] = useState(false);

  const { data: datasources, isLoading } = useDatasources();
  const { data: stats } = useDatasourceStats();
  const deleteDatasource = useDeleteDatasource();
  const testConnection = useTestConnection();

  const filteredDatasources = datasources?.filter((ds) =>
    ds.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    ds.plugin_name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const getStatusBadge = (status: ConnectionStatus) => {
    if (status === 'Connected') {
      return <Badge className="bg-green-100 text-green-800 border-green-200">Connected</Badge>;
    } else if (status === 'Connecting') {
      return <Badge className="bg-blue-100 text-blue-800 border-blue-200">Connecting</Badge>;
    } else if (status === 'Disconnected') {
      return <Badge variant="secondary">Disconnected</Badge>;
    } else if (typeof status === 'object' && 'Degraded' in status) {
      return <Badge className="bg-yellow-100 text-yellow-800 border-yellow-200">Degraded</Badge>;
    } else if (typeof status === 'object' && 'Error' in status) {
      return <Badge variant="destructive">Error</Badge>;
    }
    return <Badge variant="secondary">Unknown</Badge>;
  };

  const getTypeBadge = (type: Datasource['metadata']['datasource_type']) => {
    const typeStr = typeof type === 'string' ? type : type?.Custom || 'Unknown';
    const colors: Record<string, string> = {
      Relational: 'bg-blue-50 text-blue-700 border-blue-200',
      Document: 'bg-green-50 text-green-700 border-green-200',
      Search: 'bg-purple-50 text-purple-700 border-purple-200',
      ObjectStorage: 'bg-orange-50 text-orange-700 border-orange-200',
      Streaming: 'bg-red-50 text-red-700 border-red-200',
      Graph: 'bg-pink-50 text-pink-700 border-pink-200',
      TimeSeries: 'bg-indigo-50 text-indigo-700 border-indigo-200',
    };
    return (
      <Badge variant="outline" className={colors[typeStr] || 'bg-gray-50 text-gray-700'}>
        {typeStr}
      </Badge>
    );
  };

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

  return (
    <div className="container mx-auto py-6 space-y-6">
      {/* Page Header */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15 }}
        className="flex items-start justify-between pb-4 border-b-2 border-border"
      >
        <div>
          <h1 className="text-2xl font-semibold text-foreground mb-1">Data Sources</h1>
          <p className="text-sm text-muted-foreground">
            Enterprise data catalog - manage connections to external systems
          </p>
        </div>

        <Button className="gap-2" onClick={() => setShowConfigDialog(true)}>
          <Plus className="h-4 w-4" />
          Add Datasource
        </Button>
      </motion.div>

      {/* Stats Cards */}
      {stats && (
        <motion.div
          initial={{ opacity: 0, y: -8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.05 }}
          className="grid grid-cols-1 md:grid-cols-4 gap-4"
        >
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Total Datasources
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{stats.total_datasources}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Connected
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold text-green-600">{stats.connected}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Disconnected
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold text-gray-500">{stats.disconnected}</div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Errors
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold text-red-600">{stats.errors}</div>
            </CardContent>
          </Card>
        </motion.div>
      )}

      {/* Search */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.1 }}
        className="relative"
      >
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="Search datasources by name or type..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="pl-9"
        />
      </motion.div>

      {/* Datasource List */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.15 }}
        className="space-y-3"
      >
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        ) : filteredDatasources && filteredDatasources.length > 0 ? (
          filteredDatasources.map((datasource, idx) => (
            <motion.div
              key={datasource.id}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ duration: 0.15, delay: 0.2 + idx * 0.02 }}
            >
              <Card className="hover:border-primary transition-colors">
                <CardHeader>
                  <div className="flex items-start justify-between">
                    <div className="flex items-start gap-3">
                      <div className="p-2 bg-primary/10 rounded-sm">
                        <Database className="h-5 w-5 text-primary" />
                      </div>
                      <div>
                        <CardTitle className="text-base mb-1">{datasource.name}</CardTitle>
                        <CardDescription className="flex items-center gap-2 text-xs">
                          <span>{datasource.metadata.name} v{datasource.metadata.version}</span>
                          <span>•</span>
                          <span>{datasource.metadata.author}</span>
                        </CardDescription>
                        <div className="flex items-center gap-2 mt-2">
                          {getStatusBadge(datasource.status)}
                          {getTypeBadge(datasource.metadata.datasource_type)}
                          {!datasource.enabled && (
                            <Badge variant="outline" className="bg-gray-100 text-gray-600">
                              Disabled
                            </Badge>
                          )}
                        </div>
                      </div>
                    </div>

                    <div className="flex items-center gap-1">
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0"
                        title="View details"
                        onClick={() => handleViewDetails(datasource)}
                      >
                        <Eye className="h-4 w-4" />
                      </Button>

                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0"
                        title="Test connection"
                        onClick={() => handleTestConnection(datasource.id)}
                      >
                        <Activity className="h-4 w-4" />
                      </Button>

                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                        title="Delete"
                        onClick={() => handleDelete(datasource.id)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                </CardHeader>

                <CardContent>
                  <p className="text-sm text-muted-foreground">{datasource.metadata.description}</p>

                  <div className="flex items-center gap-4 mt-3 text-xs text-muted-foreground">
                    <div className="flex items-center gap-1">
                      <span className="font-medium">CDC:</span>
                      <span>{datasource.capabilities.cdc ? 'Yes' : 'No'}</span>
                    </div>
                    <div className="flex items-center gap-1">
                      <span className="font-medium">Profiling:</span>
                      <span>{datasource.capabilities.profiling ? 'Yes' : 'No'}</span>
                    </div>
                    <div className="flex items-center gap-1">
                      <span className="font-medium">Lineage:</span>
                      <span>{datasource.capabilities.lineage_discovery ? 'Yes' : 'No'}</span>
                    </div>
                  </div>
                </CardContent>
              </Card>
            </motion.div>
          ))
        ) : (
          <Card>
            <CardContent className="flex flex-col items-center justify-center py-12">
              <Database className="h-12 w-12 text-muted-foreground mb-4" />
              <p className="text-sm text-muted-foreground text-center">
                {searchQuery
                  ? 'No datasources found matching your search'
                  : 'No datasources configured yet'}
              </p>
              {!searchQuery && (
                <Button className="mt-4 gap-2" onClick={() => setShowConfigDialog(true)}>
                  <Plus className="h-4 w-4" />
                  Add Your First Datasource
                </Button>
              )}
            </CardContent>
          </Card>
        )}
      </motion.div>

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
            <AlertDialogTitle>Delete Datasource</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete this datasource? This action cannot be undone and may
              affect workflows that depend on this datasource.
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

      {/* Configuration Wizard Dialog */}
      <DatasourceWizard open={showConfigDialog} onOpenChange={setShowConfigDialog} />
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
              <CapabilityBadge
                label="CDC"
                enabled={datasource.capabilities.cdc}
              />
              <CapabilityBadge
                label="Batch Read"
                enabled={datasource.capabilities.batch_read}
              />
              <CapabilityBadge
                label="Batch Write"
                enabled={datasource.capabilities.batch_write}
              />
              <CapabilityBadge
                label="Profiling"
                enabled={datasource.capabilities.profiling}
              />
              <CapabilityBadge
                label="Lineage"
                enabled={datasource.capabilities.lineage_discovery}
              />
              <CapabilityBadge
                label="Transactions"
                enabled={datasource.capabilities.transactions}
              />
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
