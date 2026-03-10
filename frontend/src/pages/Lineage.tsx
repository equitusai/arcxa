/**
 * Data Lineage Page
 * Advanced lineage visualization with Sankey diagrams, filters, and temporal navigation
 */

import React, { useState, useMemo } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  GitBranch,
  Download,
  Filter as FilterIcon,
  Search,
  Loader2,
  AlertCircle,
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { useRecordLineage } from '@/hooks/useLineage';
import { useLineageGraph, type LineageFilters } from '@/hooks/useLineageGraph';
import { LineageGraph } from '@/components/lineage/LineageGraph';
import { LineageNodeDetails } from '@/components/lineage/LineageNodeDetails';
import { LineageControls } from '@/components/lineage/LineageControls';
import { LineageTimeSlider } from '@/components/lineage/LineageTimeSlider';
import { FieldSelector } from '@/components/lineage/FieldSelector';
import { WhatIfSimulator } from '@/components/lineage/WhatIfSimulator'; // Phase 3
import { LineageMockup } from '@/components/lineage/LineageMockup';
import { EntityTypeahead } from '@/components/lineage/EntityTypeahead';
import type { LineageNode, LineageGraph as LineageGraphType } from '@/hooks/useLineageGraph';
import type { SimulationResult } from '@/components/lineage/WhatIfSimulator'; // Phase 3

