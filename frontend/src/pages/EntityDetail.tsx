import React from 'react';
import { useParams } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Progress } from '@/components/ui/progress';
import {
  ArrowLeft,
  Database,
  GitBranch,
  Edit,
  Trash2,
  Download,
  Clock,
  Activity,
  CheckCircle,
  TrendingUp
} from 'lucide-react';
import { motion } from 'framer-motion';

// Mock data
const mockEntity = {
  id: 'ENT-001',
  domain: 'Customer',
  confidence: 0.95,
  created: '2025-09-15T10:30:00',
  updated: '2025-10-01T14:22:00',
  status: 'active',
  attributes: [
    { name: 'customer_id', value: '12345', type: 'String', confidence: 1.0, source: 'CRM' },
    { name: 'full_name', value: 'John Doe', type: 'String', confidence: 0.98, source: 'CRM' },
    { name: 'email', value: 'john.doe@example.com', type: 'String', confidence: 0.95, source: 'CRM' },
    { name: 'age', value: '34', type: 'Integer', confidence: 0.89, source: 'ML Model' },
    { name: 'gender', value: 'Male', type: 'String', confidence: 0.92, source: 'ML Model' },
    { name: 'lifetime_value', value: '$12,450', type: 'Float', confidence: 0.87, source: 'Analytics' },
  ],
  fusionHistory: [
    {
      date: '2025-09-20',
      action: 'Merged with ENT-045',
      confidence: 0.94,
      attributes: 3
    },
    {
      date: '2025-09-18',
      action: 'Merged with ENT-023',
      confidence: 0.91,
      attributes: 2
    }
  ]
};

