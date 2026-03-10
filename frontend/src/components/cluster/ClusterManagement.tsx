/**
 * Cluster & Sharding Management
 *
 * Comprehensive cluster management interface with tabbed navigation
 * Supports both single-node and distributed modes with progressive disclosure
 */

import React, { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Server, Activity, Settings as SettingsIcon } from 'lucide-react';
import { motion } from 'framer-motion';
import { ClusterOverview } from './ClusterOverview';
import { useClusterConfig } from '@/hooks/useCluster';

export function ClusterManagement() {
  const [activeTab, setActiveTab] = useState('overview');
  const { data: config } = useClusterConfig();

  const isSingleNode = config?.mode === 'single-node';
  const modeLabel = isSingleNode ? 'Single-Node Mode' : `Distributed Mode`;

  return (
    <div className="space-y-4">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15 }}
        className="pb-4 border-b-2 border-border"
      >
        <div className="flex items-center gap-3 mb-2">
          <Server className="h-6 w-6 text-entity" />
          <h2 className="text-2xl font-semibold text-foreground">
            Cluster & Sharding
          </h2>
        </div>
        <div className="flex items-center gap-2">
          <p className="text-sm text-muted-foreground">
            Monitor cluster health, topology, and performance metrics
          </p>
          <Badge variant="outline" className="ml-2">
            {modeLabel}
          </Badge>
        </div>
      </motion.div>

      {/* Tabbed Interface */}
      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="overview" className="gap-2">
            <Activity className="h-4 w-4" />
            Overview
          </TabsTrigger>
          <TabsTrigger value="performance" className="gap-2">
            <Activity className="h-4 w-4" />
            Performance
          </TabsTrigger>
          <TabsTrigger value="configuration" className="gap-2">
            <SettingsIcon className="h-4 w-4" />
            Configuration
          </TabsTrigger>
        </TabsList>

        {/* Overview Tab */}
        <TabsContent value="overview" className="mt-4">
          <ClusterOverview
            onNavigateToTopology={() => {
              // Future: navigate to dedicated topology view
              console.log('Navigate to topology');
            }}
          />
        </TabsContent>

        {/* Performance Tab */}
        <TabsContent value="performance" className="mt-4">
          <Card className="glass-morphism border-border">
            <CardContent className="p-6">
              <p className="text-sm text-muted-foreground">
                Performance metrics and trends visualization coming soon.
              </p>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Configuration Tab */}
        <TabsContent value="configuration" className="mt-4">
          <Card className="glass-morphism border-border">
            <CardContent className="p-6">
              <p className="text-sm text-muted-foreground">
                Cluster configuration management coming soon.
              </p>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
