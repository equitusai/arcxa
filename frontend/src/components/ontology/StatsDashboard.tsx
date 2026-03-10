/**
 * Visual Stats Dashboard Component
 * Week 1.3: Beautiful gradient cards showing ontology statistics
 */

import React, { useState } from 'react';
import { Database, Network, Layers, TrendingUp, ChevronDown, ChevronUp } from 'lucide-react';
import { Card } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import type { OntologyTreeResponse } from '@/api/ontology';

interface StatsDashboardProps {
  treeData: OntologyTreeResponse | null;
  ontologyName: string;
}

export function StatsDashboard({ treeData, ontologyName }: StatsDashboardProps) {
  const [propertiesExpanded, setPropertiesExpanded] = useState(false);

  if (!treeData) return null;

  const stats = [
    {
      label: 'Total Classes',
      value: treeData.stats.total_classes,
      icon: Database,
      gradient: 'from-blue-500 to-cyan-500',
      bgGradient: 'from-blue-50 to-cyan-50 dark:from-blue-950/30 dark:to-cyan-950/30',
      borderColor: 'border-blue-200 dark:border-blue-800',
    },
    {
      label: 'Properties',
      value: treeData.stats.total_properties,
      icon: Network,
      gradient: 'from-purple-500 to-pink-500',
      bgGradient: 'from-purple-50 to-pink-50 dark:from-purple-950/30 dark:to-pink-950/30',
      borderColor: 'border-purple-200 dark:border-purple-800',
    },
    {
      label: 'Max Depth',
      value: treeData.stats.max_depth,
      icon: Layers,
      gradient: 'from-green-500 to-emerald-500',
      bgGradient: 'from-green-50 to-emerald-50 dark:from-green-950/30 dark:to-emerald-950/30',
      borderColor: 'border-green-200 dark:border-green-800',
    },
    {
      label: 'Root Classes',
      value: treeData.root_classes.length,
      icon: TrendingUp,
      gradient: 'from-orange-500 to-amber-500',
      bgGradient: 'from-orange-50 to-amber-50 dark:from-orange-950/30 dark:to-amber-950/30',
      borderColor: 'border-orange-200 dark:border-orange-800',
    },
  ];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-1">
          {ontologyName} Overview
        </h3>
        <p className="text-sm text-muted-foreground">
          Namespace: <code className="text-xs font-mono bg-muted px-1.5 py-0.5 rounded">{treeData.namespace}</code>
        </p>
      </div>

      {/* Stats Grid - Smaller cards */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        {stats.map((stat) => {
          const Icon = stat.icon;
          return (
            <Card
              key={stat.label}
              className={`relative overflow-hidden border ${stat.borderColor} bg-gradient-to-br ${stat.bgGradient} transition-all hover:shadow-md hover:scale-102 cursor-pointer`}
            >
              <div className="p-3">
                {/* Icon with gradient */}
                <div className={`inline-flex p-2 rounded-lg bg-gradient-to-br ${stat.gradient} mb-2`}>
                  <Icon className="h-4 w-4 text-white" />
                </div>

                {/* Value */}
                <div className="text-2xl font-bold text-foreground mb-0.5">
                  {stat.value.toLocaleString()}
                </div>

                {/* Label */}
                <div className="text-xs font-medium text-muted-foreground">
                  {stat.label}
                </div>
              </div>

              {/* Decorative gradient overlay */}
              <div className={`absolute top-0 right-0 w-16 h-16 bg-gradient-to-br ${stat.gradient} opacity-5 rounded-full -mr-8 -mt-8`} />
            </Card>
          );
        })}
      </div>

      {/* Property Distribution Preview - Collapsible */}
      {treeData.root_properties.length > 0 && (
        <Card className="border bg-gradient-to-br from-slate-50 to-gray-50 dark:from-slate-950/30 dark:to-gray-950/30 border-slate-200 dark:border-slate-800">
          <div className="p-4">
            <Button
              variant="ghost"
              size="sm"
              className="w-full justify-between h-auto p-0 hover:bg-transparent"
              onClick={() => setPropertiesExpanded(!propertiesExpanded)}
            >
              <h4 className="text-sm font-semibold text-foreground flex items-center gap-2">
                <Network className="h-4 w-4 text-purple-600 dark:text-purple-400" />
                Top Properties
                <span className="text-xs text-muted-foreground font-normal">
                  ({treeData.root_properties.length})
                </span>
              </h4>
              {propertiesExpanded ? (
                <ChevronUp className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              )}
            </Button>

            {propertiesExpanded && (
              <div className="space-y-2 mt-3">
                {treeData.root_properties.slice(0, 10).map((prop) => {
                  // Handle domain which might be string, array, or undefined
                  const domainLabel = prop.domain
                    ? typeof prop.domain === 'string'
                      ? ((prop.domain as string) as string).split('#').pop() || (prop.domain as string).split('/').pop()
                      : Array.isArray(prop.domain)
                      ? ((prop.domain as string[])[0] as string)?.split('#').pop() || ((prop.domain as string[])[0] as string)?.split('/').pop()
                      : ''
                    : '';

                  return (
                    <div
                      key={prop.uri}
                      className="flex items-center justify-between text-xs py-1.5 px-2 rounded bg-white/50 dark:bg-slate-900/50 hover:bg-white dark:hover:bg-slate-900 transition-colors"
                    >
                      <span className="font-mono text-muted-foreground truncate flex-1">
                        {prop.label || prop.uri.split('#').pop() || prop.uri.split('/').pop()}
                      </span>
                      {domainLabel && (
                        <span className="text-xs text-muted-foreground/60 ml-2">
                          → {domainLabel}
                        </span>
                      )}
                    </div>
                  );
                })}
                {treeData.root_properties.length > 10 && (
                  <div className="text-xs text-center text-muted-foreground pt-1">
                    +{treeData.root_properties.length - 10} more properties
                  </div>
                )}
              </div>
            )}
          </div>
        </Card>
      )}
    </div>
  );
}
