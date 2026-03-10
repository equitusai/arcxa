/**
 * File Inspector Component
 * Right-side slide-out panel with comprehensive file information
 */

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  FileText,
  Download,
  Trash2,
  Tag as TagIcon,
  Calendar,
  User,
  FolderOpen,
  Eye,
  Activity,
  GitBranch,
  Database,
  ExternalLink,
  ArrowRight,
  Hash,
  HardDrive,
  Clock,
  CheckCircle,
  Link2,
} from 'lucide-react';
import { getFileIcon, formatFileSize } from '@/api/fileLibrary';
import { useFile, useFileLineage, useFileUsageStats } from '@/hooks/useFileLibrary';
import { RegisterDatasourceDialog } from './RegisterDatasourceDialog';
import { cn } from '@/lib/utils';

interface FileInspectorProps {
  fileId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onDownload?: (fileId: string) => void;
  onDelete?: (fileId: string) => void;
}

export function FileInspector({
  fileId,
  open,
  onOpenChange,
  onDownload,
  onDelete,
}: FileInspectorProps) {
  const [activeTab, setActiveTab] = useState('details');
  const [showRegisterDialog, setShowRegisterDialog] = useState(false);
  const navigate = useNavigate();

  const { data: file, isLoading: fileLoading } = useFile(fileId || undefined, !!fileId);
  const { data: lineageData } = useFileLineage(fileId || undefined);
  const { data: usageStats } = useFileUsageStats(fileId || undefined);

  const handleRegisterSuccess = (datasourceId: string, datasetId?: string) => {
    // Close both dialogs
    setShowRegisterDialog(false);
    onOpenChange(false);

    // Navigation will be handled by RegisterDatasourceDialog
  };

  if (!fileId || !file) return null;

  const uploadedDate = new Date(String(file.uploaded_at || Date.now()));
  const lastAccessedDate = file.last_accessed_at ? new Date(String(file.last_accessed_at)) : null;
  const icon = getFileIcon(String(file.mime_type || 'application/octet-stream'));

  const formatRelativeTime = (date: Date) => {
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffHours < 1) return 'Just now';
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  };

  const getMimeTypeLabel = (mimeType: string): string => {
    const typeMap: Record<string, string> = {
      'text/csv': 'CSV',
      'text/tab-separated-values': 'TSV',
      'application/vnd.ms-excel': 'Excel',
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': 'Excel',
      'application/json': 'JSON',
      'application/xml': 'XML',
      'text/xml': 'XML',
      'application/pdf': 'PDF',
    };
    return typeMap[mimeType] || mimeType.split('/')[1]?.toUpperCase() || 'File';
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full sm:max-w-2xl overflow-y-auto">
        <SheetHeader>
          <div className="flex items-start gap-3">
            <div className="text-4xl mt-1">{icon}</div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-xl truncate" title={String(file.original_filename || 'Unknown')}>
                {String(file.original_filename || 'Unknown')}
              </SheetTitle>
              <div className="flex items-center gap-2 mt-1 flex-wrap text-sm text-muted-foreground">
                <Badge variant="outline" className="text-xs font-mono">
                  {getMimeTypeLabel(String(file.mime_type || 'application/octet-stream'))}
                </Badge>
                <span>•</span>
                <span>{formatFileSize(Number(file.size_bytes) || 0)}</span>
                {file.folder_id && (
                  <>
                    <span>•</span>
                    <span className="flex items-center gap-1">
                      <FolderOpen className="h-3 w-3" />
                      <span className="text-xs">In folder</span>
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>

          {/* Quick Actions */}
          <div className="flex flex-col gap-2 mt-4">
            {/* Register as Datasource (if not already registered) */}
            {file.registration_status !== 'registered' && (
              <Button
                variant="default"
                size="sm"
                onClick={() => setShowRegisterDialog(true)}
                className="w-full bg-primary hover:bg-primary/90"
              >
                <Database className="h-3.5 w-3.5 mr-1.5" />
                Register as Datasource
              </Button>
            )}

            {/* Already Registered Badge */}
            {file.registration_status === 'registered' && file.datasource_id && (
              <div className="flex items-center justify-center gap-2 p-2 bg-green-50 border border-green-200 rounded-md">
                <CheckCircle className="h-4 w-4 text-green-600" />
                <span className="text-sm font-medium text-green-900">
                  Registered as datasource
                </span>
              </div>
            )}

            {/* Download & Delete Row */}
            <div className="flex gap-2">
              {onDownload && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => onDownload(String(file.file_id || (file as any).id || ''))}
                  className="flex-1"
                >
                  <Download className="h-3.5 w-3.5 mr-1.5" />
                  Download
                </Button>
              )}
              {onDelete && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => onDelete(String(file.file_id || (file as any).id || ''))}
                  className="flex-1 text-destructive hover:text-destructive"
                >
                  <Trash2 className="h-3.5 w-3.5 mr-1.5" />
                  Delete
                </Button>
              )}
            </div>
          </div>
        </SheetHeader>

        <Tabs value={activeTab} onValueChange={setActiveTab} className="mt-6">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="details" className="text-xs">
              <FileText className="h-3.5 w-3.5 mr-1" />
              Details
            </TabsTrigger>
            <TabsTrigger value="lineage" className="text-xs">
              <GitBranch className="h-3.5 w-3.5 mr-1" />
              Lineage
            </TabsTrigger>
            <TabsTrigger value="usage" className="text-xs">
              <Activity className="h-3.5 w-3.5 mr-1" />
              Usage
            </TabsTrigger>
          </TabsList>

          {/* Details Tab */}
          <TabsContent value="details" className="space-y-4 mt-4">
            {/* File Information */}
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">File Information</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">Filename</div>
                    <div className="text-sm font-mono truncate" title={String(file.filename || 'Unknown')}>
                      {String(file.filename || 'Unknown')}
                    </div>
                  </div>
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">Size</div>
                    <div className="text-sm font-medium">{formatFileSize(Number(file.size_bytes) || 0)}</div>
                  </div>
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">MIME Type</div>
                    <div className="text-sm font-mono text-xs">{String(file.mime_type || 'application/octet-stream')}</div>
                  </div>
                  <div>
                    <div className="text-xs text-muted-foreground mb-1">Access Count</div>
                    <div className="text-sm font-medium flex items-center gap-1">
                      <Eye className="h-3.5 w-3.5" />
                      {Number(file.access_count) || 0}
                    </div>
                  </div>
                </div>

                <div className="pt-2 border-t">
                  <div className="text-xs text-muted-foreground mb-1">Checksum (SHA-256)</div>
                  <div className="text-xs font-mono bg-muted px-2 py-1.5 rounded break-all">
                    {String(file.checksum_sha256 || 'N/A')}
                  </div>
                </div>
              </CardContent>
            </Card>

            {/* Upload & Access Information */}
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Timeline</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex items-start gap-3">
                  <Calendar className="h-4 w-4 text-muted-foreground mt-0.5" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Uploaded</div>
                    <div className="text-sm font-medium">
                      {uploadedDate.toLocaleString()}
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5">
                      {formatRelativeTime(uploadedDate)}
                    </div>
                  </div>
                </div>

                <div className="flex items-start gap-3">
                  <User className="h-4 w-4 text-muted-foreground mt-0.5" />
                  <div className="flex-1">
                    <div className="text-xs text-muted-foreground">Uploaded by</div>
                    <div className="text-sm font-medium">{String(file.uploaded_by || 'Unknown')}</div>
                  </div>
                </div>

                {lastAccessedDate && (
                  <div className="flex items-start gap-3">
                    <Clock className="h-4 w-4 text-muted-foreground mt-0.5" />
                    <div className="flex-1">
                      <div className="text-xs text-muted-foreground">Last accessed</div>
                      <div className="text-sm font-medium">
                        {lastAccessedDate.toLocaleString()}
                      </div>
                      <div className="text-xs text-muted-foreground mt-0.5">
                        {formatRelativeTime(lastAccessedDate)}
                      </div>
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>

            {/* Tags */}
            {file.tags && Array.isArray(file.tags) && file.tags.length > 0 && (
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Tags</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-wrap gap-1.5">
                    {file.tags.map((tag) => (
                      <Badge
                        key={typeof tag === 'string' ? tag : String(tag)}
                        variant="secondary"
                        className="text-xs"
                      >
                        <TagIcon className="h-3 w-3 mr-1" />
                        {typeof tag === 'string' ? tag : String(tag)}
                      </Badge>
                    ))}
                  </div>
                </CardContent>
              </Card>
            )}

            {/* Custom Metadata */}
            {file.custom_metadata && Object.keys(file.custom_metadata).length > 0 && (
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Custom Metadata</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    {Object.entries(file.custom_metadata).map(([key, value]) => (
                      <div key={key} className="flex items-start gap-2 text-sm">
                        <span className="font-medium text-muted-foreground min-w-[100px]">
                          {key}:
                        </span>
                        <span className="flex-1 break-words">
                          {typeof value === 'object' ? JSON.stringify(value) : String(value)}
                        </span>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>
            )}

            {/* Ontology Mappings */}
            {file.ontology_mappings && file.ontology_mappings.length > 0 && (
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm flex items-center gap-2">
                    <Link2 className="h-4 w-4 text-blue-600" />
                    Ontology Mappings
                    <Badge variant="outline" className="ml-auto text-xs">
                      {file.ontology_mappings.length} field{file.ontology_mappings.length !== 1 ? 's' : ''} mapped
                    </Badge>
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    {file.ontology_mappings.map((mapping, idx) => (
                      <div key={idx} className="border rounded-lg p-3 bg-muted/30">
                        <div className="flex items-start justify-between gap-2 mb-2">
                          <div className="flex-1">
                            <div className="text-sm font-mono font-medium text-foreground">
                              {mapping.field_name}
                            </div>
                            <div className="text-xs text-muted-foreground mt-0.5">
                              Field name
                            </div>
                          </div>
                          <ArrowRight className="h-4 w-4 text-muted-foreground mt-1 flex-shrink-0" />
                          <div className="flex-1">
                            <div className="text-sm font-medium text-blue-600">
                              {mapping.concept_label}
                            </div>
                            <div className="text-xs text-muted-foreground mt-0.5">
                              Ontology concept
                            </div>
                          </div>
                        </div>

                        <div className="space-y-1.5 pt-2 border-t">
                          <div className="flex items-center justify-between text-xs">
                            <span className="text-muted-foreground">Ontology:</span>
                            <span className="font-mono text-foreground">{mapping.ontology_id}</span>
                          </div>
                          <div className="flex items-center justify-between text-xs">
                            <span className="text-muted-foreground">Confidence:</span>
                            <Badge
                              variant={mapping.confidence >= 0.8 ? 'default' : 'secondary'}
                              className="text-xs h-5"
                            >
                              {(mapping.confidence * 100).toFixed(0)}%
                            </Badge>
                          </div>
                          <div className="flex items-center justify-between text-xs">
                            <span className="text-muted-foreground">Method:</span>
                            <span className="text-foreground">{mapping.method}</span>
                          </div>
                          <div className="text-xs text-muted-foreground truncate mt-1" title={mapping.concept_uri}>
                            URI: {mapping.concept_uri}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>
            )}
          </TabsContent>

          {/* Lineage Tab */}
          <TabsContent value="lineage" className="mt-4">
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Data Lineage</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                {lineageData ? (
                  <>
                    {/* Upstream Sources */}
                    {lineageData.upstream && lineageData.upstream.length > 0 && (
                      <div>
                        <div className="text-xs font-medium text-muted-foreground mb-2">
                          Upstream Sources
                        </div>
                        <div className="space-y-2">
                          {lineageData.upstream.map((source: any, idx: number) => (
                            <div key={idx} className="flex items-center gap-2 p-2 border rounded-lg">
                              <Database className="h-4 w-4 text-muted-foreground" />
                              <div className="flex-1">
                                <div className="text-sm font-medium">{source.name || source.id}</div>
                                <div className="text-xs text-muted-foreground">{source.type}</div>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Current File */}
                    <div className="flex items-center justify-center py-2">
                      {lineageData.upstream?.length > 0 && (
                        <ArrowRight className="h-4 w-4 text-muted-foreground" />
                      )}
                      <div className="mx-3 px-4 py-2 bg-primary/10 border border-primary rounded-lg">
                        <div className="text-sm font-medium flex items-center gap-2">
                          {icon}
                          {String(file.original_filename || 'Unknown')}
                        </div>
                      </div>
                      {lineageData.downstream?.length > 0 && (
                        <ArrowRight className="h-4 w-4 text-muted-foreground" />
                      )}
                    </div>

                    {/* Downstream Consumers */}
                    {lineageData.downstream && lineageData.downstream.length > 0 && (
                      <div>
                        <div className="text-xs font-medium text-muted-foreground mb-2">
                          Downstream Consumers
                        </div>
                        <div className="space-y-2">
                          {lineageData.downstream.map((consumer: any, idx: number) => (
                            <div key={idx} className="flex items-center gap-2 p-2 border rounded-lg">
                              <Database className="h-4 w-4 text-muted-foreground" />
                              <div className="flex-1">
                                <div className="text-sm font-medium">{consumer.name || consumer.id}</div>
                                <div className="text-xs text-muted-foreground">{consumer.type}</div>
                              </div>
                              <Button variant="ghost" size="sm">
                                <ExternalLink className="h-3.5 w-3.5" />
                              </Button>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}

                    {!lineageData.upstream?.length && !lineageData.downstream?.length && (
                      <div className="text-center py-8 text-sm text-muted-foreground">
                        <GitBranch className="h-8 w-8 mx-auto mb-2 opacity-50" />
                        No lineage data available for this file
                      </div>
                    )}
                  </>
                ) : (
                  <div className="text-center py-8 text-sm text-muted-foreground">
                    <GitBranch className="h-8 w-8 mx-auto mb-2 opacity-50" />
                    Loading lineage data...
                  </div>
                )}
              </CardContent>
            </Card>
          </TabsContent>

          {/* Usage Tab */}
          <TabsContent value="usage" className="mt-4 space-y-4">
            {/* Access Statistics */}
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm">Access Statistics</CardTitle>
              </CardHeader>
              <CardContent>
                {usageStats ? (
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <div className="text-xs text-muted-foreground mb-1">Times Used</div>
                      <div className="text-2xl font-bold">{usageStats.times_used || 0}</div>
                    </div>
                    <div>
                      <div className="text-xs text-muted-foreground mb-1">Workflows Using</div>
                      <div className="text-2xl font-bold text-primary">
                        {usageStats.workflows_count || 0}
                      </div>
                    </div>
                    <div>
                      <div className="text-xs text-muted-foreground mb-1">Last 30 Days</div>
                      <div className="text-2xl font-bold">{usageStats.access_count_30d || 0}</div>
                    </div>
                    <div>
                      <div className="text-xs text-muted-foreground mb-1">Last Accessed</div>
                      <div className="text-sm font-medium">
                        {usageStats.last_accessed ? new Date(usageStats.last_accessed).toLocaleDateString() : 'Never'}
                      </div>
                    </div>
                  </div>
                ) : (
                  <div className="text-center py-8 text-sm text-muted-foreground">
                    Loading usage statistics...
                  </div>
                )}
              </CardContent>
            </Card>

            {/* Top Users */}
            {usageStats?.top_users && usageStats.top_users.length > 0 && (
              <Card>
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Top Users</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-2">
                    {usageStats.top_users.map((userStat: any, idx: number) => (
                      <div key={idx} className="flex items-center justify-between p-2 border rounded-lg">
                        <div className="flex items-center gap-2">
                          <User className="h-4 w-4 text-muted-foreground" />
                          <span className="text-sm font-medium">{userStat.user}</span>
                        </div>
                        <Badge variant="secondary" className="text-xs">
                          {userStat.count} accesses
                        </Badge>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>
            )}
          </TabsContent>
        </Tabs>
      </SheetContent>

      {/* Register Datasource Dialog */}
      <RegisterDatasourceDialog
        file={file}
        open={showRegisterDialog}
        onOpenChange={setShowRegisterDialog}
        onSuccess={handleRegisterSuccess}
      />
    </Sheet>
  );
}
