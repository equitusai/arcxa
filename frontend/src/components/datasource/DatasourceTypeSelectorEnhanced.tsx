/**
 * Enhanced Datasource Type Selector
 *
 * Enterprise-grade connector selection with:
 * - Visual card grid with connector logos
 * - Multi-axis filtering (search, capabilities, categories)
 * - Comparison mode for side-by-side evaluation
 * - Rich metadata and capability preview
 * - Oracle Redwood + Microsoft Fluent design language
 */

import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
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
  Filter,
  X,
  TrendingUp,
  Award,
  Cloud,
  Box,
  ArrowRight,
  Sparkles,
} from 'lucide-react';
import type { AvailablePlugin, DatasourceType } from '@/api/types';

interface DatasourceTypeSelectorEnhancedProps {
  plugins: AvailablePlugin[];
  selectedPlugin: AvailablePlugin | null;
  onSelectPlugin: (plugin: AvailablePlugin) => void;
  isLoading?: boolean;
}

// Category configuration with Fluent colors
const CATEGORY_CONFIG = {
  Relational: {
    icon: Database,
    label: 'Relational',
    color: '#0078D4',
    bgColor: 'rgba(0, 120, 212, 0.10)',
    borderColor: 'rgba(0, 120, 212, 0.25)',
  },
  Document: {
    icon: FileText,
    label: 'Document',
    color: '#107C10',
    bgColor: 'rgba(16, 124, 16, 0.10)',
    borderColor: 'rgba(16, 124, 16, 0.25)',
  },
  Search: {
    icon: Search,
    label: 'Search',
    color: '#FFB900',
    bgColor: 'rgba(255, 185, 0, 0.10)',
    borderColor: 'rgba(255, 185, 0, 0.25)',
  },
  ObjectStorage: {
    icon: HardDrive,
    label: 'Object Storage',
    color: '#5C2E91',
    bgColor: 'rgba(92, 46, 145, 0.10)',
    borderColor: 'rgba(92, 46, 145, 0.25)',
  },
  Streaming: {
    icon: Radio,
    label: 'Streaming',
    color: '#D13438',
    bgColor: 'rgba(209, 52, 56, 0.10)',
    borderColor: 'rgba(209, 52, 56, 0.25)',
  },
  Graph: {
    icon: Network,
    label: 'Graph',
    color: '#0099BC',
    bgColor: 'rgba(0, 153, 188, 0.10)',
    borderColor: 'rgba(0, 153, 188, 0.25)',
  },
  TimeSeries: {
    icon: Clock,
    label: 'Time-Series',
    color: '#767676',
    bgColor: 'rgba(118, 118, 118, 0.10)',
    borderColor: 'rgba(118, 118, 118, 0.25)',
  },
  Custom: {
    icon: FolderOpen,
    label: 'Custom',
    color: '#7F868F',
    bgColor: 'rgba(127, 134, 147, 0.10)',
    borderColor: 'rgba(127, 134, 147, 0.25)',
  },
};

// Capability icons
const CAPABILITY_ICONS = {
  cdc: { icon: Activity, label: 'CDC', tooltip: 'Change Data Capture' },
  batch_read: { icon: Database, label: 'Batch Read', tooltip: 'Batch Read Operations' },
  batch_write: { icon: Database, label: 'Batch Write', tooltip: 'Batch Write Operations' },
  profiling: { icon: Search, label: 'Profiling', tooltip: 'Data Profiling' },
  lineage_discovery: { icon: GitBranch, label: 'Lineage', tooltip: 'Lineage Discovery' },
  schema_evolution: { icon: Zap, label: 'Evolution', tooltip: 'Schema Evolution' },
  transactions: { icon: Shield, label: 'ACID', tooltip: 'Transactional Support' },
};

// Filter options
type FilterType = 'all' | 'popular' | 'cloud' | 'enterprise';
type ViewMode = 'grid' | 'list' | 'comparison';

