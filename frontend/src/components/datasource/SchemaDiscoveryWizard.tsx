/**
 * Schema Discovery Wizard
 * Datasource-backed schema discovery aligned with the coordinator API.
 */

import React, { useMemo, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { ChevronRight, ChevronLeft, Database, Loader2, AlertTriangle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';
import { DiscoveryProgressMonitor } from './DiscoveryProgressMonitor';
import { DiscoveryResults } from './DiscoveryResults';
import { useStartDiscovery } from '@/hooks/useSchemaDiscovery';
import {
  getDatasourceReadinessMessage,
  getDatasourceStatusLabel,
  isDatasourceReadyForOperation,
} from '@/api/datasources';
import {
  useDatasource,
  useTestConnection as useDatasourceTestConnection,
} from '@/hooks/useDatasources';
import type { DiscoveryOptions, DiscoveryResult } from '@/types/discovery';

interface SchemaDiscoveryWizardProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  datasourceId?: string;
  onComplete?: (result: DiscoveryResult) => void;
}

const STEPS = [
  { id: 1, title: 'Datasource', description: 'Validate datasource and capabilities' },
  { id: 2, title: 'Test Connection', description: 'Verify connectivity before discovery' },
  { id: 3, title: 'Discovery Options', description: 'Configure schema filters and sampling' },
  { id: 4, title: 'Run Discovery', description: 'Track discovery progress' },
  { id: 5, title: 'View Results', description: 'Review discovered schema' },
];

function getDatasourceSummary(config: unknown): Array<{ label: string; value: string }> {
  const details = (config || {}) as Record<string, unknown>;
  const summary: Array<{ label: string; value: string }> = [];

  if (typeof details.host === 'string' && details.host.length > 0) {
    summary.push({ label: 'Host', value: details.host });
  }
  if (typeof details.database === 'string' && details.database.length > 0) {
    summary.push({ label: 'Database', value: details.database });
  }
  if (typeof details.schema === 'string' && details.schema.length > 0) {
    summary.push({ label: 'Schema', value: details.schema });
  }
  if (typeof details.serviceName === 'string' && details.serviceName.length > 0) {
    summary.push({ label: 'Service', value: details.serviceName });
  }
  if (typeof details.bucket === 'string' && details.bucket.length > 0) {
    summary.push({ label: 'Bucket', value: details.bucket });
  }

  return summary;
}

export function SchemaDiscoveryWizard({
  open,
  onOpenChange,
  datasourceId,
  onComplete,
}: SchemaDiscoveryWizardProps) {
  const [currentStep, setCurrentStep] = useState(1);
  const [discoveryOptions, setDiscoveryOptions] = useState<DiscoveryOptions>({
    schema_filter: '',
    table_filter: '',
    sample_size: 1000,
    cache_ttl_secs: 3600,
  });
  const [discoveryId, setDiscoveryId] = useState<string | null>(null);
  const [discoveryResult, setDiscoveryResult] = useState<DiscoveryResult | null>(null);

  const datasourceQuery = useDatasource(datasourceId);
  const startDiscoveryMutation = useStartDiscovery();
  const testConnectionMutation = useDatasourceTestConnection();

  const datasource = datasourceQuery.data;
  const datasourceSummary = useMemo(
    () => getDatasourceSummary(datasource?.config?.connection),
    [datasource?.config?.connection]
  );
  const canInferSchema = datasource
    ? isDatasourceReadyForOperation(datasource, 'schemaInference')
    : false;
  const canQuery = datasource ? isDatasourceReadyForOperation(datasource, 'query') : false;
  const discoveryReadinessMessage = datasource
    ? getDatasourceReadinessMessage(datasource, 'schemaInference')
    : null;

  const handleReset = () => {
    setCurrentStep(1);
    setDiscoveryOptions({
      schema_filter: '',
      table_filter: '',
      sample_size: 1000,
      cache_ttl_secs: 3600,
    });
    setDiscoveryId(null);
    setDiscoveryResult(null);
    testConnectionMutation.reset();
    startDiscoveryMutation.reset();
  };

  const handleClose = () => {
    if (currentStep === 4 && discoveryId) {
      if (
        !confirm(
          'Discovery is in progress. Are you sure you want to close? Progress will continue on the server.'
        )
      ) {
        return;
      }
    }

    handleReset();
    onOpenChange(false);
  };

  const handleNext = () => {
    if (!datasourceId) {
      toast.error('Select a registered datasource before running discovery');
      return;
    }

    if (currentStep === 1) {
      if (datasourceQuery.isLoading) {
        return;
      }

      if (!datasource) {
        toast.error('Datasource not found');
        return;
      }

      if (!(datasource.instance_capabilities?.canTest ?? false)) {
        toast.error('This datasource cannot be tested from the coordinator');
        return;
      }
    }

    if (currentStep === 2 && testConnectionMutation.data?.success !== true) {
      toast.error('Please run a successful connection test before proceeding');
      return;
    }

    setCurrentStep((step) => Math.min(step + 1, STEPS.length));
  };

  const handleBack = () => {
    setCurrentStep((step) => Math.max(step - 1, 1));
  };

  const handleTestConnection = async () => {
    if (!datasourceId) {
      toast.error('Datasource ID is required for schema discovery');
      return;
    }

    await testConnectionMutation.mutateAsync(datasourceId);
  };

  const handleStartDiscovery = async () => {
    if (!datasourceId) {
      toast.error('Datasource ID is required for schema discovery');
      return;
    }

    try {
      const result = await startDiscoveryMutation.mutateAsync({
        datasource_id: datasourceId,
        options: {
          schema_filter: discoveryOptions.schema_filter || undefined,
          table_filter: discoveryOptions.table_filter || undefined,
          sample_size: Math.max(1, discoveryOptions.sample_size || 1000),
          cache_ttl_secs: Math.max(60, discoveryOptions.cache_ttl_secs || 3600),
        },
      });

      setDiscoveryId(result.discovery_id);
      setCurrentStep(4);
    } catch {
      // Error is surfaced by the mutation handler.
    }
  };

  const renderDatasourceStep = () => {
    if (!datasourceId) {
      return (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            Schema discovery now runs against registered datasources only. Open this
            wizard from a saved datasource to continue.
          </AlertDescription>
        </Alert>
      );
    }

    if (datasourceQuery.isLoading) {
      return (
        <div className="flex items-center justify-center py-12">
          <Loader2 className="h-6 w-6 animate-spin text-primary" />
        </div>
      );
    }

    if (!datasource) {
      return (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>Datasource could not be loaded.</AlertDescription>
        </Alert>
      );
    }

    return (
      <div className="space-y-4">
        <div>
          <h3 className="text-lg font-semibold mb-2">Registered Datasource</h3>
          <p className="text-sm text-muted-foreground">
            Discovery will use the datasource configuration already saved in the coordinator.
          </p>
        </div>

        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-3">
              <div>
                <CardTitle className="text-base">{datasource.name}</CardTitle>
                <div className="text-sm text-muted-foreground mt-1">
                  {datasource.plugin_name} · {datasource.source_type || 'Unknown'}
                </div>
              </div>
              <Badge variant={canInferSchema ? 'outline' : 'secondary'}>
                {getDatasourceStatusLabel(datasource.status)}
              </Badge>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            {datasourceSummary.length > 0 && (
              <div className="grid gap-3 sm:grid-cols-2">
                {datasourceSummary.map((item) => (
                  <div key={item.label} className="rounded-lg border p-3">
                    <div className="text-xs text-muted-foreground">{item.label}</div>
                    <div className="font-medium mt-1 break-all">{item.value}</div>
                  </div>
                ))}
              </div>
            )}

            <div className="grid gap-3 sm:grid-cols-2">
              <CapabilityItem
                label="Can Test"
                enabled={datasource.instance_capabilities?.canTest ?? false}
              />
              <CapabilityItem
                label="Can Infer Schema"
                enabled={canInferSchema}
              />
              <CapabilityItem
                label="Can Query"
                enabled={canQuery}
              />
              <CapabilityItem
                label="Can Read Workflow"
                enabled={datasource.instance_capabilities?.canReadWorkflow ?? false}
              />
            </div>

            {!canInferSchema && (
              <Alert variant="destructive">
                <AlertTriangle className="h-4 w-4" />
                <AlertDescription>
                  {discoveryReadinessMessage}
                </AlertDescription>
              </Alert>
            )}
          </CardContent>
        </Card>
      </div>
    );
  };

  const renderTestStep = () => (
    <div className="space-y-4">
      <div>
        <h3 className="text-lg font-semibold mb-2">Test Connection</h3>
        <p className="text-sm text-muted-foreground">
          Verify the datasource can connect before discovery starts.
        </p>
      </div>

      <Card>
        <CardContent className="pt-6 space-y-4">
          <Button
            type="button"
            onClick={handleTestConnection}
            disabled={!datasourceId || testConnectionMutation.isPending}
            className="w-full sm:w-auto"
          >
            {testConnectionMutation.isPending ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Testing...
              </>
            ) : (
              <>
                <Database className="h-4 w-4 mr-2" />
                Test Connection
              </>
            )}
          </Button>

          {testConnectionMutation.data?.success === true && (
            <Alert>
              <AlertDescription>
                Connection succeeded in {testConnectionMutation.data.latency_ms} ms.
              </AlertDescription>
            </Alert>
          )}

          {testConnectionMutation.data?.success === false && (
            <Alert variant="destructive">
              <AlertDescription>{testConnectionMutation.data.message}</AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>
    </div>
  );

  const renderOptionsStep = () => (
    <div className="space-y-4">
      <div>
        <h3 className="text-lg font-semibold mb-2">Discovery Options</h3>
        <p className="text-sm text-muted-foreground">
          These options map directly to the coordinator discovery API.
        </p>
      </div>

      <Card>
        <CardContent className="pt-6 space-y-4">
          <div className="space-y-2">
            <Label htmlFor="schema-filter">Schema Filter</Label>
            <Input
              id="schema-filter"
              placeholder="e.g., public, dbo, APPS"
              value={discoveryOptions.schema_filter || ''}
              onChange={(event) =>
                setDiscoveryOptions((current) => ({
                  ...current,
                  schema_filter: event.target.value,
                }))
              }
            />
            <p className="text-xs text-muted-foreground">
              Optional. Restrict discovery to a single schema.
            </p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="table-filter">Table Filter</Label>
            <Input
              id="table-filter"
              placeholder="e.g., customer%, invoice_%"
              value={discoveryOptions.table_filter || ''}
              onChange={(event) =>
                setDiscoveryOptions((current) => ({
                  ...current,
                  table_filter: event.target.value,
                }))
              }
            />
            <p className="text-xs text-muted-foreground">
              Optional. Pattern used to narrow the tables the coordinator discovers.
            </p>
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="sample-size">Sample Size</Label>
              <Input
                id="sample-size"
                type="number"
                min={1}
                value={discoveryOptions.sample_size}
                onChange={(event) =>
                  setDiscoveryOptions((current) => ({
                    ...current,
                    sample_size: Number.parseInt(event.target.value, 10) || 1000,
                  }))
                }
              />
              <p className="text-xs text-muted-foreground">
                Number of rows the coordinator may sample per table.
              </p>
            </div>

            <div className="space-y-2">
              <Label htmlFor="cache-ttl">Cache TTL (seconds)</Label>
              <Input
                id="cache-ttl"
                type="number"
                min={60}
                value={discoveryOptions.cache_ttl_secs}
                onChange={(event) =>
                  setDiscoveryOptions((current) => ({
                    ...current,
                    cache_ttl_secs: Number.parseInt(event.target.value, 10) || 3600,
                  }))
                }
              />
              <p className="text-xs text-muted-foreground">
                How long the coordinator keeps the discovery result cached.
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );

  const renderStepContent = () => {
    switch (currentStep) {
      case 1:
        return renderDatasourceStep();
      case 2:
        return renderTestStep();
      case 3:
        return renderOptionsStep();
      case 4:
        return discoveryId && datasourceId ? (
          <DiscoveryProgressMonitor
            datasource_id={datasourceId}
            discovery_id={discoveryId}
            onComplete={(result) => {
              setDiscoveryResult(result);
              setCurrentStep(5);
            }}
            onError={(message) => {
              toast.error('Discovery failed', { description: message });
            }}
            onCancel={handleClose}
          />
        ) : (
          <div className="text-center py-12">
            <Database className="h-12 w-12 mx-auto text-muted-foreground mb-4" />
            <p className="text-sm text-muted-foreground">
              Click "Start Discovery" to begin.
            </p>
          </div>
        );
      case 5:
        return discoveryResult ? (
          <DiscoveryResults
            result={discoveryResult}
            onStartMapping={(table) => {
              toast.success(`Starting mapping for table: ${table.name}`);
            }}
            onGenerateDDL={() => {
              toast.info('DDL generation is not wired into this flow yet.');
            }}
          />
        ) : (
          <div className="text-center py-12">
            <Loader2 className="h-12 w-12 mx-auto animate-spin text-primary mb-4" />
            <p className="text-sm text-muted-foreground">Loading results...</p>
          </div>
        );
      default:
        return null;
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-5xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle>Schema Discovery Wizard</DialogTitle>
          <DialogDescription>
            Step {currentStep} of {STEPS.length}: {STEPS[currentStep - 1].title}
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-1 mb-4">
          {STEPS.map((step, index) => (
            <React.Fragment key={step.id}>
              <div
                className={cn(
                  'flex-1 h-2 rounded-full transition-all',
                  index + 1 < currentStep
                    ? 'bg-green-600'
                    : index + 1 === currentStep
                    ? 'bg-primary'
                    : 'bg-muted'
                )}
              />
            </React.Fragment>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto px-1">
          <AnimatePresence mode="wait">
            <motion.div
              key={currentStep}
              initial={{ opacity: 0, x: 20 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -20 }}
              transition={{ duration: 0.2 }}
            >
              {renderStepContent()}
            </motion.div>
          </AnimatePresence>
        </div>

        <DialogFooter className="mt-6">
          <div className="flex justify-between w-full">
            <div>
              {currentStep > 1 && currentStep < 4 && (
                <Button variant="outline" onClick={handleBack}>
                  <ChevronLeft className="h-4 w-4 mr-1" />
                  Back
                </Button>
              )}
            </div>

            <div className="flex gap-2">
              <Button variant="ghost" onClick={handleClose}>
                {currentStep === 5 ? 'Close' : 'Cancel'}
              </Button>

              {currentStep < 3 && (
                <Button onClick={handleNext}>
                  Next
                  <ChevronRight className="h-4 w-4 ml-1" />
                </Button>
              )}

              {currentStep === 3 && (
                <Button
                  onClick={handleStartDiscovery}
                  disabled={!datasourceId || startDiscoveryMutation.isPending}
                >
                  {startDiscoveryMutation.isPending ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Starting...
                    </>
                  ) : (
                    <>
                      <Database className="h-4 w-4 mr-2" />
                      Start Discovery
                    </>
                  )}
                </Button>
              )}

              {currentStep === 5 && onComplete && discoveryResult && (
                <Button
                  onClick={() => {
                    onComplete(discoveryResult);
                    handleClose();
                  }}
                >
                  Done
                </Button>
              )}
            </div>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

interface CapabilityItemProps {
  label: string;
  enabled: boolean;
}

function CapabilityItem({ label, enabled }: CapabilityItemProps) {
  return (
    <div className="rounded-lg border p-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1">
        <Badge variant={enabled ? 'outline' : 'secondary'}>
          {enabled ? 'Enabled' : 'Unavailable'}
        </Badge>
      </div>
    </div>
  );
}
