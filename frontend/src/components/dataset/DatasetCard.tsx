/**
 * Dataset Card Component
 * Displays dataset overview information in grid view
 */

import React from 'react';
import { Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { QualityBadge } from './QualityBadge';
import { Database, FileText, Zap, Clock } from 'lucide-react';
import { Dataset } from '@/api/types';

interface DatasetCardProps {
  dataset: Dataset;
  delay?: number;
}

export function DatasetCard({ dataset, delay = 0 }: DatasetCardProps) {
  // Format file size
  const formatSize = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
  };

  // Format number with commas
  const formatNumber = (num: number): string => {
    return num.toLocaleString();
  };

  // Format relative time
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

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2, delay }}
    >
      <Link to={`/catalogue/${dataset.id}`}>
        <Card className="h-full hover:border-primary/50 hover:shadow-sm transition-all duration-200 cursor-pointer group">
          <CardHeader className="pb-3">
            <div className="flex items-start justify-between gap-2 mb-2">
              <CardTitle className="text-base group-hover:text-primary transition-colors">
                {dataset.name}
              </CardTitle>
              {dataset.quality_score !== undefined && (
                <QualityBadge score={dataset.quality_score} showLabel={false} />
              )}
            </div>
            {dataset.description && (
              <CardDescription className="line-clamp-2">
                {dataset.description}
              </CardDescription>
            )}
          </CardHeader>

          <CardContent className="space-y-3">
            {/* Primary Stats */}
            <div className="grid grid-cols-2 gap-3">
              <div className="flex items-center gap-2">
                <FileText className="h-4 w-4 text-muted-foreground" />
                <div>
                  <div className="text-xs text-muted-foreground">Entities</div>
                  <div className="text-sm font-semibold text-foreground">
                    {formatNumber(dataset.entity_count || dataset.record_count)}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-2">
                <Database className="h-4 w-4 text-muted-foreground" />
                <div>
                  <div className="text-xs text-muted-foreground">Size</div>
                  <div className="text-sm font-semibold text-foreground">
                    {formatSize(dataset.size_bytes || 0)}
                  </div>
                </div>
              </div>
            </div>

            {/* Secondary Info */}
            <div className="pt-3 border-t border-border space-y-2">
              {dataset.source_name && (
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Database className="h-3 w-3" />
                  <span className="truncate">{dataset.source_name}</span>
                </div>
              )}

              {dataset.fusion_candidates !== undefined && dataset.fusion_candidates > 0 && (
                <div className="flex items-center gap-2 text-xs">
                  <Zap className="h-3 w-3 text-warning" />
                  <span className="text-warning font-semibold">
                    {formatNumber(dataset.fusion_candidates)} fusion {dataset.fusion_candidates === 1 ? 'candidate' : 'candidates'}
                  </span>
                </div>
              )}

              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Clock className="h-3 w-3" />
                <span>Updated {formatRelativeTime(dataset.last_updated || dataset.updated_at)}</span>
              </div>
            </div>
          </CardContent>
        </Card>
      </Link>
    </motion.div>
  );
}
