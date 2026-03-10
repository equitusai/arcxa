import React, { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Plus, Search, Grid3x3, List, Brain, CheckCircle, AlertCircle, Loader2 } from 'lucide-react';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';
import { useModels } from '@/hooks/useModels';
import { RegisterModelWizard } from '@/components/models/RegisterModelWizard';

export function Models() {
  const [viewMode, setViewMode] = useState<'grid' | 'list'>('grid');
  const [searchQuery, setSearchQuery] = useState('');
  const [isRegisterDialogOpen, setIsRegisterDialogOpen] = useState(false);
  const navigate = useNavigate();

  const { data: models, isLoading, error } = useModels();

  // Filter models based on search query
  const filteredModels = useMemo(() => {
    if (!models) return [];
    if (!searchQuery) return models;

    const query = searchQuery.toLowerCase();
    return models.filter(model =>
      model.name.toLowerCase().includes(query) ||
      model.id.toLowerCase().includes(query) ||
      model.version.toLowerCase().includes(query)
    );
  }, [models, searchQuery]);

  const getStatusBadge = (status: string) => {
    if (status === 'deployed') return { variant: 'success' as const, icon: CheckCircle };
    return { variant: 'warning' as const, icon: AlertCircle };
  };

  return (
    <div className="space-y-6">
      {/* Page Header - 80px standard height */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15 }}
        className="flex items-start justify-between pb-4 border-b-2 border-border"
      >
        <div className="min-w-0 flex-1">
          <h1 className="text-2xl font-semibold text-foreground mb-1">
            Model Registry
          </h1>
          <p className="text-sm text-muted-foreground">
            Manage ML models and track their impact on entities
          </p>
        </div>
        <Button size="default" className="gap-2 ml-4" onClick={() => setIsRegisterDialogOpen(true)}>
          <Plus className="h-4 w-4" />
          Register Model
        </Button>
      </motion.div>

      {/* Search and View Controls */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.05 }}
      >
        <Card className="glass-morphism border-border">
          <CardContent className="p-4">
            <div className="flex items-center justify-between gap-4">
              <div className="relative flex-1">
                <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  placeholder="Search models by name, ID, or version..."
                  className="pl-10 h-10"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                />
              </div>
              <div className="flex gap-2">
                <Button
                  variant={viewMode === 'grid' ? 'default' : 'outline'}
                  size="default"
                  onClick={() => setViewMode('grid')}
                  className="gap-2 h-10 px-4"
                >
                  <Grid3x3 className="h-4 w-4" />
                  Grid
                </Button>
                <Button
                  variant={viewMode === 'list' ? 'default' : 'outline'}
                  size="default"
                  onClick={() => setViewMode('list')}
                  className="gap-2 h-10 px-4"
                >
                  <List className="h-4 w-4" />
                  List
                </Button>
              </div>
            </div>
          </CardContent>
        </Card>
      </motion.div>

      {/* Loading State */}
      {isLoading && (
        <div className="flex items-center justify-center py-12">
          <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        </div>
      )}

      {/* Error State */}
      {error && (
        <Card className="glass-morphism border-border">
          <CardContent className="p-8 text-center">
            <AlertCircle className="h-12 w-12 mx-auto mb-4 text-error" />
            <p className="text-sm text-muted-foreground">
              Failed to load models. Please try again.
            </p>
          </CardContent>
        </Card>
      )}

      {/* Empty State */}
      {!isLoading && !error && filteredModels.length === 0 && (
        <Card className="glass-morphism border-border">
          <CardContent className="p-12 text-center">
            <Brain className="h-16 w-16 mx-auto mb-4 text-muted-foreground opacity-50" />
            <h3 className="text-lg font-semibold mb-2">No models found</h3>
            <p className="text-sm text-muted-foreground mb-6">
              {searchQuery
                ? `No models match "${searchQuery}"`
                : 'Get started by registering your first ML model'}
            </p>
            {!searchQuery && (
              <Button onClick={() => setIsRegisterDialogOpen(true)} className="gap-2">
                <Plus className="h-4 w-4" />
                Register Your First Model
              </Button>
            )}
          </CardContent>
        </Card>
      )}

      {/* Model Grid */}
      {!isLoading && !error && filteredModels.length > 0 && (
        <div className={viewMode === 'grid' ? 'grid grid-cols-3 gap-4' : 'space-y-4'}>
          {filteredModels.map((model, index) => (
            <motion.div
              key={model.id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.15, delay: 0.1 + index * 0.03 }}
            >
              <Card
                className="glass-morphism border-border hover:bg-background-secondary/50 transition-all cursor-pointer h-full"
                onClick={() => navigate(`/models/${model.id}`)}
              >
                <CardHeader className="px-6 pt-6 pb-4">
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex items-start gap-3">
                      <div className="p-2.5 rounded-lg bg-model/10">
                        <Brain className="h-5 w-5 text-model" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <CardTitle className="text-base font-mono truncate">{model.name}</CardTitle>
                        <CardDescription className="mt-1.5">
                          <Badge variant="model" className="text-xs">v{model.version}</Badge>
                        </CardDescription>
                      </div>
                    </div>
                    <div className="flex items-center gap-1 ml-2">
                      <CheckCircle className="h-4 w-4 text-success" />
                    </div>
                  </div>
                </CardHeader>
                <CardContent className="px-6 pb-6 space-y-4">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Model ID</span>
                    <span className="font-mono text-xs truncate ml-3">{model.id}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Protocol</span>
                    <span className="font-mono font-medium uppercase">{model.protocol}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Status</span>
                    <Badge variant="success" className="text-xs">Active</Badge>
                  </div>
                </CardContent>
              </Card>
            </motion.div>
          ))}
        </div>
      )}

      {/* Register Model Wizard */}
      <RegisterModelWizard
        open={isRegisterDialogOpen}
        onOpenChange={setIsRegisterDialogOpen}
      />
    </div>
  );
}