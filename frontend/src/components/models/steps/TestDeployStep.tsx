import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  CheckCircle2,
  XCircle,
  Loader2,
  Play,
  ChevronDown,
  Rocket,
  AlertTriangle,
  Info,
  Clock,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useRegisterModel, useInvokeModel } from '@/hooks/useModels';
import type { WizardFormData } from '../RegisterModelWizard';
import type { RegisterModelRequest } from '@/api/types';

interface TestDeployStepProps {
  formData: WizardFormData;
  onClose: () => void;
}

export function TestDeployStep({ formData, onClose }: TestDeployStepProps) {
  const [testInput, setTestInput] = useState('');
  const [testResult, setTestResult] = useState<{
    success: boolean;
    output?: any;
    latency_ms?: number;
    error?: string;
  } | null>(null);
  const [showConfig, setShowConfig] = useState(false);

  const registerModelMutation = useRegisterModel();
  const invokeModelMutation = useInvokeModel();

  // Validation checks
  const validationChecks = [
    {
      id: 'name',
      label: 'Model name specified',
      valid: !!formData.name,
    },
    {
      id: 'endpoint',
      label: 'Endpoint URL configured',
      valid: !!formData.endpoint.url,
    },
    {
      id: 'framework',
      label: 'Serving framework selected',
      valid: !!formData.framework,
    },
    {
      id: 'schema',
      label: 'Input schema defined',
      valid: formData.input_schema.length > 0,
    },
    {
      id: 'output',
      label: 'Output fields specified',
      valid: formData.output_schema.length > 0,
    },
  ];

  const allValid = validationChecks.every(check => check.valid);

  // Test model invocation
  const handleTest = async () => {
    setTestResult(null);
    try {
      const parsedInput = JSON.parse(testInput);

      // Note: This would normally invoke the model, but we need it registered first
      // For now, just validate the input structure
      const missingFields = formData.input_schema
        .filter(field => field.required && !(field.name in parsedInput))
        .map(field => field.name);

      if (missingFields.length > 0) {
        setTestResult({
          success: false,
          error: `Missing required fields: ${missingFields.join(', ')}`,
        });
        return;
      }

      setTestResult({
        success: true,
        output: { message: 'Test input validated successfully. Deploy to test actual inference.' },
        latency_ms: 0,
      });
    } catch (error) {
      setTestResult({
        success: false,
        error: 'Invalid JSON format',
      });
    }
  };

  // Generate sample test input
  const generateSampleInput = () => {
    const sample: Record<string, any> = {};
    formData.input_schema.forEach(field => {
      switch (field.data_type) {
        case 'string':
          sample[field.name] = 'sample_value';
          break;
        case 'integer':
          sample[field.name] = 42;
          break;
        case 'float':
          sample[field.name] = 3.14;
          break;
        case 'boolean':
          sample[field.name] = true;
          break;
        case 'array':
          sample[field.name] = [1, 2, 3];
          break;
        case 'object':
          sample[field.name] = {};
          break;
      }
    });
    setTestInput(JSON.stringify(sample, null, 2));
  };

  // Deploy model
  const handleDeploy = async () => {
    if (!allValid) return;

    const request: RegisterModelRequest = {
      id: formData.id,
      name: formData.name,
      version: formData.version,
      endpoint: {
        protocol: formData.endpoint.protocol,
        url: formData.endpoint.url,
        timeout_ms: formData.endpoint.timeout_ms,
        headers: formData.endpoint.headers,
      },
      framework: formData.framework,
      input_schema: formData.input_schema,
      output_schema: formData.output_schema,
      description: formData.description,
      tags: formData.tags,
      circuitBreaker: formData.circuitBreaker,
      retry: formData.retry,
      cache: formData.cache,
    };

    await registerModelMutation.mutateAsync(request);
    onClose();
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-2">
          Test & Deploy
        </h3>
        <p className="text-sm text-muted-foreground">
          Validate configuration and deploy your model
        </p>
      </div>

      {/* Pre-deployment Checklist */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <CheckCircle2 className="h-5 w-5 text-entity" />
            Pre-deployment Checklist
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          {validationChecks.map(check => (
            <div key={check.id} className="flex items-center gap-2">
              {check.valid ? (
                <CheckCircle2 className="h-4 w-4 text-success" />
              ) : (
                <XCircle className="h-4 w-4 text-error" />
              )}
              <span className={cn(
                'text-sm',
                check.valid ? 'text-foreground' : 'text-muted-foreground'
              )}>
                {check.label}
              </span>
            </div>
          ))}

          {!allValid && (
            <Alert variant="destructive" className="mt-4">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                Complete all required fields before deploying
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Configuration Summary */}
      <Card>
        <CardHeader>
          <Collapsible open={showConfig} onOpenChange={setShowConfig}>
            <CollapsibleTrigger className="flex items-center gap-2 w-full">
              <CardTitle className="text-base flex items-center gap-2 flex-1">
                <Info className="h-5 w-5 text-entity" />
                Configuration Summary
              </CardTitle>
              <ChevronDown
                className={cn(
                  'h-4 w-4 transition-transform',
                  showConfig && 'rotate-180'
                )}
              />
            </CollapsibleTrigger>
            <CollapsibleContent className="mt-4">
              <div className="space-y-3 text-sm">
                <div className="grid grid-cols-2 gap-2">
                  <div>
                    <span className="text-muted-foreground">Model ID:</span>
                    <p className="font-mono text-xs">{formData.id}</p>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Version:</span>
                    <p className="font-mono text-xs">{formData.version}</p>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Framework:</span>
                    <p className="font-mono text-xs">{formData.framework}</p>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Protocol:</span>
                    <p className="font-mono text-xs">{formData.endpoint.protocol}</p>
                  </div>
                </div>

                <div>
                  <span className="text-muted-foreground">Endpoint:</span>
                  <p className="font-mono text-xs break-all">{formData.endpoint.url}</p>
                </div>

                <div className="grid grid-cols-3 gap-2">
                  <div>
                    <span className="text-muted-foreground">Input Features:</span>
                    <p className="font-semibold">{formData.input_schema.length}</p>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Output Fields:</span>
                    <p className="font-semibold">{formData.output_schema.length}</p>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Timeout:</span>
                    <p className="font-semibold">{formData.endpoint.timeout_ms}ms</p>
                  </div>
                </div>

                <div className="grid grid-cols-3 gap-2 pt-2 border-t border-border">
                  <div>
                    <span className="text-muted-foreground">Circuit Breaker:</span>
                    <Badge variant={formData.circuitBreaker.enabled ? 'default' : 'outline'} className="mt-1">
                      {formData.circuitBreaker.enabled ? 'Enabled' : 'Disabled'}
                    </Badge>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Retry:</span>
                    <Badge variant={formData.retry.enabled ? 'default' : 'outline'} className="mt-1">
                      {formData.retry.enabled ? `${formData.retry.maxAttempts}x` : 'Disabled'}
                    </Badge>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Cache:</span>
                    <Badge variant={formData.cache.enabled ? 'default' : 'outline'} className="mt-1">
                      {formData.cache.enabled ? `${formData.cache.ttlSeconds}s` : 'Disabled'}
                    </Badge>
                  </div>
                </div>
              </div>
            </CollapsibleContent>
          </Collapsible>
        </CardHeader>
      </Card>

      {/* Test Invocation */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <Play className="h-5 w-5 text-entity" />
            Test Invocation (Optional)
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label htmlFor="test-input">Sample Input</Label>
              <Button
                variant="outline"
                size="sm"
                onClick={generateSampleInput}
                className="text-xs"
              >
                Generate Sample
              </Button>
            </div>
            <Textarea
              id="test-input"
              value={testInput}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setTestInput(e.target.value)}
              placeholder="Paste sample JSON input..."
              rows={6}
              className="font-mono text-sm"
            />
          </div>

          <Button
            onClick={handleTest}
            disabled={!testInput || invokeModelMutation.isPending}
            variant="outline"
            className="w-full"
          >
            {invokeModelMutation.isPending ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Testing...
              </>
            ) : (
              <>
                <Play className="h-4 w-4 mr-2" />
                Validate Input
              </>
            )}
          </Button>

          {testResult && (
            <Alert variant={testResult.success ? 'default' : 'destructive'}>
              <AlertDescription>
                {testResult.success ? (
                  <div className="space-y-2">
                    <div className="flex items-center gap-2">
                      <CheckCircle2 className="h-4 w-4 text-success" />
                      <span className="font-semibold">Input validated successfully</span>
                    </div>
                    {testResult.output && (
                      <p className="text-xs text-muted-foreground">
                        {testResult.output.message}
                      </p>
                    )}
                  </div>
                ) : (
                  <div className="flex items-center gap-2">
                    <XCircle className="h-4 w-4" />
                    <span>{testResult.error || 'Validation failed'}</span>
                  </div>
                )}
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Deploy Action */}
      <div className="flex items-center justify-between pt-4 border-t border-border">
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Clock className="h-4 w-4" />
          <span>Ready to deploy</span>
        </div>
        <Button
          onClick={handleDeploy}
          disabled={!allValid || registerModelMutation.isPending}
          size="lg"
          className="gap-2"
        >
          {registerModelMutation.isPending ? (
            <>
              <Loader2 className="h-5 w-5 animate-spin" />
              Deploying...
            </>
          ) : (
            <>
              <Rocket className="h-5 w-5" />
              Deploy Model
            </>
          )}
        </Button>
      </div>

      {registerModelMutation.isError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            Failed to deploy model. Please check your configuration and try again.
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