export function DatasourceTypeSelectorEnhanced({
  plugins,
  selectedPlugin,
  onSelectPlugin,
  isLoading = false,
}: DatasourceTypeSelectorEnhancedProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [activeFilter, setActiveFilter] = useState<FilterType>('all');
  const [selectedCapabilities, setSelectedCapabilities] = useState<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<ViewMode>('grid');

  // Mock popularity data (in real app, fetch from backend)
  const popularPlugins = ['PostgreSQL', 'Snowflake', 'Oracle'];
  const enterprisePlugins = ['Oracle', 'IBM DB2', 'SAP HANA'];
  const cloudPlugins = ['Snowflake', 'S3 Parquet'];

  // Filter plugins
  const filteredPlugins = useMemo(() => {
    let filtered = plugins;

    // Quick filter
    if (activeFilter === 'popular') {
      filtered = filtered.filter((p) => popularPlugins.includes(p.name));
    } else if (activeFilter === 'cloud') {
      filtered = filtered.filter((p) => cloudPlugins.includes(p.name));
    } else if (activeFilter === 'enterprise') {
      filtered = filtered.filter((p) => enterprisePlugins.includes(p.name));
    }

    // Search filter
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      filtered = filtered.filter(
        (p) =>
          p.name.toLowerCase().includes(query) ||
          p.description.toLowerCase().includes(query) ||
          (typeof p.datasource_type === 'string' &&
            p.datasource_type.toLowerCase().includes(query))
      );
    }

    // Capability filter
    if (selectedCapabilities.size > 0) {
      filtered = filtered.filter((p) =>
        Array.from(selectedCapabilities).every((cap) => p.capabilities[cap as keyof typeof p.capabilities])
      );
    }

    return filtered;
  }, [plugins, searchQuery, activeFilter, selectedCapabilities]);

  // Get category for plugin
  const getPluginCategory = (plugin: AvailablePlugin) => {
    const type = typeof plugin.datasource_type === 'string' ? plugin.datasource_type : 'Custom';
    return CATEGORY_CONFIG[type as keyof typeof CATEGORY_CONFIG] || CATEGORY_CONFIG.Custom;
  };

  // Count active capabilities
  const countCapabilities = (plugin: AvailablePlugin) => {
    return Object.values(plugin.capabilities).filter(Boolean).length;
  };

  // Toggle capability filter
  const toggleCapabilityFilter = (capability: string) => {
    const newSet = new Set(selectedCapabilities);
    if (newSet.has(capability)) {
      newSet.delete(capability);
    } else {
      newSet.add(capability);
    }
    setSelectedCapabilities(newSet);
  };

  // Get plugin tags
  const getPluginTags = (plugin: AvailablePlugin): Array<{ label: string; icon: any; color: string }> => {
    const tags = [];
    if (popularPlugins.includes(plugin.name)) {
      tags.push({ label: 'Popular', icon: TrendingUp, color: '#0078D4' });
    }
    if (enterprisePlugins.includes(plugin.name)) {
      tags.push({ label: 'Enterprise', icon: Award, color: '#5C2E91' });
    }
    if (cloudPlugins.includes(plugin.name)) {
      tags.push({ label: 'Cloud', icon: Cloud, color: '#0099BC' });
    }
    return tags;
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-24">
        <div className="text-center">
          <Loader2 className="h-10 w-10 animate-spin text-primary mx-auto mb-3" />
          <p className="text-sm text-muted-foreground">Loading connectors...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="space-y-3">
        <div className="flex items-start justify-between">
          <div>
            <h3 className="text-lg font-semibold text-foreground mb-1">
              Select Data Source
            </h3>
            <p className="text-sm text-muted-foreground">
              Choose a connector to integrate your data systems
            </p>
          </div>
          <Badge variant="outline" className="text-xs font-medium px-3 py-1">
            {filteredPlugins.length} of {plugins.length}
          </Badge>
        </div>

        {/* Search & Filters */}
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search by name, type, or capability..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9 h-10"
            />
          </div>
          <Button
            variant="outline"
            size="sm"
            className="h-10"
            onClick={() => {
              setSearchQuery('');
              setActiveFilter('all');
              setSelectedCapabilities(new Set());
            }}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>

        {/* Quick Filters */}
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs font-medium text-muted-foreground">Quick filters:</span>
          {(['all', 'popular', 'cloud', 'enterprise'] as const).map((filter) => (
            <Button
              key={filter}
              variant={activeFilter === filter ? 'default' : 'outline'}
              size="sm"
              className="h-7 text-xs"
              onClick={() => setActiveFilter(filter)}
            >
              {filter === 'all' && 'All'}
              {filter === 'popular' && (
                <>
                  <TrendingUp className="h-3 w-3 mr-1" />
                  Popular
                </>
              )}
              {filter === 'cloud' && (
                <>
                  <Cloud className="h-3 w-3 mr-1" />
                  Cloud
                </>
              )}
              {filter === 'enterprise' && (
                <>
                  <Award className="h-3 w-3 mr-1" />
                  Enterprise
                </>
              )}
            </Button>
          ))}
        </div>

        {/* Capability Filters */}
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs font-medium text-muted-foreground flex items-center gap-1">
            <Filter className="h-3 w-3" />
            Capabilities:
          </span>
          {Object.entries(CAPABILITY_ICONS).map(([key, config]) => (
            <Button
              key={key}
              variant={selectedCapabilities.has(key) ? 'default' : 'ghost'}
              size="sm"
              className="h-7 text-xs"
              onClick={() => toggleCapabilityFilter(key)}
            >
              {React.createElement(config.icon, { className: 'h-3 w-3 mr-1' })}
              {config.label}
            </Button>
          ))}
        </div>
      </div>

      {/* Connector Grid */}
      {filteredPlugins.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-16 text-center">
          <div
            className="p-4 rounded-md mb-4"
            style={{ backgroundColor: 'rgba(127, 134, 147, 0.08)' }}
          >
            <Search className="h-12 w-12 text-muted-foreground" />
          </div>
          <p className="text-sm font-medium text-foreground mb-1">No connectors found</p>
          <p className="text-xs text-muted-foreground max-w-xs">
            Try adjusting your search or filter criteria
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3 max-h-[500px] overflow-y-auto pr-2">
          {filteredPlugins.map((plugin) => {
            const category = getPluginCategory(plugin);
            const isSelected = selectedPlugin?.name === plugin.name;
            const capabilityCount = countCapabilities(plugin);
            const tags = getPluginTags(plugin);
            const Icon = category.icon;

            return (
              <Card
                key={plugin.name}
                className={`
                  cursor-pointer transition-all border-2 relative overflow-hidden
                  hover:border-[var(--border-color)] hover:shadow-sm
                  ${
                    isSelected
                      ? 'border-[var(--border-color)] shadow-md ring-2 ring-[var(--ring-color)]'
                      : 'border-black/8'
                  }
                `}
                style={
                  isSelected
                    ? ({
                        '--border-color': category.borderColor,
                        '--ring-color': category.bgColor,
                      } as React.CSSProperties)
                    : {}
                }
                onClick={() => onSelectPlugin(plugin)}
              >
                {/* Selection indicator */}
                {isSelected && (
                  <div
                    className="absolute top-0 right-0 p-2"
                    style={{ backgroundColor: category.bgColor }}
                  >
                    <CheckCircle2 className="h-5 w-5" style={{ color: category.color }} />
                  </div>
                )}

                <CardHeader className="pb-3">
                  <div className="flex items-start gap-3">
                    {/* Icon */}
                    <div
                      className="p-2.5 rounded-md flex-shrink-0"
                      style={{
                        backgroundColor: category.bgColor,
                        color: category.color,
                      }}
                    >
                      <Icon className="h-6 w-6" />
                    </div>

                    <div className="flex-1 min-w-0">
                      <CardTitle className="text-base font-semibold text-foreground mb-0.5">
                        {plugin.name}
                      </CardTitle>
                      <CardDescription className="text-xs">
                        v{plugin.version}
                      </CardDescription>
                    </div>
                  </div>

                  {/* Tags */}
                  {tags.length > 0 && (
                    <div className="flex items-center gap-1.5 mt-2">
                      {tags.map((tag) => (
                        <Badge
                          key={tag.label}
                          variant="secondary"
                          className="text-xs h-5 px-2 font-normal"
                          style={{
                            backgroundColor: `${tag.color}15`,
                            color: tag.color,
                            borderColor: `${tag.color}30`,
                          }}
                        >
                          {React.createElement(tag.icon, { className: 'h-3 w-3 mr-1' })}
                          {tag.label}
                        </Badge>
                      ))}
                    </div>
                  )}
                </CardHeader>

                <CardContent className="space-y-3">
                  <p className="text-xs text-muted-foreground leading-relaxed line-clamp-2">
                    {plugin.description}
                  </p>

                  {/* Category badge */}
                  <div className="flex items-center justify-between pt-2 border-t border-black/8">
                    <Badge
                      variant="outline"
                      className="text-xs h-6 px-2 font-medium"
                      style={{
                        borderColor: category.borderColor,
                        color: category.color,
                      }}
                    >
                      {category.label}
                    </Badge>
                    <span className="text-xs text-muted-foreground">
                      {capabilityCount} {capabilityCount === 1 ? 'capability' : 'capabilities'}
                    </span>
                  </div>

                  {/* Capabilities (compact) */}
                  {capabilityCount > 0 && (
                    <div className="flex items-center gap-1 flex-wrap">
                      {Object.entries(plugin.capabilities)
                        .filter(([_, enabled]) => enabled)
                        .slice(0, 4)
                        .map(([key]) => {
                          const capConfig = CAPABILITY_ICONS[key as keyof typeof CAPABILITY_ICONS];
                          if (!capConfig) return null;
                          const CapIcon = capConfig.icon;

                          return (
                            <div
                              key={key}
                              className="flex items-center gap-1 px-1.5 py-0.5 rounded bg-neutral-100/80"
                              title={capConfig.tooltip}
                            >
                              <CapIcon className="h-3 w-3 text-neutral-600" />
                              <span className="text-xs text-neutral-700">{capConfig.label}</span>
                            </div>
                          );
                        })}
                      {capabilityCount > 4 && (
                        <span className="text-xs text-muted-foreground">
                          +{capabilityCount - 4} more
                        </span>
                      )}
                    </div>
                  )}
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      {/* Selection Footer */}
      {selectedPlugin && (
        <div
          className="flex items-center justify-between p-3 rounded-md border-2 animate-in fade-in slide-in-from-bottom-2 duration-200"
          style={{
            backgroundColor: getPluginCategory(selectedPlugin).bgColor,
            borderColor: getPluginCategory(selectedPlugin).borderColor,
          }}
        >
          <div className="flex items-center gap-3">
            <CheckCircle2
              className="h-5 w-5"
              style={{ color: getPluginCategory(selectedPlugin).color }}
            />
            <div>
              <p className="text-sm font-semibold text-foreground">
                {selectedPlugin.name} selected
              </p>
              <p className="text-xs text-muted-foreground">
                {typeof selectedPlugin.datasource_type === 'string'
                  ? CATEGORY_CONFIG[
                      selectedPlugin.datasource_type as keyof typeof CATEGORY_CONFIG
                    ]?.label
                  : 'Custom'}{' '}
                • {countCapabilities(selectedPlugin)} capabilities
              </p>
            </div>
          </div>
          <Button
            size="sm"
            style={{
              backgroundColor: getPluginCategory(selectedPlugin).color,
              color: 'white',
            }}
            className="h-8"
          >
            Continue
            <ArrowRight className="h-4 w-4 ml-1" />
          </Button>
        </div>
      )}
    </div>
  );
}
