import React from 'react';
import { useParams } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ArrowLeft, Brain, GitBranch, Edit, Trash2, Download, CheckCircle } from 'lucide-react';
import { motion } from 'framer-motion';

export function ModelDetail() {
  const { id } = useParams<{ id: string }>();

  const mockModel = {
    name: 'customer_segmentation_v2',
    version: 'v2.1.0',
    status: 'deployed',
    deployedAt: '2025-09-20',
    schema: 'classification',
    accuracy: 0.94,
    predictions: 12458,
  };

  return (
    <div className="space-y-6 pb-8">
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4 }}
        className="space-y-4"
      >
        <Button variant="ghost" size="sm" className="gap-2">
          <ArrowLeft className="h-4 w-4" />
          Back to Models
        </Button>

        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div className="flex items-start gap-4">
            <div className="p-3 rounded-lg bg-model/10">
              <Brain className="h-8 w-8 text-model" />
            </div>
            <div>
              <h1 className="text-4xl font-bold font-mono">{mockModel.name}</h1>
              <div className="flex items-center gap-3 mt-2">
                <Badge variant="model">{mockModel.version}</Badge>
                <Badge variant="success">Deployed</Badge>
                <span className="text-sm text-muted-foreground">
                  {mockModel.schema}
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
              Update
            </Button>
            <Button variant="outline" size="sm" className="gap-2 text-error hover:text-error">
              <Trash2 className="h-4 w-4" />
              Deprecate
            </Button>
          </div>
        </div>
      </motion.div>

      <div className="grid gap-6 md:grid-cols-3">
        {[
          { label: 'Accuracy', value: `${(mockModel.accuracy * 100).toFixed(1)}%`, delay: 0.1 },
          { label: 'Predictions', value: mockModel.predictions.toLocaleString(), delay: 0.2 },
          { label: 'Deployed', value: '13 days ago', delay: 0.3 },
        ].map((stat) => (
          <motion.div
            key={stat.label}
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.4, delay: stat.delay }}
          >
            <Card className="glass-morphism border-white/10">
              <CardHeader className="pb-3">
                <CardTitle className="text-sm font-medium text-muted-foreground">
                  {stat.label}
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-3xl font-bold">{stat.value}</div>
              </CardContent>
            </Card>
          </motion.div>
        ))}
      </div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4, delay: 0.4 }}
      >
        <Tabs defaultValue="impact" className="space-y-6">
          <TabsList className="glass-morphism border-white/10">
            <TabsTrigger value="impact">Impact Analysis</TabsTrigger>
            <TabsTrigger value="performance">Performance</TabsTrigger>
            <TabsTrigger value="predictions">Predictions</TabsTrigger>
          </TabsList>

          <TabsContent value="impact">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <div className="flex items-center gap-2">
                  <GitBranch className="h-5 w-5 text-model" />
                  <CardTitle>Impact Analysis</CardTitle>
                </div>
                <CardDescription>
                  Entities affected by this model
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="h-96 flex items-center justify-center border-2 border-dashed border-white/10 rounded-lg">
                  <div className="text-center">
                    <GitBranch className="h-12 w-12 text-muted-foreground mx-auto mb-3" />
                    <p className="text-sm text-muted-foreground">
                      React Flow impact graph will render here
                    </p>
                    <p className="text-xs text-muted-foreground mt-1">
                      Showing affected entities and downstream dependencies
                    </p>
                  </div>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="performance">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <CardTitle>Performance Metrics</CardTitle>
                <CardDescription>Model accuracy and precision over time</CardDescription>
              </CardHeader>
              <CardContent>
                <div className="h-64 flex items-center justify-center border-2 border-dashed border-white/10 rounded-lg">
                  <p className="text-sm text-muted-foreground">
                    Performance charts (Recharts) will be displayed here
                  </p>
                </div>
              </CardContent>
            </Card>
          </TabsContent>

          <TabsContent value="predictions">
            <Card className="glass-morphism border-white/10">
              <CardHeader>
                <CardTitle>Recent Predictions</CardTitle>
                <CardDescription>Latest model predictions and confidence scores</CardDescription>
              </CardHeader>
              <CardContent>
                <div className="space-y-3">
                  {Array.from({ length: 5 }).map((_, i) => (
                    <div
                      key={i}
                      className="flex items-center justify-between p-3 rounded-lg bg-white/5 border border-white/10"
                    >
                      <div>
                        <p className="font-mono text-sm">ENT-{1000 + i}</p>
                        <p className="text-xs text-muted-foreground">Segment: High Value</p>
                      </div>
                      <Badge variant="success">{(90 + i)}% confidence</Badge>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          </TabsContent>
        </Tabs>
      </motion.div>
    </div>
  );
}
