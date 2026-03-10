/**
 * Cluster Skeleton Loading States
 *
 * Professional skeleton loaders that match final content structure
 * Maintains layout to prevent content shift, with smooth pulsing animations
 */

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';

// Base skeleton pulse animation classes
const skeletonBase = 'animate-pulse bg-neutral-200 dark:bg-neutral-800 rounded';

/**
 * Cluster Health Status Skeleton
 * Matches the main health card structure
 */
export function ClusterHealthSkeleton() {
  return (
    <Card className="glass-morphism border-border">
      <CardContent className="p-6">
        <div className="flex items-start justify-between mb-3">
          <div className="flex items-center gap-3">
            {/* Icon placeholder */}
            <div className={cn(skeletonBase, 'h-10 w-10 rounded-lg')} />
            <div className="space-y-2">
              {/* Title placeholder */}
              <div className={cn(skeletonBase, 'h-5 w-24')} />
              {/* Description placeholder */}
              <div className={cn(skeletonBase, 'h-4 w-48')} />
            </div>
          </div>
          {/* Badge placeholder */}
          <div className={cn(skeletonBase, 'h-6 w-20 rounded-full')} />
        </div>
        {/* Timestamp placeholder */}
        <div className={cn(skeletonBase, 'h-3 w-40 mt-2')} />
      </CardContent>
    </Card>
  );
}

/**
 * Performance Metrics Skeleton
 * Two-column grid matching the metrics cards
 */
export function MetricsSkeleton() {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* Performance Metrics Card */}
      <Card className="glass-morphism border-border">
        <CardHeader className="pb-3">
          <div className="flex items-center gap-2">
            <div className={cn(skeletonBase, 'h-4 w-4')} />
            <div className={cn(skeletonBase, 'h-4 w-36')} />
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i}>
              <div className="flex items-center justify-between">
                <div className={cn(skeletonBase, 'h-4 w-24')} />
                <div className={cn(skeletonBase, 'h-6 w-16')} />
              </div>
              {i < 3 && <Separator className="bg-border my-3" />}
            </div>
          ))}
        </CardContent>
      </Card>

      {/* Data Overview Card */}
      <Card className="glass-morphism border-border">
        <CardHeader className="pb-3">
          <div className="flex items-center gap-2">
            <div className={cn(skeletonBase, 'h-4 w-4')} />
            <div className={cn(skeletonBase, 'h-4 w-28')} />
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i}>
              <div className="flex items-center justify-between">
                <div className={cn(skeletonBase, 'h-4 w-28')} />
                <div className={cn(skeletonBase, 'h-6 w-20')} />
              </div>
              {i < 3 && <Separator className="bg-border my-3" />}
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

/**
 * Shard Health Summary Skeleton
 * List of shard items
 */
export function ShardListSkeleton() {
  return (
    <Card className="glass-morphism border-border">
      <CardHeader>
        <div className="flex items-center justify-between">
          <div className={cn(skeletonBase, 'h-4 w-36')} />
          <div className={cn(skeletonBase, 'h-8 w-32 rounded-md')} />
        </div>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {[1, 2, 3, 4, 5].map((i) => (
            <div
              key={i}
              className="flex items-center justify-between p-2 rounded border border-border"
            >
              <div className="flex items-center gap-3">
                <div className={cn(skeletonBase, 'h-4 w-16')} />
                <div className={cn(skeletonBase, 'h-5 w-16 rounded-full')} />
              </div>
              <div className="flex items-center gap-4">
                <div className={cn(skeletonBase, 'h-3 w-20')} />
                <div className={cn(skeletonBase, 'h-3 w-16')} />
              </div>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

/**
 * Configuration Summary Skeleton
 * Config settings list
 */
export function ConfigSkeleton() {
  return (
    <Card className="glass-morphism border-border">
      <CardHeader>
        <div className={cn(skeletonBase, 'h-4 w-40 mb-2')} />
        <div className={cn(skeletonBase, 'h-3 w-32')} />
      </CardHeader>
      <CardContent className="space-y-3">
        {[1, 2, 3, 4].map((i) => (
          <div key={i}>
            <div className="flex items-center justify-between">
              <div className={cn(skeletonBase, 'h-4 w-32')} />
              <div className={cn(skeletonBase, 'h-5 w-24 rounded-full')} />
            </div>
            {i < 4 && <Separator className="bg-border my-3" />}
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

/**
 * Compact skeleton for individual metric rows
 * Useful for partial loading states
 */
export function MetricRowSkeleton() {
  return (
    <div className="flex items-center justify-between">
      <div className={cn(skeletonBase, 'h-4 w-24')} />
      <div className={cn(skeletonBase, 'h-6 w-16')} />
    </div>
  );
}

/**
 * Compact skeleton for shard items
 */
export function ShardItemSkeleton() {
  return (
    <div className="flex items-center justify-between p-2 rounded border border-border">
      <div className="flex items-center gap-3">
        <div className={cn(skeletonBase, 'h-4 w-16')} />
        <div className={cn(skeletonBase, 'h-5 w-16 rounded-full')} />
      </div>
      <div className="flex items-center gap-4">
        <div className={cn(skeletonBase, 'h-3 w-20')} />
        <div className={cn(skeletonBase, 'h-3 w-16')} />
      </div>
    </div>
  );
}
