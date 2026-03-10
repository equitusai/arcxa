/**
 * Dataset Detail Page
 * Detailed view of a single dataset with quality metrics, fusion operations, and workflows
 */

import React from 'react';
import { useParams, Link, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Separator } from '@/components/ui/separator';
import { Badge } from '@/components/ui/badge';
import {
  ArrowLeft,
  Database,
  FileText,
  Zap,
  Clock,
  Activity,
  TrendingUp,
  AlertCircle,
  CheckCircle,
  Combine,
  Workflow as WorkflowIcon,
} from 'lucide-react';
import { useDataset, useDatasetStats } from '@/hooks/useDatasets';
import { QualityBadge } from '@/components/dataset/QualityBadge';
import { Breadcrumbs } from '@/components/Breadcrumbs';

export function DatasetDetail() {
  const { datasetId } = useParams<{ datasetId: string }>();
  const navigate = useNavigate();

  // Fetch dataset and stats
  const { data: dataset, isLoading: isLoadingDataset, error: datasetError } = useDataset(datasetId);
  const { data: stats, isLoading: isLoadingStats } = useDatasetStats(datasetId);

  // Format helpers
  const formatSize = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
  };

  const formatNumber = (num: number): string => {
    return num.toLocaleString();
  };

  const formatDateTime = (isoString?: string): string => {
    if (!isoString) return 'Unknown';
    return new Date(isoString).toLocaleString();
  };

  const formatRelativeTime = (isoString?: string): string => {
    if (!isoString) return 'Unknown';

    const date = new Date(isoString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins} min ago`;
    if (diffHours < 24) return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
    if (diffDays < 7) return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;

    return date.toLocaleDateString();
  };

  if (datasetError) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" className="gap-2" onClick={() => navigate('/catalogue')}>
          <ArrowLeft className="h-4 w-4" />
          Back to Catalogue
        </Button>
        <Alert variant="destructive">
          <AlertDescription>
            Failed to load dataset. Dataset may not exist or the backend may be unavailable.
          </AlertDescription>
        </Alert>
      </div>
    );
  }

  if (isLoadingDataset) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-8 w-64" />
        <Skeleton className="h-32 w-full" />
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-32" />
          ))}
        </div>
      </div>
    );
  }

  if (!dataset) {
    return (
      <div className="space-y-4">
        <Button variant="ghost" className="gap-2" onClick={() => navigate('/catalogue')}>
          <ArrowLeft className="h-4 w-4" />
          Back to Catalogue
        </Button>
        <Alert>
          <AlertDescription>Dataset not found.</AlertDescription>
        </Alert>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Breadcrumbs */}
      <Breadcrumbs
        items={[
          { label: 'Data Management', href: '/catalogue' },
          { label: 'Data Catalogue', href: '/catalogue' },
          { label: dataset.name },
        ]}
      />

      {/* Back Button */}
      <motion.div
        initial={{ opacity: 0, x: -8 }}
        animate={{ opacity: 1, x: 0 }}
        transition={{ duration: 0.15 }}
      >
        <Button variant="ghost" className="gap-2" onClick={() => navigate('/catalogue')}>
          <ArrowLeft className="h-4 w-4" />
          Back to Catalogue
        </Button>
      </motion.div>

      {/* Dataset Header */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.05 }}
      >
        <Card className="glass-morphism border-border">
          <CardHeader>
            <div className="flex items-start justify-between gap-4">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-3 mb-2">
                  <CardTitle className="text-2xl">{dataset.name}</CardTitle>
                  {dataset.quality_score !== undefined && (
                    <QualityBadge score={dataset.quality_score} />
                  )}
                </div>
                {dataset.description && (
                  <CardDescription className="text-base">{dataset.description}</CardDescription>
                )}
                <div className="flex items-center gap-4 mt-3 text-sm text-muted-foreground">
                  {dataset.source_name && (
                    <div className="flex items-center gap-1.5">
                      <Database className="h-4 w-4" />
                      <span>{dataset.source_name}</span>
                    </div>
                  )}
                  <div className="flex items-center gap-1.5">
                    <Clock className="h-4 w-4" />
                    <span>Updated {formatRelativeTime(dataset.last_updated || dataset.updated_at)}</span>
                  </div>
                </div>
              </div>

              <div className="flex gap-2">
                <Button variant="outline" className="gap-2" asChild>
                  <Link to="/entities">
                    <FileText className="h-4 w-4" />
                    View Entities
                  </Link>
                </Button>
                <Button className="gap-2" asChild>
                  <Link to="/fusion-new">
                    <Combine className="h-4 w-4" />
                    Start Fusion
                  </Link>
                </Button>
              </div>
            </div>
          </CardHeader>
        </Card>
      </motion.div>

      {/* Stats Overview */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.1 }}
        className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4"
      >
        {/* Entity Count */}
        <Card className="glass-morphism border-border hover:border-border-emphasis transition-colors">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-bold text-muted-foreground uppercase tracking-wide">
              Entities
            </CardTitle>
            <div className="p-2 rounded-sm bg-entity/10 text-entity">
              <FileText className="h-5 w-5" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-foreground">
              {formatNumber(dataset.entity_count || dataset.record_count)}
            </div>
            <p className="text-xs text-muted-foreground mt-1">Total records</p>
          </CardContent>
        </Card>

        {/* Size */}
        <Card className="glass-morphism border-border hover:border-border-emphasis transition-colors">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-bold text-muted-foreground uppercase tracking-wide">
              Size
            </CardTitle>
            <div className="p-2 rounded-sm bg-primary/10 text-primary">
              <Database className="h-5 w-5" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-foreground">
              {formatSize(dataset.size_bytes || 0)}
            </div>
            <p className="text-xs text-muted-foreground mt-1">Storage used</p>
          </CardContent>
        </Card>

        {/* Quality Score */}
        <Card className="glass-morphism border-border hover:border-border-emphasis transition-colors">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-bold text-muted-foreground uppercase tracking-wide">
              Quality
            </CardTitle>
            <div className="p-2 rounded-sm bg-success/10 text-success">
              <Activity className="h-5 w-5" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-foreground">
              {dataset.quality_score ?? '—'}%
            </div>
            <p className="text-xs text-muted-foreground mt-1">Overall score</p>
          </CardContent>
        </Card>

        {/* Fusion Candidates */}
        <Card className="glass-morphism border-border hover:border-border-emphasis transition-colors">
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-xs font-bold text-muted-foreground uppercase tracking-wide">
              Fusion
            </CardTitle>
            <div className="p-2 rounded-sm bg-warning/10 text-warning">
              <Zap className="h-5 w-5" />
            </div>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-foreground">
              {formatNumber(dataset.fusion_candidates || 0)}
            </div>
            <p className="text-xs text-muted-foreground mt-1">Pending candidates</p>
          </CardContent>
        </Card>
      </motion.div>

      {/* Quality Breakdown & Operations */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {/* Quality Breakdown */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.15 }}
        >
          <Card className="glass-morphism border-border h-full">
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                <TrendingUp className="h-5 w-5 text-success" />
                Quality Metrics
              </CardTitle>
              <CardDescription>Data quality breakdown by dimension</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {dataset.quality_breakdown ? (
                <>
                  {Object.entries(dataset.quality_breakdown).map(([key, value]) => {
                    const percentage = value as number;
                    const variant =
                      percentage >= 80 ? 'success' : percentage >= 60 ? 'warning' : 'destructive';

                    return (
                      <div key={key}>
                        <div className="flex items-center justify-between mb-1.5">
                          <span className="text-sm font-medium text-foreground capitalize">
                            {key}
                          </span>
                          <span className="text-sm font-semibold text-foreground">{percentage}%</span>
                        </div>
                        <div className="h-2 rounded-full bg-background-tertiary overflow-hidden">
                          <div
                            className={`h-full rounded-full transition-all ${
                              variant === 'success'
                                ? 'bg-success'
                                : variant === 'warning'
                                ? 'bg-warning'
                                : 'bg-destructive'
                            }`}
                            style={{ width: `${percentage}%` }}
                          />
                        </div>
                      </div>
                    );
                  })}
                </>
              ) : (
                <div className="text-sm text-muted-foreground text-center py-4">
                  No quality metrics available
                </div>
              )}
            </CardContent>
          </Card>
        </motion.div>

        {/* Fusion & Workflows */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.2 }}
          className="space-y-4"
        >
          {/* Fusion Operations */}
          <Card className="glass-morphism border-border">
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                <Combine className="h-5 w-5 text-primary" />
                Fusion Operations
              </CardTitle>
              <CardDescription>Entity fusion activity</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {isLoadingStats ? (
                <div className="space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-3/4" />
                </div>
              ) : stats ? (
                <>
                  <div className="flex justify-between items-center">
                    <span className="text-sm text-muted-foreground">Committed</span>
                    <span className="text-sm font-semibold text-foreground">
                      {formatNumber(stats.fusion_operations.total_committed)}
                    </span>
                  </div>
                  <Separator />
                  <div className="flex justify-between items-center">
                    <span className="text-sm text-muted-foreground">Pending</span>
                    <span className="text-sm font-semibold text-warning">
                      {formatNumber(stats.fusion_operations.pending_candidates)}
                    </span>
                  </div>
                  <Separator />
                  <div className="flex justify-between items-center">
                    <span className="text-sm text-muted-foreground">Last Fusion</span>
                    <span className="text-sm text-muted-foreground">
                      {formatRelativeTime(stats.fusion_operations.last_fusion_at)}
                    </span>
                  </div>
                </>
              ) : (
                <div className="text-sm text-muted-foreground text-center py-2">
                  No fusion data available
                </div>
              )}
            </CardContent>
          </Card>

          {/* Workflows */}
          <Card className="glass-morphism border-border">
            <CardHeader>
              <CardTitle className="text-base flex items-center gap-2">
                <WorkflowIcon className="h-5 w-5 text-accent" />
                Workflows
              </CardTitle>
              <CardDescription>Automated workflow activity</CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {isLoadingStats ? (
                <div className="space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-3/4" />
                </div>
              ) : stats ? (
                <>
                  <div className="flex justify-between items-center">
                    <span className="text-sm text-muted-foreground">Active</span>
                    <span className="text-sm font-semibold text-success">
                      {formatNumber(stats.workflows.active_count)}
                    </span>
                  </div>
                  <Separator />
                  <div className="flex justify-between items-center">
                    <span className="text-sm text-muted-foreground">Total Executions</span>
                    <span className="text-sm font-semibold text-foreground">
                      {formatNumber(stats.workflows.total_executions)}
                    </span>
                  </div>
                  <Separator />
                  <div className="flex justify-between items-center">
                    <span className="text-sm text-muted-foreground">Last Execution</span>
                    <span className="text-sm text-muted-foreground">
                      {formatRelativeTime(stats.workflows.last_execution_at)}
                    </span>
                  </div>
                </>
              ) : (
                <div className="text-sm text-muted-foreground text-center py-2">
                  No workflow data available
                </div>
              )}
            </CardContent>
          </Card>
        </motion.div>
      </div>
    </div>
  );
}
