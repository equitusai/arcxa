/**
 * LineageMockup Component
 * Stunning, production-grade mockup demonstrating data lineage visualization
 *
 * Design Philosophy:
 * - Oracle Redwood × Microsoft Fluent DNA
 * - Premium SaaS aesthetic (Linear, Figma, Vercel)
 * - Enterprise-grade professionalism
 * - Content over chrome, clarity over decoration
 * - High contrast for 3 AM operational use
 *
 * Features:
 * ✨ Beautiful mockup showing typical e-commerce data flow
 * 🎨 Premium color-coded nodes by operation type
 * 💫 Smooth animations and micro-interactions
 * 🌓 Full dark mode support with high contrast
 * 📊 Interactive tooltips showing metadata
 * ⚡ Gracefully fades out when real data loads
 * 🎯 Clear call-to-action to search for data
 */

import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Database, ShieldCheck, Shuffle, Network, Package, Clock, TrendingUp } from 'lucide-react';
import { cn } from '@/lib/utils';

// Sample nodes representing typical e-commerce data lineage
const MOCK_NODES = [
  {
    id: 'customers_csv',
    label: 'customers_raw.csv',
    type: 'source' as const,
    icon: Database,
    color: '#3b82f6', // blue-500
    darkColor: '#60a5fa', // blue-400
    description: 'Customer source data',
    metadata: { rows: '12,453', size: '2.4 MB', format: 'CSV' },
    position: { x: 60, y: 140 }
  },
  {
    id: 'validate_emails',
    label: 'Email Validator',
    type: 'quality' as const,
    icon: ShieldCheck,
    color: '#f59e0b', // amber-500
    darkColor: '#fbbf24', // amber-400
    description: 'Validate email formats',
    metadata: { passed: '11,892', failed: '561', confidence: '95.5%' },
    position: { x: 240, y: 90 }
  },
  {
    id: 'normalize_addresses',
    label: 'Address Normalizer',
    type: 'transform' as const,
    icon: Shuffle,
    color: '#8b5cf6', // purple-500
    darkColor: '#a78bfa', // purple-400
    description: 'Standardize addresses',
    metadata: { standardized: '12,100', enriched: '8,340' },
    position: { x: 240, y: 210 }
  },
  {
    id: 'semantic_mapper',
    label: 'Semantic Mapper',
    type: 'mapping' as const,
    icon: Network,
    color: '#10b981', // green-500
    darkColor: '#34d399', // green-400
    description: 'Map to ontology',
    metadata: { mapped: '12,453', concepts: '48', confidence: '94.2%' },
    position: { x: 420, y: 120 }
  },
  {
    id: 'customer_kg',
    label: 'Customer KG',
    type: 'destination' as const,
    icon: Package,
    color: '#06b6d4', // cyan-500
    darkColor: '#22d3ee', // cyan-400
    description: 'Knowledge Graph',
    metadata: { entities: '12,453', relationships: '34,890', triples: '89,234' },
    position: { x: 600, y: 140 }
  }
];

// Sample edges showing data flow
const MOCK_EDGES = [
  { from: 'customers_csv', to: 'validate_emails', confidence: 0.98 },
  { from: 'customers_csv', to: 'normalize_addresses', confidence: 0.97 },
  { from: 'validate_emails', to: 'semantic_mapper', confidence: 0.95 },
  { from: 'normalize_addresses', to: 'semantic_mapper', confidence: 0.94 },
  { from: 'semantic_mapper', to: 'customer_kg', confidence: 0.96 }
];

interface LineageMockupProps {
  onSearchPrompt?: () => void;
  className?: string;
}

