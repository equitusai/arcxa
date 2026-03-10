/**
 * Datasource Type Selection Component
 *
 * Enterprise-grade type selector with progressive disclosure:
 * 1. Category-first navigation (Relational, NoSQL, Analytics, etc.)
 * 2. Plugin detail view with capabilities preview
 * 3. Visual hierarchy using Oracle Redwood + Microsoft Fluent design
 */

import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Database,
  Server,
  Search,
  HardDrive,
  Radio,
  Network,
  Clock,
  FolderOpen,
  CheckCircle2,
  Loader2,
  Zap,
  Shield,
  Activity,
  GitBranch,
  FileText,
  ArrowRight,
  ChevronRight,
} from 'lucide-react';
import type { AvailablePlugin, DatasourceType } from '@/api/types';

interface DatasourceTypeSelectorProps {
  plugins: AvailablePlugin[];
  selectedPlugin: AvailablePlugin | null;
  onSelectPlugin: (plugin: AvailablePlugin) => void;
  isLoading?: boolean;
}

// Category definitions with icons, colors, and descriptions
// This is the full catalog - only categories with actual plugins will be shown
const CATEGORY_CONFIG = {
  Relational: {
    icon: Database,
    label: 'Relational Databases',
    color: 'rgb(0, 120, 212)', // Fluent Blue
    bgColor: 'rgba(0, 120, 212, 0.08)',
    borderColor: 'rgba(0, 120, 212, 0.20)',
    description: 'Traditional SQL databases with ACID guarantees',
    examples: 'SQL-based relational systems',
  },
  Document: {
    icon: FileText,
    label: 'Document Stores',
    color: 'rgb(16, 124, 16)', // Success Green
    bgColor: 'rgba(16, 124, 16, 0.08)',
    borderColor: 'rgba(16, 124, 16, 0.20)',
    description: 'Schema-flexible JSON/BSON document databases',
    examples: 'NoSQL document databases',
  },
  Search: {
    icon: Search,
    label: 'Search Engines',
    color: 'rgb(255, 185, 0)', // Warning Amber
    bgColor: 'rgba(255, 185, 0, 0.08)',
    borderColor: 'rgba(255, 185, 0, 0.20)',
    description: 'Full-text search and analytics platforms',
    examples: 'Search and indexing engines',
  },
  ObjectStorage: {
    icon: HardDrive,
    label: 'Object Storage',
    color: 'rgb(92, 46, 145)', // Purple
    bgColor: 'rgba(92, 46, 145, 0.08)',
    borderColor: 'rgba(92, 46, 145, 0.20)',
    description: 'Cloud-native blob and object stores',
    examples: 'Cloud and object storage systems',
  },
  Streaming: {
    icon: Radio,
    label: 'Streaming Platforms',
    color: 'rgb(209, 52, 56)', // Danger Red
    bgColor: 'rgba(209, 52, 56, 0.08)',
    borderColor: 'rgba(209, 52, 56, 0.20)',
    description: 'Real-time event streaming and message queues',
    examples: 'Event streaming platforms',
  },
  Graph: {
    icon: Network,
    label: 'Graph Databases',
    color: 'rgb(0, 153, 188)', // Cyan
    bgColor: 'rgba(0, 153, 188, 0.08)',
    borderColor: 'rgba(0, 153, 188, 0.20)',
    description: 'Relationship-first graph data platforms',
    examples: 'Graph database systems',
  },
  TimeSeries: {
    icon: Clock,
    label: 'Time-Series',
    color: 'rgb(118, 118, 118)', // Neutral Gray
    bgColor: 'rgba(118, 118, 118, 0.08)',
    borderColor: 'rgba(118, 118, 118, 0.20)',
    description: 'Optimized for temporal and metrics data',
    examples: 'Time-series databases',
  },
  Custom: {
    icon: FolderOpen,
    label: 'Custom Plugins',
    color: 'rgb(127, 134, 147)', // Muted
    bgColor: 'rgba(127, 134, 147, 0.08)',
    borderColor: 'rgba(127, 134, 147, 0.20)',
    description: 'User-defined and third-party connectors',
    examples: 'Custom integrations',
  },
};

// Capability icon mapping
const CAPABILITY_ICONS = {
  cdc: { icon: Activity, label: 'Change Data Capture', color: 'text-blue-600' },
  batch_read: { icon: Database, label: 'Batch Read', color: 'text-green-600' },
  batch_write: { icon: Database, label: 'Batch Write', color: 'text-green-600' },
  profiling: { icon: Search, label: 'Data Profiling', color: 'text-purple-600' },
  lineage_discovery: { icon: GitBranch, label: 'Lineage Discovery', color: 'text-orange-600' },
  schema_evolution: { icon: Zap, label: 'Schema Evolution', color: 'text-yellow-600' },
  transactions: { icon: Shield, label: 'Transactions', color: 'text-red-600' },
};

