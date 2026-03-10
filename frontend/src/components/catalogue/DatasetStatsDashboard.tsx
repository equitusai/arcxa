/**
 * Dataset Statistics Dashboard
 * Overview cards, trend charts, and quick filters
 */

import { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Database,
  Layers,
  TrendingUp,
  AlertTriangle,
  BarChart3,
  ArrowUp,
  ArrowDown,
  Minus
} from 'lucide-react';
import { Dataset } from '@/api/types';

interface DatasetStatsDashboardProps {
  datasets: Dataset[];
  onQuickFilter: (filter: 'all' | 'low-quality' | 'stale' | 'high-quality') => void;
  activeFilter?: string;
}

export function DatasetStatsDashboard({
  datasets,
  onQuickFilter,
  activeFilter = 'all'
}: DatasetStatsDashboardProps) {
  // Calculate statistics
  const stats = useMemo(() => {
    const total = datasets.length;
    const totalEntities = datasets.reduce((sum, d) => sum + (d.entity_count || d.record_count || 0), 0);
    const avgQuality = total > 0
      ? datasets.reduce((sum, d) => sum + (d.quality_score || 0), 0) / total
      : 0;
    const needingAttention = datasets.filter(d => (d.quality_score || 0) < 60).length;
    const staleDatasets = datasets.filter(d => d.status === 'stale').length;
    const activeDatasets = datasets.filter(d => d.status === 'active').length;

    // Quality distribution
    const high = datasets.filter(d => (d.quality_score || 0) >= 80).length;
    const medium = datasets.filter(d => {
      const score = d.quality_score || 0;
      return score >= 60 && score < 80;
    }).length;
    const low = datasets.filter(d => (d.quality_score || 0) < 60).length;

    // Mock trend data (in production, this would come from backend)
    const previousTotal = Math.max(0, total - Math.floor(Math.random() * 5));
    const previousQuality = avgQuality - (Math.random() * 10 - 5);
    const totalChange = total - previousTotal;
    const qualityChange = avgQuality - previousQuality;

    return {
      total,
      totalEntities,
      avgQuality: Math.round(avgQuality),
      needingAttention,
      staleDatasets,
      activeDatasets,
      qualityDistribution: { high, medium, low },
      trends: {
        totalChange,
        qualityChange
      }
    };
  }, [datasets]);

  const formatNumber = (num: number) => {
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
    return num.toString();
  };

  const getTrendIcon = (change: number) => {
    if (change > 0) return <ArrowUp className="h-3 w-3" />;
    if (change < 0) return <ArrowDown className="h-3 w-3" />;
    return <Minus className="h-3 w-3" />;
  };

  const getTrendColor = (change: number) => {
    if (change > 0) return 'text-green-600';
    if (change < 0) return 'text-red-600';
    return 'text-muted-foreground';
  };

  if (datasets.length === 0) return null;

  return (
    <div className="space-y-6 mb-8">
      {/* Overview Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Total Datasets */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <Database className="h-4 w-4" />
              Total Datasets
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.total}</div>
            <div className={`text-xs flex items-center gap-1 mt-1 ${getTrendColor(stats.trends.totalChange)}`}>
              {getTrendIcon(stats.trends.totalChange)}
              <span>{Math.abs(stats.trends.totalChange)} from last week</span>
            </div>
          </CardContent>
        </Card>

        {/* Total Entities */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <Layers className="h-4 w-4" />
              Total Entities
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{formatNumber(stats.totalEntities)}</div>
            <div className="text-xs text-muted-foreground mt-1">
              Across all datasets
            </div>
          </CardContent>
        </Card>

        {/* Avg Quality Score */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <TrendingUp className="h-4 w-4" />
              Avg Quality
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.avgQuality}%</div>
            <div className={`text-xs flex items-center gap-1 mt-1 ${getTrendColor(stats.trends.qualityChange)}`}>
              {getTrendIcon(stats.trends.qualityChange)}
              <span>{Math.abs(stats.trends.qualityChange).toFixed(1)}% from last week</span>
            </div>
          </CardContent>
        </Card>

        {/* Needs Attention */}
        <Card className={stats.needingAttention > 0 ? 'border-orange-500/50 bg-orange-50/50 dark:bg-orange-950/20' : ''}>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <AlertTriangle className="h-4 w-4" />
              Needs Attention
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.needingAttention}</div>
            <div className="text-xs text-muted-foreground mt-1">
              Quality below 60%
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Quality Distribution Bar Chart */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="text-base flex items-center gap-2">
              <BarChart3 className="h-4 w-4" />
              Quality Distribution
            </CardTitle>
            <div className="flex gap-2 text-xs">
              <div className="flex items-center gap-1">
                <div className="w-3 h-3 rounded-sm bg-green-600" />
                <span className="text-muted-foreground">High (≥80%)</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="w-3 h-3 rounded-sm bg-yellow-600" />
                <span className="text-muted-foreground">Medium (60-79%)</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="w-3 h-3 rounded-sm bg-red-600" />
                <span className="text-muted-foreground">Low (&lt;60%)</span>
              </div>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {/* Stacked Bar */}
            <div className="h-8 flex rounded-lg overflow-hidden bg-muted">
              {stats.qualityDistribution.high > 0 && (
                <div
                  className="bg-green-600 flex items-center justify-center text-white text-xs font-medium"
                  style={{ width: `${(stats.qualityDistribution.high / stats.total) * 100}%` }}
                >
                  {stats.qualityDistribution.high > 0 && stats.qualityDistribution.high}
                </div>
              )}
              {stats.qualityDistribution.medium > 0 && (
                <div
                  className="bg-yellow-600 flex items-center justify-center text-white text-xs font-medium"
                  style={{ width: `${(stats.qualityDistribution.medium / stats.total) * 100}%` }}
                >
                  {stats.qualityDistribution.medium > 0 && stats.qualityDistribution.medium}
                </div>
              )}
              {stats.qualityDistribution.low > 0 && (
                <div
                  className="bg-red-600 flex items-center justify-center text-white text-xs font-medium"
                  style={{ width: `${(stats.qualityDistribution.low / stats.total) * 100}%` }}
                >
                  {stats.qualityDistribution.low > 0 && stats.qualityDistribution.low}
                </div>
              )}
            </div>

            {/* Percentage Labels */}
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>
                {stats.qualityDistribution.high} high quality ({Math.round((stats.qualityDistribution.high / stats.total) * 100)}%)
              </span>
              <span>
                {stats.qualityDistribution.medium} medium ({Math.round((stats.qualityDistribution.medium / stats.total) * 100)}%)
              </span>
              <span>
                {stats.qualityDistribution.low} low ({Math.round((stats.qualityDistribution.low / stats.total) * 100)}%)
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Quick Filters */}
      <div className="flex flex-wrap gap-2 items-center">
        <span className="text-sm font-medium text-muted-foreground">Quick Filters:</span>
        <Button
          variant={activeFilter === 'all' ? 'default' : 'outline'}
          size="sm"
          onClick={() => onQuickFilter('all')}
        >
          All Datasets
          <Badge variant="secondary" className="ml-2">
            {stats.total}
          </Badge>
        </Button>
        {stats.qualityDistribution.high > 0 && (
          <Button
            variant={activeFilter === 'high-quality' ? 'default' : 'outline'}
            size="sm"
            onClick={() => onQuickFilter('high-quality')}
          >
            High Quality
            <Badge variant="secondary" className="ml-2">
              {stats.qualityDistribution.high}
            </Badge>
          </Button>
        )}
        {stats.needingAttention > 0 && (
          <Button
            variant={activeFilter === 'low-quality' ? 'default' : 'outline'}
            size="sm"
            onClick={() => onQuickFilter('low-quality')}
          >
            <AlertTriangle className="h-3 w-3 mr-1" />
            Needs Attention
            <Badge variant="secondary" className="ml-2">
              {stats.needingAttention}
            </Badge>
          </Button>
        )}
        {stats.staleDatasets > 0 && (
          <Button
            variant={activeFilter === 'stale' ? 'default' : 'outline'}
            size="sm"
            onClick={() => onQuickFilter('stale')}
          >
            Stale Datasets
            <Badge variant="secondary" className="ml-2">
              {stats.staleDatasets}
            </Badge>
          </Button>
        )}
      </div>
    </div>
  );
}