export function EntityDetail() {
  const { id } = useParams<{ id: string }>();

  return (
    <div className="space-y-6 pb-8">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4 }}
        className="space-y-4"
      >
        <Button variant="ghost" size="sm" className="gap-2">
          <ArrowLeft className="h-4 w-4" />
          Back to Entities
        </Button>

        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div className="flex items-start gap-4">
            <div className="p-3 rounded-lg bg-entity/10">
              <Database className="h-8 w-8 text-entity" />
            </div>
            <div>
              <h1 className="text-4xl font-bold font-mono">{mockEntity.id}</h1>
              <div className="flex items-center gap-3 mt-2">
                <Badge variant="entity">{mockEntity.domain}</Badge>
                <Badge variant="success">Active</Badge>
                <span className="text-sm text-muted-foreground">
                  Confidence: {(mockEntity.confidence * 100).toFixed(0)}%
                </span>
              </div>
            </div>
          </div>

          <div className="flex gap-2">
            <Button variant="outline" size="sm" className="gap-2">
              <Download className="h-4 w-4" />
              Export
            </Button>
            <Button variant="outline" size="sm" className="gap-2">
              <Edit className="h-4 w-4" />
              Edit
            </Button>
            <Button variant="outline" size="sm" className="gap-2 text-error hover:text-error">
              <Trash2 className="h-4 w-4" />
              Delete
            </Button>
          </div>
        </div>
      </motion.div>

      {/* Overview Cards */}
      <div className="grid gap-6 md:grid-cols-3">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: 0.1 }}
        >
          <Card className="glass-morphism border-white/10">
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Attributes
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-3xl font-bold">{mockEntity.attributes.length}</div>
              <p className="text-xs text-muted-foreground mt-1">
                4 derived, 2 direct
              </p>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: 0.2 }}
        >
          <Card className="glass-morphism border-white/10">
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Fusion Events
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-3xl font-bold">{mockEntity.fusionHistory.length}</div>
              <p className="text-xs text-muted-foreground mt-1">
                5 attributes merged
              </p>
            </CardContent>
          </Card>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.4, delay: 0.3 }}
        >
          <Card className="glass-morphism border-white/10">
            <CardHeader className="pb-3">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                Last Updated
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-lg font-bold">2 days ago</div>
              <p className="text-xs text-muted-foreground mt-1">
                {new Date(mockEntity.updated).toLocaleString()}
              </p>
            </CardContent>
          </Card>
        </motion.div>
      </div>

      {/* Tabbed Content */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4, delay: 0.4 }}
      >
        <Tabs defaultValue="attributes" className="space-y-6">
          <TabsList className="glass-morphism border-white/10">
            <TabsTrigger value="attributes">Attributes</TabsTrigger>
            <TabsTrigger value="lineage">Lineage</TabsTrigger>
            <TabsTrigger value="fusion">Fusion History</TabsTrigger>
            <TabsTrigger value="activity">Activity Log</TabsTrigger>
          </TabsList>

          {/* Attributes Tab */}
          <TabsContent value="attributes" className="space-y-4">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <CardTitle>Entity Attributes</CardTitle>
                <CardDescription>
                  All attributes associated with this entity
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                {mockEntity.attributes.map((attr, index) => (
                  <motion.div
                    key={attr.name}
                    initial={{ opacity: 0, x: -20 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.3, delay: index * 0.05 }}
                    className="flex items-start justify-between p-4 rounded-lg bg-white/5 border border-white/10"
                  >
                    <div className="flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        <span className="font-mono font-medium">{attr.name}</span>
                        <Badge variant="outline" className="text-xs">
                          {attr.type}
                        </Badge>
                      </div>
                      <p className="text-sm text-muted-foreground">{attr.value}</p>
                      <div className="flex items-center gap-4 mt-2">
                        <span className="text-xs text-muted-foreground">
                          Source: {attr.source}
                        </span>
                        <div className="flex items-center gap-2">
                          <Progress value={attr.confidence * 100} className="w-20 h-1.5" />
                          <span className="text-xs font-mono text-muted-foreground">
                            {(attr.confidence * 100).toFixed(0)}%
                          </span>
                        </div>
                      </div>
                    </div>
                  </motion.div>
                ))}
              </CardContent>
            </Card>
          </TabsContent>

          {/* Lineage Tab */}
          <TabsContent value="lineage" className="space-y-4">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <div className="flex items-center gap-2">
                  <GitBranch className="h-5 w-5 text-entity" />
                  <CardTitle>Data Lineage</CardTitle>
                </div>
                <CardDescription>
                  Visual representation of data provenance
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="h-96 flex items-center justify-center border-2 border-dashed border-white/10 rounded-lg">
                  <div className="text-center">
                    <GitBranch className="h-12 w-12 text-muted-foreground mx-auto mb-3" />
                    <p className="text-sm text-muted-foreground">
                      React Flow lineage graph will render here
                    </p>
                    <p className="text-xs text-muted-foreground mt-1">
                      Showing upstream and downstream dependencies
                    </p>
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          {/* Fusion History Tab */}
          <TabsContent value="fusion" className="space-y-4">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <CardTitle>Fusion History</CardTitle>
                <CardDescription>
                  Timeline of entity merge operations
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                {mockEntity.fusionHistory.map((event, index) => (
                  <motion.div
                    key={index}
                    initial={{ opacity: 0, x: -20 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.3, delay: index * 0.1 }}
                    className="flex items-start gap-4 p-4 rounded-lg bg-white/5 border border-white/10"
                  >
                    <div className="p-2 rounded-lg bg-success/10">
                      <CheckCircle className="h-5 w-5 text-success" />
                    </div>
                    <div className="flex-1">
                      <p className="font-medium">{event.action}</p>
                      <div className="flex items-center gap-4 mt-1 text-sm text-muted-foreground">
                        <span className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          {event.date}
                        </span>
                        <span>{event.attributes} attributes merged</span>
                        <span className="font-mono">
                          {(event.confidence * 100).toFixed(0)}% confidence
                        </span>
                      </div>
                    </div>
                  </motion.div>
                ))}
              </CardContent>
            </Card>
          </TabsContent>

          {/* Activity Log Tab */}
          <TabsContent value="activity" className="space-y-4">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <div className="flex items-center gap-2">
                  <Activity className="h-5 w-5 text-entity" />
                  <CardTitle>Activity Log</CardTitle>
                </div>
                <CardDescription>
                  Recent changes and operations
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="h-64 flex items-center justify-center border-2 border-dashed border-white/10 rounded-lg">
                  <p className="text-sm text-muted-foreground">
                    Activity timeline will be displayed here
                  </p>
                </div>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </motion.div>
    </div>
  );
}