export function DatasourceTypeSelector({
  plugins,
  selectedPlugin,
  onSelectPlugin,
  isLoading = false,
}: DatasourceTypeSelectorProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);
  const [hoveredPlugin, setHoveredPlugin] = useState<string | null>(null);

  // Group plugins by category
  const pluginsByCategory = useMemo(() => {
    const grouped: Record<string, AvailablePlugin[]> = {};

    plugins.forEach((plugin) => {
      const category = typeof plugin.datasource_type === 'string'
        ? plugin.datasource_type
        : 'Custom';

      if (!grouped[category]) {
        grouped[category] = [];
      }
      grouped[category].push(plugin);
    });

    return grouped;
  }, [plugins]);

  // Filter categories and plugins by search
  const filteredCategories = useMemo(() => {
    if (!searchQuery.trim()) return Object.keys(pluginsByCategory);

    const query = searchQuery.toLowerCase();
    return Object.keys(pluginsByCategory).filter((category) => {
      const categoryConfig = CATEGORY_CONFIG[category as keyof typeof CATEGORY_CONFIG];
      const categoryMatches = categoryConfig?.label.toLowerCase().includes(query) ||
                              categoryConfig?.description.toLowerCase().includes(query);

      const hasMatchingPlugins = pluginsByCategory[category].some((plugin) =>
        plugin.name.toLowerCase().includes(query) ||
        plugin.description.toLowerCase().includes(query)
      );

      return categoryMatches || hasMatchingPlugins;
    });
  }, [searchQuery, pluginsByCategory]);

  // Get filtered plugins for selected category
  const filteredPlugins = useMemo(() => {
    if (!selectedCategory) return [];

    const categoryPlugins = pluginsByCategory[selectedCategory] || [];

    if (!searchQuery.trim()) return categoryPlugins;

    const query = searchQuery.toLowerCase();
    return categoryPlugins.filter((plugin) =>
      plugin.name.toLowerCase().includes(query) ||
      plugin.description.toLowerCase().includes(query)
    );
  }, [selectedCategory, pluginsByCategory, searchQuery]);

  // Count active capabilities
  const countCapabilities = (plugin: AvailablePlugin) => {
    return Object.values(plugin.capabilities).filter(Boolean).length;
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Search Header */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-base font-semibold text-neutral-900">Select Datasource Type</h3>
            <p className="text-sm text-neutral-600 mt-0.5">
              Choose a category to explore available connectors
            </p>
          </div>
          <Badge variant="outline" className="text-xs font-normal">
            {plugins.length} {plugins.length === 1 ? 'plugin' : 'plugins'} available
          </Badge>
        </div>

        <Input
          placeholder="Search datasources..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="h-9"
        />
      </div>

      {/* Two-pane layout: Categories | Plugin Details */}
      <div className="grid grid-cols-5 gap-4 min-h-[420px]">
        {/* Left: Category Navigation */}
        <div className="col-span-2 space-y-2 overflow-y-auto max-h-[520px] pr-2">
          {filteredCategories.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <Search className="h-12 w-12 text-muted-foreground/40 mb-3" />
              <p className="text-sm font-medium text-neutral-700">No results found</p>
              <p className="text-xs text-muted-foreground mt-1">
                Try adjusting your search terms
              </p>
            </div>
          ) : (
            filteredCategories.map((category) => {
              const config = CATEGORY_CONFIG[category as keyof typeof CATEGORY_CONFIG];
              if (!config) return null;

              const Icon = config.icon;
              const pluginCount = pluginsByCategory[category]?.length || 0;
              const isSelected = selectedCategory === category;

              return (
                <button
                  key={category}
                  onClick={() => setSelectedCategory(category)}
                  className={`
                    w-full text-left p-3 rounded-md border-2 transition-all
                    ${isSelected
                      ? 'border-[var(--border-color)] bg-[var(--bg-color)] shadow-sm'
                      : 'border-transparent hover:border-black/10 hover:bg-neutral-50'
                    }
                  `}
                  style={isSelected ? {
                    '--border-color': config.borderColor,
                    '--bg-color': config.bgColor,
                  } as React.CSSProperties : {}}
                >
                  <div className="flex items-start gap-3">
                    <div
                      className="p-1.5 rounded-sm flex-shrink-0"
                      style={{
                        backgroundColor: config.bgColor,
                        color: config.color,
                      }}
                    >
                      <Icon className="h-4 w-4" />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center justify-between gap-2">
                        <p className="text-sm font-semibold text-neutral-900 truncate">
                          {config.label}
                        </p>
                        <Badge variant="secondary" className="text-xs flex-shrink-0">
                          {pluginCount}
                        </Badge>
                      </div>
                      <p className="text-xs text-neutral-600 mt-0.5 line-clamp-2">
                        {config.description}
                      </p>
                    </div>
                    {isSelected && (
                      <ChevronRight className="h-4 w-4 text-neutral-400 flex-shrink-0 mt-0.5" />
                    )}
                  </div>
                </button>
              );
            })
          )}
        </div>

        {/* Right: Plugin Details */}
        <div className="col-span-3 border-2 border-black/10 rounded-md bg-neutral-50/50">
          {!selectedCategory ? (
            <div className="flex flex-col items-center justify-center h-full text-center p-6">
              <div
                className="p-4 rounded-md mb-4"
                style={{ backgroundColor: 'rgba(127, 134, 147, 0.08)' }}
              >
                <Database className="h-10 w-10 text-neutral-400" />
              </div>
              <p className="text-sm font-medium text-neutral-700">Select a category</p>
              <p className="text-xs text-muted-foreground mt-1 max-w-xs">
                Choose a datasource category from the left to view available plugins
              </p>
            </div>
          ) : filteredPlugins.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-center p-6">
              <Search className="h-10 w-10 text-muted-foreground/40 mb-3" />
              <p className="text-sm font-medium text-neutral-700">No plugins found</p>
              <p className="text-xs text-muted-foreground mt-1">
                No plugins match your search in this category
              </p>
            </div>
          ) : (
            <div className="overflow-y-auto max-h-[520px] p-3 space-y-2">
              {filteredPlugins.map((plugin) => {
                const categoryConfig = CATEGORY_CONFIG[selectedCategory as keyof typeof CATEGORY_CONFIG];
                const isSelected = selectedPlugin?.name === plugin.name;
                const isHovered = hoveredPlugin === plugin.name;
                const capabilityCount = countCapabilities(plugin);

                return (
                  <Card
                    key={plugin.name}
                    className={`
                      cursor-pointer transition-all border-2
                      ${isSelected
                        ? 'border-[var(--border-color)] bg-white shadow-md ring-2 ring-[var(--ring-color)]'
                        : isHovered
                          ? 'border-black/15 bg-white shadow-sm'
                          : 'border-black/8 hover:border-black/12'
                      }
                    `}
                    style={isSelected ? {
                      '--border-color': categoryConfig.borderColor,
                      '--ring-color': categoryConfig.bgColor,
                    } as React.CSSProperties : {}}
                    onClick={() => onSelectPlugin(plugin)}
                    onMouseEnter={() => setHoveredPlugin(plugin.name)}
                    onMouseLeave={() => setHoveredPlugin(null)}
                  >
                    <CardHeader className="pb-3">
                      <div className="flex items-start justify-between gap-3">
                        <div className="flex items-start gap-3 flex-1 min-w-0">
                          <div
                            className="p-2 rounded-sm flex-shrink-0"
                            style={{
                              backgroundColor: categoryConfig.bgColor,
                              color: categoryConfig.color,
                            }}
                          >
                            {React.createElement(categoryConfig.icon, { className: 'h-5 w-5' })}
                          </div>
                          <div className="flex-1 min-w-0">
                            <CardTitle className="text-base font-semibold text-neutral-900">
                              {plugin.name}
                            </CardTitle>
                            <CardDescription className="text-xs mt-0.5">
                              v{plugin.version} • {categoryConfig.label}
                            </CardDescription>
                          </div>
                        </div>
                        {isSelected && (
                          <CheckCircle2
                            className="h-5 w-5 flex-shrink-0"
                            style={{ color: categoryConfig.color }}
                          />
                        )}
                      </div>
                    </CardHeader>
                    <CardContent className="space-y-3">
                      <p className="text-xs text-neutral-700 leading-relaxed">
                        {plugin.description}
                      </p>

                      {/* Capabilities Grid */}
                      {capabilityCount > 0 && (
                        <div className="pt-2 border-t border-black/8">
                          <p className="text-xs font-medium text-neutral-700 mb-2">
                            Capabilities ({capabilityCount})
                          </p>
                          <div className="grid grid-cols-2 gap-1.5">
                            {Object.entries(plugin.capabilities).map(([key, enabled]) => {
                              if (!enabled) return null;

                              const capConfig = CAPABILITY_ICONS[key as keyof typeof CAPABILITY_ICONS];
                              if (!capConfig) return null;

                              const CapIcon = capConfig.icon;

                              return (
                                <div
                                  key={key}
                                  className="flex items-center gap-1.5 px-2 py-1 rounded bg-neutral-100/80 text-xs"
                                >
                                  <CapIcon className={`h-3 w-3 ${capConfig.color} flex-shrink-0`} />
                                  <span className="text-neutral-700 truncate">
                                    {capConfig.label}
                                  </span>
                                </div>
                              );
                            })}
                          </div>
                        </div>
                      )}
                    </CardContent>
                  </Card>
                );
              })}
            </div>
          )}
        </div>
      </div>

      {/* Selection Summary Footer */}
      {selectedPlugin && (
        <div className="flex items-center justify-between p-3 bg-neutral-50 border-2 border-black/10 rounded-md">
          <div className="flex items-center gap-3">
            <CheckCircle2 className="h-5 w-5 text-green-600" />
            <div>
              <p className="text-sm font-medium text-neutral-900">
                {selectedPlugin.name} selected
              </p>
              <p className="text-xs text-neutral-600">
                {typeof selectedPlugin.datasource_type === 'string'
                  ? CATEGORY_CONFIG[selectedPlugin.datasource_type as keyof typeof CATEGORY_CONFIG]?.label
                  : 'Custom Plugin'}
              </p>
            </div>
          </div>
          <Badge className="text-xs">
            {countCapabilities(selectedPlugin)} capabilities
          </Badge>
        </div>
      )}
    </div>
  );
}
