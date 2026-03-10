/**
 * Data Catalogue Page
 *
 * Unified view of all data sources:
 * - Uploaded files from File Library
 * - External datasource connections
 *
 * This is Phase 1 of the backend architecture integration.
 */

import React, { useState, useMemo } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
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
  Database,
  Search,
  Plus,
  FolderOpen,
  Filter,
  X,
} from 'lucide-react';
import { DataSourceGrid, DataSourceInspector } from '@/components/data-catalogue';
import {
  listUnifiedSources,
  getDataCatalogueStats,
  deleteUnifiedSources,
  downloadUnifiedSource,
  formatSize,
  type DataSourceType,
  type UnifiedDataSource,
} from '@/api/dataCatalogue';
import { toast } from 'sonner';

export function DataCatalogue() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  // State
  const [searchQuery, setSearchQuery] = useState('');
  const [typeFilter, setTypeFilter] = useState<DataSourceType | 'all'>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [showFilters, setShowFilters] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [sourcesToDelete, setSourcesToDelete] = useState<UnifiedDataSource[]>([]);
  const [inspectorSource, setInspectorSource] = useState<UnifiedDataSource | null>(null);

  // Fetch unified sources
  const { data: sourcesData, isLoading, error } = useQuery({
    queryKey: ['unified-sources'],
    queryFn: () => listUnifiedSources(),
  });

  // Fetch stats
  const { data: stats } = useQuery({
    queryKey: ['catalogue-stats'],
    queryFn: () => getDataCatalogueStats(),
  });

  // Apply filters and search client-side
  const filteredSources = useMemo(() => {
    if (!sourcesData) return [];

    let filtered = sourcesData.sources;

    // Type filter
    if (typeFilter !== 'all') {
      filtered = filtered.filter(s => s.type === typeFilter);
    }

    // Status filter
    if (statusFilter !== 'all') {
      filtered = filtered.filter(s => s.status === statusFilter);
    }

    // Search filter
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      filtered = filtered.filter(s =>
        s.name.toLowerCase().includes(query) ||
        s.description?.toLowerCase().includes(query) ||
        s.tags.some(tag => tag.toLowerCase().includes(query))
      );
    }

    return filtered;
  }, [sourcesData, typeFilter, statusFilter, searchQuery]);

  // Handlers
  const handleDeleteRequest = (sources: UnifiedDataSource[]) => {
    setSourcesToDelete(sources);
    setDeleteDialogOpen(true);
  };

  const handleDeleteConfirm = async () => {
    if (sourcesToDelete.length === 0) return;

    const toastId = toast.loading(
      `Deleting ${sourcesToDelete.length} source${sourcesToDelete.length > 1 ? 's' : ''}...`
    );

    try {
      const result = await deleteUnifiedSources(sourcesToDelete);

      if (result.success) {
        toast.success(
          `Successfully deleted ${result.deleted} source${result.deleted > 1 ? 's' : ''}`,
          { id: toastId }
        );
      } else {
        toast.error(
          `Deleted ${result.deleted}, failed ${result.failed}. ${result.errors.join(', ')}`,
          { id: toastId, duration: 5000 }
        );
      }

      // Refresh data
      queryClient.invalidateQueries({ queryKey: ['unified-sources'] });
      queryClient.invalidateQueries({ queryKey: ['catalogue-stats'] });
      queryClient.invalidateQueries({ queryKey: ['file-library-files'] });
      queryClient.invalidateQueries({ queryKey: ['datasources'] });
    } catch (error) {
      toast.error(
        `Delete failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
        { id: toastId }
      );
    } finally {
      setDeleteDialogOpen(false);
      setSourcesToDelete([]);
    }
  };

  const handleView = (source: UnifiedDataSource) => {
    setInspectorSource(source);
  };

  const handleDownload = async (source: UnifiedDataSource) => {
    if (source.type !== 'file') {
      toast.error('Only files can be downloaded');
      return;
    }

    const toastId = toast.loading(`Downloading ${source.name}...`);

    try {
      const blobUrl = await downloadUnifiedSource(source);
      if (blobUrl) {
        // Create temporary download link
        const link = document.createElement('a');
        link.href = blobUrl;
        link.download = source.name;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);

        // Clean up blob URL
        setTimeout(() => URL.revokeObjectURL(blobUrl), 100);

        toast.success(`Downloaded ${source.name}`, { id: toastId });
      } else {
        toast.error('Download failed', { id: toastId });
      }
    } catch (error) {
      toast.error(
        `Download failed: ${error instanceof Error ? error.message : 'Unknown error'}`,
        { id: toastId }
      );
    }
  };

  const clearFilters = () => {
    setSearchQuery('');
    setTypeFilter('all');
    setStatusFilter('all');
  };

  const hasActiveFilters = searchQuery || typeFilter !== 'all' || statusFilter !== 'all';

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
          <h1 className="text-2xl font-semibold text-foreground mb-1 flex items-center gap-2">
            <Database className="h-6 w-6 text-primary" />
            Data Catalogue
          </h1>
          <p className="text-sm text-muted-foreground">
            Unified view of all data sources - uploaded files and external connections
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            className="gap-2"
            onClick={() => navigate('/file-library')}
          >
            <FolderOpen className="h-4 w-4" />
            File Library
          </Button>
          <Button
            className="gap-2"
            onClick={() => navigate('/datasources')}
          >
            <Plus className="h-4 w-4" />
            Add Source
          </Button>
        </div>
      </motion.div>

      {/* Stats Cards */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.05 }}
        className="grid grid-cols-1 md:grid-cols-4 gap-4"
      >
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Total Sources
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold">
              {stats?.total_sources ?? 0}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              Files + Data Sources
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Files
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold">
              {stats?.files ?? 0}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              {stats?.total_size_bytes ? formatSize(stats.total_size_bytes) : 'From File Library'}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Data Sources
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold">
              {stats?.datasources ?? 0}
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              External Connections
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Recent Additions
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold text-green-600">
              {stats?.recent_additions ?? 0}
            </div>
            <p className="text-xs text-muted-foreground mt-1">Last 24 hours</p>
          </CardContent>
        </Card>
      </motion.div>

      {/* Search and Filters */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.1 }}
        className="space-y-3"
      >
        {/* Search Bar */}
        <div className="flex items-center gap-3">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search sources by name, description, or tags..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
            />
          </div>
          <Button
            variant={showFilters ? 'default' : 'outline'}
            size="sm"
            onClick={() => setShowFilters(!showFilters)}
            className="gap-2"
          >
            <Filter className="h-4 w-4" />
            Filters
          </Button>
          {hasActiveFilters && (
            <Button
              variant="ghost"
              size="sm"
              onClick={clearFilters}
              className="gap-2"
            >
              <X className="h-4 w-4" />
              Clear
            </Button>
          )}
        </div>

        {/* Filter Dropdowns */}
        {showFilters && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            className="flex items-center gap-3 p-4 bg-muted/50 rounded-md border"
          >
            <div className="flex items-center gap-2 flex-1">
              <label className="text-sm font-medium text-muted-foreground whitespace-nowrap">
                Type:
              </label>
              <Select value={typeFilter} onValueChange={(val: any) => setTypeFilter(val)}>
                <SelectTrigger className="w-[180px]">
                  <SelectValue placeholder="All types" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Types</SelectItem>
                  <SelectItem value="file">Files Only</SelectItem>
                  <SelectItem value="datasource">Data Sources Only</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-center gap-2 flex-1">
              <label className="text-sm font-medium text-muted-foreground whitespace-nowrap">
                Status:
              </label>
              <Select value={statusFilter} onValueChange={setStatusFilter}>
                <SelectTrigger className="w-[180px]">
                  <SelectValue placeholder="All statuses" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Statuses</SelectItem>
                  <SelectItem value="active">Active</SelectItem>
                  <SelectItem value="registered">Registered</SelectItem>
                  <SelectItem value="unregistered">Unregistered</SelectItem>
                  <SelectItem value="inactive">Inactive</SelectItem>
                  <SelectItem value="error">Error</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="text-sm text-muted-foreground">
              {filteredSources.length} result{filteredSources.length !== 1 ? 's' : ''}
            </div>
          </motion.div>
        )}
      </motion.div>

      {/* Content Area */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.15 }}
      >
        {error ? (
          <Card className="border-destructive">
            <CardContent className="flex flex-col items-center justify-center py-12">
              <p className="text-destructive font-semibold">Error loading data sources</p>
              <p className="text-sm text-muted-foreground mt-2">
                {error instanceof Error ? error.message : 'Unknown error'}
              </p>
            </CardContent>
          </Card>
        ) : (
          <DataSourceGrid
            sources={filteredSources}
            loading={isLoading}
            onDelete={handleDeleteRequest}
            onView={handleView}
            onDownload={handleDownload}
            emptyMessage={
              hasActiveFilters
                ? 'No sources match your filters'
                : 'No data sources yet'
            }
            emptyDescription={
              hasActiveFilters
                ? 'Try adjusting your search or filter criteria'
                : 'Get started by uploading files to the File Library or connecting external datasources.'
            }
          />
        )}
      </motion.div>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm Deletion</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete {sourcesToDelete.length} source
              {sourcesToDelete.length > 1 ? 's' : ''}?
              {sourcesToDelete.length === 1 && (
                <span className="block mt-2 font-medium">
                  "{sourcesToDelete[0]?.name}"
                </span>
              )}
              {sourcesToDelete.length > 1 && (
                <ul className="mt-2 list-disc list-inside max-h-40 overflow-y-auto">
                  {sourcesToDelete.map((s) => (
                    <li key={s.id}>{s.name}</li>
                  ))}
                </ul>
              )}
              <span className="block mt-3 text-destructive">
                This action cannot be undone.
              </span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteConfirm}
              className="bg-destructive hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Source Inspector */}
      <DataSourceInspector
        source={inspectorSource}
        open={!!inspectorSource}
        onClose={() => setInspectorSource(null)}
        onDelete={() => {
          if (inspectorSource) {
            handleDeleteRequest([inspectorSource]);
            setInspectorSource(null);
          }
        }}
        onDownload={() => {
          if (inspectorSource) {
            handleDownload(inspectorSource);
          }
        }}
      />
    </div>
  );
}
