/**
 * Dataset Card Component
 * Displays dataset information with quality metrics and actions
 */

import { useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Database,
  ExternalLink,
  Eye,
  Layers,
  MoreVertical,
  CheckCircle,
  XCircle,
  AlertCircle,
  ChevronDown,
  ChevronUp,
  Download,
  Copy,
  Archive,
  RefreshCw,
  ArrowRight,
  Tag
} from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { Dataset } from '@/api/types';
import {
  useProfileDataset,
  useExportDatasetMetadata,
  useCloneDataset,
  useRefreshDatasetSchema,
  useArchiveDataset,
} from '@/hooks/useDatasets';

interface DatasetCardProps {
  dataset: Dataset;
  onOpenInspector?: () => void;
}

export function DatasetCard({ dataset, onOpenInspector }: DatasetCardProps) {
  const navigate = useNavigate();
  const [schemaExpanded, setSchemaExpanded] = useState(false);

  // Initialize mutation hooks
  const profileDataset = useProfileDataset();
  const exportMetadata = useExportDatasetMetadata();
  const cloneDataset = useCloneDataset();
  const refreshSchema = useRefreshDatasetSchema();
  const archiveDataset = useArchiveDataset();

  // Mock data for lineage and health (replace with real API data later)
  const healthStatus =
    dataset.status === 'active'
      ? 'healthy'
      : dataset.status === 'error'
      ? 'error'
      : dataset.status === 'stale'
      ? 'warning'
      : 'unknown';
  const hasLineage = dataset.source_datasource_id !== undefined;
  const mockTags = dataset.tags || ['Production', 'PII'];
  const mockUpstream = dataset.source_datasource_id ? 1 : 0;
  const mockDownstream = Math.floor(Math.random() * 3); // Mock value
  const datasetOriginLabel =
    dataset.source_name ||
    dataset.source ||
    (dataset.asset_kind === 'source_asset' ? 'Source asset' : 'Materialized dataset');

  // Mock schema columns (replace with real data from dataset.schema)
  const mockSchemaColumns = dataset.schema?.fields || [
    { name: 'id', type: 'integer', nullable: false },
    { name: 'name', type: 'string', nullable: false },
    { name: 'email', type: 'string', nullable: true },
    { name: 'created_at', type: 'timestamp', nullable: false },
    { name: 'updated_at', type: 'timestamp', nullable: true },
  ].slice(0, 5);

  // Format numbers
  const formatNumber = (num: number) => {
    if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
    if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
    return num.toString();
  };

  // Format date
  const formatRelativeTime = (dateStr: string | undefined) => {
    if (!dateStr) return 'Unknown';
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins} minute${diffMins !== 1 ? 's' : ''} ago`;
    if (diffHours < 24) return `${diffHours} hour${diffHours !== 1 ? 's' : ''} ago`;
    return `${diffDays} day${diffDays !== 1 ? 's' : ''} ago`;
  };

  // Get quality score color
  const getQualityColor = (score: number | undefined) => {
    if (!score) return 'text-muted-foreground';
    if (score >= 90) return 'text-green-600';
    if (score >= 70) return 'text-yellow-600';
    return 'text-red-600';
  };

  // Get status badge variant
  const getStatusBadgeVariant = (status: string | undefined): 'default' | 'secondary' | 'destructive' => {
    if (!status) return 'secondary';
    if (status === 'active') return 'default';
    if (status === 'stale') return 'secondary';
    return 'destructive';
  };

  const qualityScore = dataset.quality_score || 0;
  const qualityBreakdown = dataset.quality_breakdown;

  const handleStartFusion = () => {
    navigate(`/fusion-new?dataset=${dataset.id}`);
  };

  const handleViewEntities = () => {
    navigate(`/entities?source=${dataset.source}`);
  };

  const handleViewSchema = () => {
    if (onOpenInspector) {
      onOpenInspector();
    } else {
      console.log('View schema for dataset:', dataset.id);
    }
  };

  // Quick action handlers
  const handleProfileData = () => {
    profileDataset.mutate(dataset.id);
  };

  const handleExportMetadata = () => {
    exportMetadata.mutate({ datasetId: dataset.id, format: 'json' });
  };

  const handleCloneDataset = () => {
    const newName = prompt(`Enter a name for the cloned dataset:`, `${dataset.name} (Copy)`);
    if (newName && newName.trim()) {
      cloneDataset.mutate({ datasetId: dataset.id, newName: newName.trim() });
    }
  };

  const handleArchiveDataset = () => {
    if (confirm(`Are you sure you want to archive "${dataset.name}"? This action can be undone.`)) {
      archiveDataset.mutate(dataset.id);
    }
  };

  const handleRefreshSchema = () => {
    refreshSchema.mutate(dataset.id);
  };

  // Get health icon and color
  const getHealthIcon = () => {
    switch (healthStatus) {
      case 'healthy':
        return <CheckCircle className="h-4 w-4 text-green-600" />;
      case 'error':
        return <XCircle className="h-4 w-4 text-red-600" />;
      case 'warning':
        return <AlertCircle className="h-4 w-4 text-yellow-600" />;
      default:
        return <AlertCircle className="h-4 w-4 text-muted-foreground" />;
    }
  };

  const getHealthText = () => {
    switch (healthStatus) {
      case 'healthy':
        return 'Connected';
      case 'error':
        return 'Connection Error';
      case 'warning':
        return 'Stale';
      default:
        return 'No health signal';
    }
  };

  return (
    <Card className="w-full hover:shadow-md transition-shadow">
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="flex items-start gap-2 flex-1">
            <Database className="h-5 w-5 text-primary mt-1" />
            <div className="flex-1">
              <CardTitle className="text-lg">{dataset.name}</CardTitle>
              <CardDescription className="text-sm mt-1">
                <span className="font-medium">{datasetOriginLabel}</span>
                {dataset.status && (
                  <>
                    {' • '}
                    <Badge variant={getStatusBadgeVariant(dataset.status)} className="text-xs">
                      {dataset.status}
                    </Badge>
                  </>
                )}
                {dataset.dataset_type && (
                  <>
                    {' • '}
                    <Badge variant="outline" className="text-xs">
                      {dataset.dataset_type}
                    </Badge>
                  </>
                )}
                {dataset.last_updated && (
                  <>
                    {' • '}
                    <span className="text-muted-foreground">
                      Updated {formatRelativeTime(dataset.last_updated)}
                    </span>
                  </>
                )}
              </CardDescription>
              {/* Health Status */}
              <div className="flex items-center gap-1 mt-2 text-xs">
                {getHealthIcon()}
                <span className="text-muted-foreground">{getHealthText()}</span>
              </div>
            </div>
          </div>
          <div className="flex gap-1">
            <Button onClick={handleStartFusion} size="sm">
              <Layers className="h-3.5 w-3.5 mr-1.5" />
              Start Fusion
            </Button>
            {/* Quick Actions Menu */}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm" className="h-8 w-8 p-0">
                  <MoreVertical className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={handleProfileData}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  Profile Data
                </DropdownMenuItem>
                <DropdownMenuItem onClick={handleExportMetadata}>
                  <Download className="h-4 w-4 mr-2" />
                  Export Metadata
                </DropdownMenuItem>
                <DropdownMenuItem onClick={handleCloneDataset}>
                  <Copy className="h-4 w-4 mr-2" />
                  Clone Dataset
                </DropdownMenuItem>
                <DropdownMenuItem onClick={handleRefreshSchema}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  Refresh Schema
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={handleArchiveDataset} className="text-destructive">
                  <Archive className="h-4 w-4 mr-2" />
                  Archive
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Key Metrics */}
        <div className="grid grid-cols-3 gap-4 text-sm">
          <div>
            <div className="text-muted-foreground text-xs mb-1">Entities</div>
            <div className="font-semibold text-lg">
              {formatNumber(dataset.entity_count || dataset.record_count)}
            </div>
          </div>
          <div>
            <div className="text-muted-foreground text-xs mb-1">Quality Score</div>
            <div className={`font-semibold text-lg ${getQualityColor(qualityScore)}`}>
              {qualityScore}%
              {qualityScore >= 90 && ' ✅'}
            </div>
          </div>
          {dataset.fusion_candidates !== undefined && (
            <div>
              <div className="text-muted-foreground text-xs mb-1">Fusion Candidates</div>
              <div className="font-semibold text-lg text-primary">
                {dataset.fusion_candidates}
              </div>
            </div>
          )}
        </div>

        {/* Lineage Preview */}
        {hasLineage && (mockUpstream > 0 || mockDownstream > 0) && (
          <div className="space-y-2 pt-2 border-t">
            <div className="text-xs font-medium text-muted-foreground">Data Lineage</div>
            <div className="flex items-center gap-2 text-xs">
              {mockUpstream > 0 && (
                <div className="flex items-center gap-1 text-muted-foreground">
                  <span>{mockUpstream} upstream</span>
                </div>
              )}
              {mockUpstream > 0 && mockDownstream > 0 && <ArrowRight className="h-3 w-3" />}
              <div className="flex items-center gap-1 font-medium">
                <Database className="h-3 w-3" />
                <span>{dataset.name}</span>
              </div>
              {mockDownstream > 0 && (
                <>
                  <ArrowRight className="h-3 w-3" />
                  <div className="flex items-center gap-1 text-muted-foreground">
                    <span>{mockDownstream} downstream</span>
                  </div>
                </>
              )}
            </div>
          </div>
        )}

        {/* Quality Breakdown */}
        {qualityBreakdown && (
          <div className="space-y-2 pt-2 border-t">
            <div className="text-xs font-medium text-muted-foreground">Quality Breakdown</div>
            <div className="space-y-2">
              {Object.entries(qualityBreakdown).map(([key, value]) => (
                <div key={key} className="space-y-1">
                  <div className="flex justify-between text-xs">
                    <span className="capitalize">{key}</span>
                    <span className={`font-medium ${getQualityColor(value)}`}>{value}%</span>
                  </div>
                  <Progress value={value} className="h-1.5" />
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Tags */}
        {mockTags.length > 0 && (
          <div className="flex flex-wrap gap-1.5 pt-2 border-t">
            {mockTags.map((tag, index) => (
              <Badge key={index} variant="outline" className="text-xs">
                <Tag className="h-3 w-3 mr-1" />
                {tag}
              </Badge>
            ))}
          </div>
        )}

        {/* Schema Preview */}
        <Collapsible open={schemaExpanded} onOpenChange={setSchemaExpanded} className="pt-2 border-t">
          <CollapsibleTrigger className="flex items-center justify-between w-full text-xs font-medium text-muted-foreground hover:text-foreground transition-colors">
            <span>Schema Preview ({mockSchemaColumns.length} columns)</span>
            {schemaExpanded ? (
              <ChevronUp className="h-4 w-4" />
            ) : (
              <ChevronDown className="h-4 w-4" />
            )}
          </CollapsibleTrigger>
          <CollapsibleContent className="mt-2">
            <div className="space-y-1 bg-muted/30 rounded-md p-2">
              {mockSchemaColumns.map((col, index) => (
                <div key={index} className="flex items-center justify-between text-xs py-1">
                  <span className="font-mono font-medium">{col.name}</span>
                  <div className="flex items-center gap-2">
                    <Badge variant="secondary" className="text-xs h-5">
                      {col.type}
                    </Badge>
                    {col.nullable && (
                      <span className="text-muted-foreground text-xs">nullable</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </CollapsibleContent>
        </Collapsible>

        {/* Action Buttons */}
        <div className="flex gap-2 pt-2 border-t">
          <Button variant="outline" size="sm" onClick={handleViewEntities} className="flex-1">
            <Eye className="h-3.5 w-3.5 mr-1.5" />
            View Entities
          </Button>
          <Button variant="outline" size="sm" onClick={handleViewSchema} className="flex-1">
            <ExternalLink className="h-3.5 w-3.5 mr-1.5" />
            View Schema
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