export function Lineage() {
  // Search state
  const [recordId, setRecordId] = useState('');
  const [searchRecordId, setSearchRecordId] = useState<string | undefined>(undefined);
  const [selectedEntity, setSelectedEntity] = useState<{ id: string; label: string } | null>(null);

  // UI state
  const [selectedNode, setSelectedNode] = useState<LineageNode | null>(null);
  const [showFilters, setShowFilters] = useState(true);
  const [showFieldSelector, setShowFieldSelector] = useState(false); // Phase 2: Field-level view
  const [showNodeDetails, setShowNodeDetails] = useState(false);
  const [filters, setFilters] = useState<LineageFilters>({});
  const [selectedTime, setSelectedTime] = useState<string | undefined>(undefined);

  // Phase 3: What-If Simulation state
  const [showSimulator, setShowSimulator] = useState(false);
  const [simulationResult, setSimulationResult] = useState<SimulationResult | null>(null);
  const [viewMode, setViewMode] = useState<'current' | 'simulated' | 'diff'>('current');

  // Fetch lineage data
  const { data: lineageData, isLoading, error } = useRecordLineage(searchRecordId);

  // Transform lineage events into graph
  const { graph, filteredGraph, isFiltered, stats } = useLineageGraph({
    events: lineageData?.events || [],
    filters,
    enableFieldIsolation: true,
  });

  // Apply time filter separately to avoid circular dependency
  const timeFilteredGraph = useMemo(() => {
    if (!selectedTime) return filteredGraph;

    const timeRange: [string, string] = [graph.metadata.dateRange.start, selectedTime];
    const filtered = filteredGraph.edges.filter(
      (edge) => edge.timestamp >= timeRange[0] && edge.timestamp <= timeRange[1]
    );
    const nodeIds = new Set<string>();
    filtered.forEach((edge) => {
      nodeIds.add(edge.source);
      nodeIds.add(edge.target);
    });

    return {
      ...filteredGraph,
      nodes: filteredGraph.nodes.filter((node) => nodeIds.has(node.id)),
      edges: filtered,
    };
  }, [filteredGraph, selectedTime, graph.metadata.dateRange.start]);

  // Available filter options
  const availableDatasets = useMemo(
    () => Array.from(graph.metadata.datasets),
    [graph.metadata.datasets]
  );

  const availableModels = useMemo(
    () => Array.from(graph.metadata.models),
    [graph.metadata.models]
  );

  const handleSearch = () => {
    if (recordId.trim()) {
      setSearchRecordId(recordId.trim());
      setSelectedNode(null);
      setFilters({});
      setSelectedTime(undefined);
      setSelectedEntity(null);
    }
  };

  const handleEntitySelect = (entity: { id: string; label: string }) => {
    setSelectedEntity(entity);
    setRecordId(entity.id);
    setSearchRecordId(entity.id);
    setSelectedNode(null);
    setFilters({});
    setSelectedTime(undefined);
  };

  const handleNodeClick = (node: LineageNode) => {
    setSelectedNode(node);
    setShowNodeDetails(true);
  };

  const handleExportGraph = () => {
    // Export graph as JSON
    const dataStr = JSON.stringify(timeFilteredGraph, null, 2);
    const dataBlob = new Blob([dataStr], { type: 'application/json' });
    const url = URL.createObjectURL(dataBlob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `lineage-${searchRecordId}-${new Date().toISOString()}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };

  // Phase 3: Handle simulation changes
  const handleSimulationChange = (result: SimulationResult | null) => {
    setSimulationResult(result);
    if (result) {
      setViewMode('diff'); // Auto-switch to diff view when simulation runs
    } else {
      setViewMode('current'); // Reset to current view when simulation cleared
    }
  };

  // Get related edges for selected node
  const relatedEdges = useMemo(() => {
    if (!selectedNode) return [];
    return timeFilteredGraph.edges.filter(
      (edge) => edge.source === selectedNode.id || edge.target === selectedNode.id
    );
  }, [selectedNode, timeFilteredGraph]);

  return (
    <div className="space-y-4 pb-8 h-[calc(100vh-8rem)]">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15 }}
        className="space-y-4 pb-4 border-b-2 border-border"
      >
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div>
            <h1 className="text-2xl font-semibold text-foreground mb-1">Data Lineage</h1>
            <p className="text-sm text-muted-foreground">
              Visualize data provenance and transformation flows
            </p>
          </div>

          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              className="gap-2"
              onClick={() => setShowFilters(!showFilters)}
            >
              <FilterIcon className="h-4 w-4" />
              {showFilters ? 'Hide' : 'Show'} Filters
            </Button>
            <Button
              variant={showFieldSelector ? 'default' : 'outline'}
              size="sm"
              className="gap-2"
              onClick={() => setShowFieldSelector(!showFieldSelector)}
              disabled={!lineageData || graph.nodes.length === 0}
            >
              <Search className="h-4 w-4" />
              Field View
            </Button>
            <Button
              variant={showSimulator ? 'default' : 'outline'}
              size="sm"
              className="gap-2"
              onClick={() => {
                setShowSimulator(!showSimulator);
                if (showSimulator) {
                  // Reset simulation when closing
                  setSimulationResult(null);
                  setViewMode('current');
                }
              }}
              disabled={!lineageData || graph.nodes.length === 0}
            >
              <GitBranch className="h-4 w-4" />
              What-If
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="gap-2"
              onClick={handleExportGraph}
              disabled={!lineageData || graph.nodes.length === 0}
            >
              <Download className="h-4 w-4" />
              Export
            </Button>
          </div>
        </div>

        {/* Search Section */}
        <div className="flex gap-2 items-start">
          <EntityTypeahead
            value={recordId}
            onChange={setRecordId}
            onSelect={handleEntitySelect}
            placeholder="Search for entities or enter record ID..."
            className="max-w-md flex-1"
          />
          <Button onClick={handleSearch} disabled={isLoading} className="gap-2 mt-0">
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Searching...
              </>
            ) : (
              <>
                <Search className="h-4 w-4" />
                Search
              </>
            )}
          </Button>
          {selectedEntity && (
            <div className="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950/30 border border-blue-200 dark:border-blue-800 rounded-md">
              <span className="text-xs font-medium text-blue-700 dark:text-blue-300">
                Viewing: {selectedEntity.label}
              </span>
            </div>
          )}

          {/* Phase 3: View Mode Toggle (when simulation is active) */}
          {simulationResult && (
            <div className="flex gap-1 ml-4 border border-border rounded-md p-1">
              <Button
                variant={viewMode === 'current' ? 'default' : 'ghost'}
                size="sm"
                className="h-7 px-3 text-xs"
                onClick={() => setViewMode('current')}
              >
                Current
              </Button>
              <Button
                variant={viewMode === 'simulated' ? 'default' : 'ghost'}
                size="sm"
                className="h-7 px-3 text-xs"
                onClick={() => setViewMode('simulated')}
              >
                Simulated
              </Button>
              <Button
                variant={viewMode === 'diff' ? 'default' : 'ghost'}
                size="sm"
                className="h-7 px-3 text-xs"
                onClick={() => setViewMode('diff')}
              >
                Diff
              </Button>
            </div>
          )}
        </div>
      </motion.div>

      {/* Main Content */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.05 }}
        className="relative h-[calc(100%-8rem)] flex gap-4"
      >
        {/* Left Sidebar: Filters, Field Selector, or What-If Simulator */}
        {(showFilters || showFieldSelector || showSimulator) && lineageData && graph.nodes.length > 0 && (
          <div className="w-72 flex-shrink-0 flex flex-col gap-4">
            {/* Phase 3: What-If Simulator */}
            {showSimulator && (
              <WhatIfSimulator
                originalGraph={timeFilteredGraph}
                onSimulationChange={handleSimulationChange}
              />
            )}

            {/* Phase 2: Field-Level Selector */}
            {showFieldSelector && !showSimulator && (
              <FieldSelector
                nodes={timeFilteredGraph.nodes}
                edges={timeFilteredGraph.edges}
                selectedField={filters.selectedField}
                onFieldSelect={(field) => setFilters({ ...filters, selectedField: field })}
              />
            )}

            {/* Standard Filters */}
            {showFilters && !showFieldSelector && !showSimulator && (
              <LineageControls
                filters={filters}
                onFiltersChange={setFilters}
                availableDatasets={availableDatasets}
                availableModels={availableModels}
              />
            )}
          </div>
        )}

        {/* Main Graph Area */}
        <div className="flex-1 min-w-0 flex flex-col gap-4">
          <Card className="glass-morphism border-border flex-1 min-h-0">
            <CardContent className="p-0 h-full relative">
              {/* Content States with AnimatePresence for smooth transitions */}
              <AnimatePresence mode="wait">
                {/* Mockup State: Beautiful demo when no search performed */}
                {!searchRecordId && (
                  <motion.div
                    key="mockup"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.3 }}
                    className="absolute inset-0"
                  >
                    <LineageMockup
                      onSearchPrompt={() => {
                        // Focus the search input when user clicks the prompt
                        const searchInput = document.querySelector('input[placeholder*="record ID"]') as HTMLInputElement;
                        searchInput?.focus();
                      }}
                    />
                  </motion.div>
                )}

                {/* Loading State */}
                {searchRecordId && isLoading && (
                  <motion.div
                    key="loading"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.2 }}
                    className="absolute inset-0 flex items-center justify-center bg-neutral-50 dark:bg-neutral-900"
                  >
                    <div className="text-center">
                      <Loader2 className="h-16 w-16 text-blue-500 dark:text-blue-400 mx-auto mb-4 animate-spin" />
                      <p className="text-lg font-semibold text-neutral-900 dark:text-neutral-50">
                        Loading Lineage...
                      </p>
                      <p className="text-sm text-neutral-600 dark:text-neutral-400 mt-2">
                        Tracing data provenance for {searchRecordId}
                      </p>
                    </div>
                  </motion.div>
                )}

                {/* Error State */}
                {searchRecordId && error && (
                  <motion.div
                    key="error"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.2 }}
                    className="absolute inset-0 flex items-center justify-center bg-neutral-50 dark:bg-neutral-900"
                  >
                    <div className="text-center max-w-md">
                      <AlertCircle className="h-16 w-16 text-red-500 dark:text-red-400 mx-auto mb-4" />
                      <p className="text-lg font-semibold text-neutral-900 dark:text-neutral-50">
                        Error Loading Lineage
                      </p>
                      <p className="text-sm text-neutral-600 dark:text-neutral-400 mt-2">
                        {error instanceof Error ? error.message : 'Failed to fetch lineage data'}
                      </p>
                      <button
                        onClick={() => setSearchRecordId(undefined)}
                        className="mt-4 px-4 py-2 bg-blue-500 hover:bg-blue-600 dark:bg-blue-600 dark:hover:bg-blue-700 text-white rounded-lg text-sm font-medium transition-colors"
                      >
                        Back to Search
                      </button>
                    </div>
                  </motion.div>
                )}

                {/* Empty Results State */}
                {searchRecordId && lineageData && !isLoading && graph.nodes.length === 0 && (
                  <motion.div
                    key="empty"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.2 }}
                    className="absolute inset-0 flex items-center justify-center bg-neutral-50 dark:bg-neutral-900"
                  >
                    <div className="text-center max-w-md">
                      <GitBranch className="h-16 w-16 text-neutral-400 dark:text-neutral-600 mx-auto mb-4" />
                      <p className="text-lg font-semibold text-neutral-900 dark:text-neutral-50">
                        No Lineage Events Found
                      </p>
                      <p className="text-sm text-neutral-600 dark:text-neutral-400 mt-2">
                        Record ID: {lineageData.record_id}
                      </p>
                      <p className="text-xs text-neutral-500 dark:text-neutral-500 mt-3">
                        This record has no recorded lineage events or transformations
                      </p>
                      <button
                        onClick={() => setSearchRecordId(undefined)}
                        className="mt-4 px-4 py-2 bg-blue-500 hover:bg-blue-600 dark:bg-blue-600 dark:hover:bg-blue-700 text-white rounded-lg text-sm font-medium transition-colors"
                      >
                        Try Another Search
                      </button>
                    </div>
                  </motion.div>
                )}

                {/* Success State: Real Lineage Graph */}
                {searchRecordId && lineageData && !isLoading && graph.nodes.length > 0 && (
                  <motion.div
                    key="graph"
                    initial={{ opacity: 0, scale: 0.98 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.98 }}
                    transition={{ duration: 0.3 }}
                    className="absolute inset-0"
                  >
                    <LineageGraph
                      graph={timeFilteredGraph}
                      onNodeClick={handleNodeClick}
                      selectedNodeId={selectedNode?.id}
                      simulationGraph={simulationResult?.simulatedGraph}
                      viewMode={viewMode}
                      className="h-full"
                    />
                  </motion.div>
                )}
              </AnimatePresence>
            </CardContent>
          </Card>

          {/* Time Slider */}
          {searchRecordId && lineageData && graph.nodes.length > 0 && (
            <LineageTimeSlider
              dateRange={graph.metadata.dateRange}
              selectedTime={selectedTime || graph.metadata.dateRange.end}
              onTimeChange={setSelectedTime}
              totalEvents={lineageData.total_count}
            />
          )}
        </div>

        {/* Node Details Panel */}
        {showNodeDetails && selectedNode && (
          <div className="w-96 flex-shrink-0">
            <LineageNodeDetails
              node={selectedNode}
              relatedEdges={relatedEdges}
              onClose={() => setShowNodeDetails(false)}
            />
          </div>
        )}
      </motion.div>
    </div>
  );
}