export function LineageMockup({ onSearchPrompt, className }: LineageMockupProps) {
  const [hoveredNode, setHoveredNode] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);

  const handleNodeClick = (nodeId: string) => {
    setSelectedNode(selectedNode === nodeId ? null : nodeId);
  };

  const selectedNodeData = MOCK_NODES.find(n => n.id === selectedNode);

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3 }}
      className={cn('relative w-full h-full flex items-center justify-center overflow-hidden', className)}
    >
      {/* Background grid pattern - Oracle Redwood style */}
      <div className="absolute inset-0 bg-neutral-50 dark:bg-neutral-900">
        <svg className="absolute inset-0 w-full h-full opacity-30 dark:opacity-20" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <pattern id="mockup-grid" width="40" height="40" patternUnits="userSpaceOnUse">
              <circle cx="2" cy="2" r="1" fill="currentColor" className="text-neutral-300 dark:text-neutral-700" />
            </pattern>
          </defs>
          <rect width="100%" height="100%" fill="url(#mockup-grid)" />
        </svg>
      </div>

      {/* Main content container */}
      <div className="relative z-10 w-full max-w-4xl px-8">
        {/* Header: Call to action */}
        <motion.div
          initial={{ opacity: 0, y: -20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className="text-center mb-12"
        >
          <div className="inline-flex items-center gap-2 px-4 py-2 bg-blue-50 dark:bg-blue-950/30 border border-blue-200 dark:border-blue-800 rounded-full mb-4">
            <TrendingUp className="w-4 h-4 text-blue-600 dark:text-blue-400" />
            <span className="text-xs font-semibold text-blue-700 dark:text-blue-300 uppercase tracking-wide">
              Interactive Demo
            </span>
          </div>

          <h2 className="text-2xl font-semibold text-neutral-900 dark:text-neutral-50 mb-2">
            Visualize Your Data Journey
          </h2>
          <p className="text-sm text-neutral-600 dark:text-neutral-400 max-w-md mx-auto mb-6">
            See how data flows through transformations, validations, and semantic mappings
          </p>

          <p className="text-xs text-neutral-500 dark:text-neutral-500">
            Use the search above to find entities: <code className="px-2 py-0.5 bg-neutral-200 dark:bg-neutral-800 rounded font-mono">customer_12345</code>
            {' or '}
            <code className="px-2 py-0.5 bg-neutral-200 dark:bg-neutral-800 rounded font-mono">order_67890</code>
          </p>
        </motion.div>

        {/* Lineage graph mockup */}
        <motion.div
          initial={{ opacity: 0, scale: 0.95 }}
          animate={{ opacity: 1, scale: 1 }}
          transition={{ delay: 0.4, duration: 0.4 }}
          className="relative h-96 bg-white dark:bg-neutral-800 rounded-xl border border-neutral-200 dark:border-neutral-700 shadow-xl dark:shadow-2xl overflow-hidden"
        >
          {/* Stats overlay - top left */}
          <div className="absolute top-4 left-4 z-20 px-3 py-2 bg-white/95 dark:bg-neutral-900/95 backdrop-blur-sm border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg">
            <div className="flex items-center gap-3 text-xs">
              <div className="flex items-center gap-1">
                <span className="text-neutral-600 dark:text-neutral-400">Nodes:</span>
                <span className="font-semibold text-neutral-900 dark:text-neutral-50">5</span>
              </div>
              <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
              <div className="flex items-center gap-1">
                <span className="text-neutral-600 dark:text-neutral-400">Edges:</span>
                <span className="font-semibold text-neutral-900 dark:text-neutral-50">5</span>
              </div>
              <div className="w-px h-3 bg-neutral-300 dark:bg-neutral-700" />
              <div className="flex items-center gap-1">
                <Clock className="w-3 h-3 text-neutral-500 dark:text-neutral-500" />
                <span className="text-neutral-600 dark:text-neutral-400">Live</span>
              </div>
            </div>
          </div>

          {/* Legend - bottom left */}
          <div className="absolute bottom-4 left-4 z-20 px-3 py-2 bg-white/95 dark:bg-neutral-900/95 backdrop-blur-sm border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg">
            <div className="text-[10px] font-semibold text-neutral-900 dark:text-neutral-50 mb-2 uppercase tracking-wide">
              Node Types
            </div>
            <div className="space-y-1.5">
              {[
                { color: '#3b82f6', darkColor: '#60a5fa', label: 'Source' },
                { color: '#f59e0b', darkColor: '#fbbf24', label: 'Quality Check' },
                { color: '#8b5cf6', darkColor: '#a78bfa', label: 'Transform' },
                { color: '#10b981', darkColor: '#34d399', label: 'Mapping' },
                { color: '#06b6d4', darkColor: '#22d3ee', label: 'Destination' }
              ].map(item => (
                <div key={item.label} className="flex items-center gap-2">
                  <div
                    className="w-3 h-3 rounded-sm"
                    style={{
                      backgroundColor: 'var(--tw-dark-mode, light) === "dark" ? item.darkColor : item.color',
                    }}
                  >
                    <div
                      className="w-full h-full rounded-sm dark:hidden"
                      style={{ backgroundColor: item.color }}
                    />
                    <div
                      className="w-full h-full rounded-sm hidden dark:block"
                      style={{ backgroundColor: item.darkColor }}
                    />
                  </div>
                  <span className="text-[10px] text-neutral-600 dark:text-neutral-400">{item.label}</span>
                </div>
              ))}
            </div>
          </div>

          {/* SVG Canvas */}
          <svg className="w-full h-full">
            <defs>
              {/* Edge gradients for confidence visualization */}
              <linearGradient id="edge-high" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%" stopColor="#10b981" stopOpacity="0.6" />
                <stop offset="100%" stopColor="#10b981" stopOpacity="0.2" />
              </linearGradient>
              <linearGradient id="edge-medium" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%" stopColor="#f59e0b" stopOpacity="0.6" />
                <stop offset="100%" stopColor="#f59e0b" stopOpacity="0.2" />
              </linearGradient>
            </defs>

            {/* Render edges with curved paths */}
            <g>
              {MOCK_EDGES.map((edge, idx) => {
                const fromNode = MOCK_NODES.find(n => n.id === edge.from);
                const toNode = MOCK_NODES.find(n => n.id === edge.to);
                if (!fromNode || !toNode) return null;

                const x1 = fromNode.position.x + 80; // Right edge of from node
                const y1 = fromNode.position.y + 30; // Center of from node
                const x2 = toNode.position.x; // Left edge of to node
                const y2 = toNode.position.y + 30; // Center of to node

                // Bezier curve control points
                const cx1 = x1 + (x2 - x1) * 0.5;
                const cy1 = y1;
                const cx2 = x1 + (x2 - x1) * 0.5;
                const cy2 = y2;

                const pathData = `M ${x1} ${y1} C ${cx1} ${cy1}, ${cx2} ${cy2}, ${x2} ${y2}`;
                const gradient = edge.confidence >= 0.95 ? 'url(#edge-high)' : 'url(#edge-medium)';

                return (
                  <motion.g
                    key={`edge-${idx}`}
                    initial={{ pathLength: 0, opacity: 0 }}
                    animate={{ pathLength: 1, opacity: 1 }}
                    transition={{ delay: 0.6 + idx * 0.1, duration: 0.6, ease: 'easeInOut' }}
                  >
                    <motion.path
                      d={pathData}
                      fill="none"
                      stroke={gradient}
                      strokeWidth={4}
                      className="drop-shadow-sm"
                    />
                  </motion.g>
                );
              })}
            </g>

            {/* Render nodes */}
            <g>
              {MOCK_NODES.map((node, idx) => {
                const Icon = node.icon;
                const isHovered = hoveredNode === node.id;
                const isSelected = selectedNode === node.id;
                const scale = isHovered || isSelected ? 1.05 : 1;

                return (
                  <motion.g
                    key={node.id}
                    initial={{ opacity: 0, scale: 0 }}
                    animate={{ opacity: 1, scale: 1 }}
                    transition={{ delay: 0.5 + idx * 0.1, type: 'spring', stiffness: 300, damping: 20 }}
                    onMouseEnter={() => setHoveredNode(node.id)}
                    onMouseLeave={() => setHoveredNode(null)}
                    onClick={() => handleNodeClick(node.id)}
                    style={{ cursor: 'pointer' }}
                  >
                    {/* Node container */}
                    <motion.rect
                      x={node.position.x}
                      y={node.position.y}
                      width={80}
                      height={60}
                      rx={6}
                      className="fill-white dark:fill-neutral-800 stroke-neutral-200 dark:stroke-neutral-700"
                      strokeWidth={isSelected ? 3 : 2}
                      animate={{
                        scale,
                        strokeWidth: isSelected ? 3 : isHovered ? 2.5 : 2
                      }}
                      transition={{ duration: 0.15 }}
                      style={{
                        transformOrigin: `${node.position.x + 40}px ${node.position.y + 30}px`,
                        filter: isHovered || isSelected
                          ? 'drop-shadow(0 4px 12px rgba(0,0,0,0.15))'
                          : 'drop-shadow(0 2px 4px rgba(0,0,0,0.1))'
                      }}
                    />

                    {/* Icon background circle */}
                    <motion.circle
                      cx={node.position.x + 40}
                      cy={node.position.y + 20}
                      r={12}
                      className="dark:hidden"
                      fill={node.color}
                      fillOpacity={0.15}
                      animate={{ scale }}
                      transition={{ duration: 0.15 }}
                      style={{ transformOrigin: `${node.position.x + 40}px ${node.position.y + 20}px` }}
                    />
                    <motion.circle
                      cx={node.position.x + 40}
                      cy={node.position.y + 20}
                      r={12}
                      className="hidden dark:block"
                      fill={node.darkColor}
                      fillOpacity={0.15}
                      animate={{ scale }}
                      transition={{ duration: 0.15 }}
                      style={{ transformOrigin: `${node.position.x + 40}px ${node.position.y + 20}px` }}
                    />

                    {/* Icon (rendered as foreignObject for better icon rendering) */}
                    <foreignObject
                      x={node.position.x + 32}
                      y={node.position.y + 12}
                      width={16}
                      height={16}
                    >
                      <div className="flex items-center justify-center w-full h-full">
                        <Icon
                          className="w-4 h-4 dark:hidden"
                          style={{ color: node.color }}
                        />
                        <Icon
                          className="w-4 h-4 hidden dark:block"
                          style={{ color: node.darkColor }}
                        />
                      </div>
                    </foreignObject>

                    {/* Node label */}
                    <text
                      x={node.position.x + 40}
                      y={node.position.y + 42}
                      textAnchor="middle"
                      className="text-[9px] font-semibold fill-neutral-900 dark:fill-neutral-50 pointer-events-none select-none"
                      style={{ letterSpacing: '-0.01em' }}
                    >
                      {node.label}
                    </text>

                    {/* Type label */}
                    <text
                      x={node.position.x + 40}
                      y={node.position.y + 53}
                      textAnchor="middle"
                      className="text-[8px] fill-neutral-500 dark:fill-neutral-500 pointer-events-none select-none uppercase"
                      style={{ letterSpacing: '0.02em' }}
                    >
                      {node.type}
                    </text>

                    {/* Selection indicator */}
                    {isSelected && (
                      <motion.rect
                        x={node.position.x - 3}
                        y={node.position.y - 3}
                        width={86}
                        height={66}
                        rx={8}
                        className="fill-none stroke-blue-500 dark:stroke-blue-400"
                        strokeWidth={2}
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        transition={{ duration: 0.2 }}
                      />
                    )}
                  </motion.g>
                );
              })}
            </g>
          </svg>
        </motion.div>

        {/* Node details panel (when selected) */}
        <AnimatePresence>
          {selectedNodeData && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 10 }}
              transition={{ duration: 0.2 }}
              className="mt-4 p-4 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-lg"
            >
              <div className="flex items-start gap-3">
                <div
                  className="p-2 rounded-lg dark:hidden"
                  style={{ backgroundColor: `${selectedNodeData.color}15` }}
                >
                  <selectedNodeData.icon className="w-5 h-5" style={{ color: selectedNodeData.color }} />
                </div>
                <div
                  className="p-2 rounded-lg hidden dark:block"
                  style={{ backgroundColor: `${selectedNodeData.darkColor}15` }}
                >
                  <selectedNodeData.icon className="w-5 h-5" style={{ color: selectedNodeData.darkColor }} />
                </div>

                <div className="flex-1">
                  <div className="flex items-center gap-2 mb-1">
                    <h4 className="font-semibold text-neutral-900 dark:text-neutral-50">
                      {selectedNodeData.label}
                    </h4>
                    <span className="px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wide bg-neutral-200 dark:bg-neutral-700 text-neutral-600 dark:text-neutral-400 rounded">
                      {selectedNodeData.type}
                    </span>
                  </div>
                  <p className="text-xs text-neutral-600 dark:text-neutral-400 mb-3">
                    {selectedNodeData.description}
                  </p>

                  {/* Metadata grid */}
                  <div className="grid grid-cols-3 gap-3">
                    {Object.entries(selectedNodeData.metadata).map(([key, value]) => (
                      <div key={key} className="space-y-0.5">
                        <div className="text-[10px] uppercase tracking-wide font-semibold text-neutral-500 dark:text-neutral-500">
                          {key}
                        </div>
                        <div className="text-sm font-semibold text-neutral-900 dark:text-neutral-50 tabular-nums">
                          {value}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>

                <button
                  onClick={() => setSelectedNode(null)}
                  className="text-neutral-400 hover:text-neutral-600 dark:text-neutral-500 dark:hover:text-neutral-300 transition-colors"
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Footer hint */}
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 1.2 }}
          className="text-center mt-6 text-xs text-neutral-500 dark:text-neutral-500"
        >
          Click on nodes to view details • Edges show data flow confidence
        </motion.div>
      </div>
    </motion.div>
  );
}
