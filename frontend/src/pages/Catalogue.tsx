/**
 * Data Catalogue Page
 * Browse datasets with quality metrics and start fusion workflows
 */

import { useState, useMemo } from 'react';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Database, Plus, Search, Filter, AlertCircle, Loader2, Sparkles, Link as LinkIcon } from 'lucide-react';
import { DatasetCard } from '@/components/catalogue/DatasetCard';
import { DatasetImportWizard } from '@/components/catalogue/DatasetImportWizard';
import { DiscoveredDatasets } from '@/components/catalogue/DiscoveredDatasets';
import { DatasetStatsDashboard } from '@/components/catalogue/DatasetStatsDashboard';
import { DatasetDetailInspector } from '@/components/catalogue/DatasetDetailInspector';
import { useDatasets } from '@/hooks/useDatasets';
import { useDatasources } from '@/hooks/useDatasources';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { useNavigate } from 'react-router-dom';
import { Dataset } from '@/api/types';

export default function Catalogue() {
  const [searchQuery, setSearchQuery] = useState('');
  const [sourceFilter, setSourceFilter] = useState<string>('all');
  const [qualityFilter, setQualityFilter] = useState<string>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [wizardOpen, setWizardOpen] = useState(false);
  const [quickFilter, setQuickFilter] = useState<'all' | 'low-quality' | 'stale' | 'high-quality'>('all');
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [selectedDataset, setSelectedDataset] = useState<Dataset | null>(null);
  const [initialDatasourceId, setInitialDatasourceId] = useState<string | undefined>();
  const [initialTableName, setInitialTableName] = useState<string | undefined>();
  const navigate = useNavigate();

  const handleOpenInspector = (dataset: Dataset) => {
    setSelectedDataset(dataset);
    setInspectorOpen(true);
  };

  const handleImport = (datasourceId?: string, tableName?: string) => {
    setInitialDatasourceId(datasourceId);
    setInitialTableName(tableName);
    setWizardOpen(true);
  };

  const handleWizardClose = (open: boolean) => {
    setWizardOpen(open);
    if (!open) {
      // Clear initial values when wizard closes
      setInitialDatasourceId(undefined);
      setInitialTableName(undefined);
    }
  };

  // Fetch datasets and datasources
  const { data, isLoading, error } = useDatasets();
  const { data: datasources } = useDatasources();

  // Get unique sources for filter
  const uniqueSources = useMemo(() => {
    if (!data?.datasets) return [];
    const sources = data.datasets
      .map((d) => d.source_name || d.source)
      .filter((s): s is string => Boolean(s));
    return Array.from(new Set(sources));
  }, [data]);

  // Handle quick filter changes
  const handleQuickFilter = (filter: 'all' | 'low-quality' | 'stale' | 'high-quality') => {
    setQuickFilter(filter);

    // Apply corresponding filters
    switch (filter) {
      case 'low-quality':
        setQualityFilter('low');
        setStatusFilter('all');
        break;
      case 'stale':
        setStatusFilter('stale');
        setQualityFilter('all');
        break;
      case 'high-quality':
        setQualityFilter('high');
        setStatusFilter('all');
        break;
      case 'all':
      default:
        setQualityFilter('all');
        setStatusFilter('all');
        break;
    }
  };

  // Filter datasets
  const filteredDatasets = useMemo(() => {
    if (!data?.datasets) return [];

    return data.datasets.filter((dataset) => {
      // Search filter
      const matchesSearch =
        !searchQuery ||
        dataset.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        dataset.description?.toLowerCase().includes(searchQuery.toLowerCase()) ||
        dataset.source_name?.toLowerCase().includes(searchQuery.toLowerCase());

      // Source filter
      const matchesSource =
        sourceFilter === 'all' ||
        dataset.source === sourceFilter ||
        dataset.source_name === sourceFilter;

      // Quality filter
      const qualityScore = dataset.quality_score || 0;
      const matchesQuality =
        qualityFilter === 'all' ||
        (qualityFilter === 'high' && qualityScore >= 80) ||
        (qualityFilter === 'medium' && qualityScore >= 60 && qualityScore < 80) ||
        (qualityFilter === 'low' && qualityScore < 60);

      // Status filter
      const matchesStatus = statusFilter === 'all' || dataset.status === statusFilter;

      return matchesSearch && matchesSource && matchesQuality && matchesStatus;
    });
  }, [data, searchQuery, sourceFilter, qualityFilter, statusFilter]);

  // Calculate summary stats
  const stats = useMemo(() => {
    if (!filteredDatasets.length) return null;

    const totalEntities = filteredDatasets.reduce(
      (sum, d) => sum + (d.entity_count || d.record_count),
      0
    );
    const avgQuality =
      filteredDatasets.reduce((sum, d) => sum + (d.quality_score || 0), 0) /
      filteredDatasets.length;

    return {
      count: filteredDatasets.length,
      entities: totalEntities,
      avgQuality: Math.round(avgQuality),
    };
  }, [filteredDatasets]);

  return (
    <div className="container mx-auto px-6 py-8">
      {/* Header */}
      <div className="mb-6">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-3xl font-bold flex items-center gap-2">
              <Database className="h-8 w-8 text-primary" />
              Data Catalogue
            </h1>
            {stats && (
              <p className="text-muted-foreground mt-1">
                {stats.count} dataset{stats.count !== 1 ? 's' : ''} •{' '}
                {stats.entities >= 1_000_000
                  ? `${(stats.entities / 1_000_000).toFixed(1)}M`
                  : stats.entities >= 1_000
                  ? `${(stats.entities / 1_000).toFixed(1)}K`
                  : stats.entities}{' '}
                entities • {stats.avgQuality}% avg quality
              </p>
            )}
          </div>
          <Button onClick={() => handleImport()}>
            <Plus className="h-4 w-4 mr-2" />
            Import Dataset
          </Button>
        </div>
      </div>

      {/* Statistics Dashboard - Only show when we have datasets */}
      {data?.datasets && data.datasets.length > 0 && !isLoading && (
        <DatasetStatsDashboard
          datasets={data.datasets}
          onQuickFilter={handleQuickFilter}
          activeFilter={quickFilter}
        />
      )}

      {/* Search and Filters */}
      <div className="mb-6 space-y-3">
        <div className="flex flex-col sm:flex-row gap-3">
          {/* Search */}
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search datasets..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
            />
          </div>

          {/* Filters */}
          <div className="flex gap-2 items-center">
            <Filter className="h-4 w-4 text-muted-foreground hidden sm:block" />

            {/* Source Filter */}
            {uniqueSources.length > 0 && (
              <Select value={sourceFilter} onValueChange={setSourceFilter}>
                <SelectTrigger className="w-[160px] h-9">
                  <SelectValue placeholder="Source" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Sources</SelectItem>
                  {uniqueSources.map((source) => (
                    <SelectItem key={source} value={source}>
                      {source}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}

            {/* Quality Filter */}
            <Select value={qualityFilter} onValueChange={setQualityFilter}>
              <SelectTrigger className="w-[140px] h-9">
                <SelectValue placeholder="Quality" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Quality</SelectItem>
                <SelectItem value="high">High (≥80%)</SelectItem>
                <SelectItem value="medium">Medium (60-79%)</SelectItem>
                <SelectItem value="low">Low (&lt;60%)</SelectItem>
              </SelectContent>
            </Select>

            {/* Status Filter */}
            <Select value={statusFilter} onValueChange={setStatusFilter}>
              <SelectTrigger className="w-[120px] h-9">
                <SelectValue placeholder="Status" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All Status</SelectItem>
                <SelectItem value="active">Active</SelectItem>
                <SelectItem value="stale">Stale</SelectItem>
                <SelectItem value="error">Error</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>

        {/* Active Filters Summary */}
        {(searchQuery || sourceFilter !== 'all' || qualityFilter !== 'all' || statusFilter !== 'all') && (
          <div className="text-sm text-muted-foreground">
            Showing {filteredDatasets.length} of {data?.total || 0} datasets
            {searchQuery && ` matching "${searchQuery}"`}
          </div>
        )}
      </div>

      {/* Content */}
      {isLoading ? (
        <div className="flex items-center justify-center py-16">
          <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
          <span className="ml-3 text-muted-foreground">Loading datasets...</span>
        </div>
      ) : error ? (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            Failed to load datasets. {error instanceof Error ? error.message : 'Unknown error'}
          </AlertDescription>
        </Alert>
      ) : filteredDatasets.length === 0 ? (
        <div className="space-y-6">
          {/* Enhanced Empty State */}
          {!datasources || datasources.length === 0 ? (
            // No datasources connected
            <div className="text-center py-16">
              <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-primary/10 text-primary mb-4">
                <LinkIcon className="h-8 w-8" />
              </div>
              <h3 className="text-xl font-semibold mb-2">No Data Sources Connected</h3>
              <p className="text-muted-foreground mb-6 max-w-md mx-auto">
                Connect a data source to start importing datasets into your catalogue.
              </p>
              <Button size="lg" onClick={() => navigate('/datasources')}>
                <Database className="h-5 w-5 mr-2" />
                Connect Datasource
              </Button>
            </div>
          ) : data?.datasets?.length === 0 ? (
            // Datasources connected but no datasets
            <div>
              {/* Discovered Datasets Section */}
              <DiscoveredDatasets onImport={handleImport} />

              {/* Empty State with Call to Action */}
              <div className="text-center py-12 mt-6">
                <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-gradient-to-br from-primary/20 to-primary/5 text-primary mb-4">
                  <Sparkles className="h-8 w-8" />
                </div>
                <h3 className="text-xl font-semibold mb-2">Ready to Import Datasets</h3>
                <p className="text-muted-foreground mb-6 max-w-md mx-auto">
                  You have {datasources.length} datasource{datasources.length !== 1 ? 's' : ''} connected.
                  Start importing datasets to build your catalogue.
                </p>
                <div className="flex gap-3 justify-center">
                  <Button size="lg" onClick={() => handleImport()}>
                    <Plus className="h-5 w-5 mr-2" />
                    Import Datasets
                  </Button>
                  <Button size="lg" variant="outline" onClick={() => navigate('/datasources')}>
                    <Database className="h-5 w-5 mr-2" />
                    View Data Sources
                  </Button>
                </div>
              </div>
            </div>
          ) : (
            // Filtered view is empty
            <div className="text-center py-16">
              <Search className="h-16 w-16 text-muted-foreground mx-auto mb-4 opacity-50" />
              <h3 className="text-lg font-semibold mb-2">No Datasets Match Your Filters</h3>
              <p className="text-muted-foreground mb-4">
                Try adjusting your search or filter criteria.
              </p>
              <Button variant="outline" onClick={() => {
                setSearchQuery('');
                setSourceFilter('all');
                setQualityFilter('all');
                setStatusFilter('all');
              }}>
                Clear Filters
              </Button>
            </div>
          )}
        </div>
      ) : (
        <div>
          {/* Show discovered datasets at the top if we have datasets */}
          {data?.datasets && data.datasets.length > 0 && data.datasets.length < 5 && (
            <div className="mb-6">
              <DiscoveredDatasets onImport={handleImport} />
            </div>
          )}

          {/* Dataset Cards Grid */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {filteredDatasets.map((dataset) => (
              <DatasetCard
                key={dataset.id}
                dataset={dataset}
                onOpenInspector={() => handleOpenInspector(dataset)}
              />
            ))}
          </div>
        </div>
      )}

      {/* Dataset Import Wizard */}
      <DatasetImportWizard
        open={wizardOpen}
        onOpenChange={handleWizardClose}
        initialDatasourceId={initialDatasourceId}
        initialTableName={initialTableName}
      />

      {/* Dataset Detail Inspector */}
      <DatasetDetailInspector
        dataset={selectedDataset}
        open={inspectorOpen}
        onOpenChange={setInspectorOpen}
      />
    </div>
  );
}
