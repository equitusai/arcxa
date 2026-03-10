import React, { useState, useEffect } from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import { ChevronDown, Check, X, Loader2, Plus, Trash2 } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useTestEndpoint } from '@/hooks/useModels';
import type { WizardFormData } from '../RegisterModelWizard';

interface EndpointConfigStepProps {
  formData: WizardFormData;
  updateFormData: (data: Partial<WizardFormData>) => void;
}

export function EndpointConfigStep({ formData, updateFormData }: EndpointConfigStepProps) {
  const [testResult, setTestResult] = useState<{
    success: boolean;
    latency_ms?: number;
    error?: string;
  } | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [customHeaders, setCustomHeaders] = useState<Array<{ key: string; value: string }>>(
    Object.entries(formData.endpoint.headers || {}).map(([key, value]) => ({ key, value }))
  );

  const testEndpointMutation = useTestEndpoint();

  const handleTest = async () => {
    setTestResult(null);
    const result = await testEndpointMutation.mutateAsync({
      url: formData.endpoint.url,
      protocol: formData.endpoint.protocol,
    });
    setTestResult(result);
  };

  const addHeader = () => {
    setCustomHeaders([...customHeaders, { key: '', value: '' }]);
  };

  const removeHeader = (index: number) => {
    setCustomHeaders(customHeaders.filter((_, i) => i !== index));
  };

  const updateHeader = (index: number, field: 'key' | 'value', value: string) => {
    const newHeaders = [...customHeaders];
    newHeaders[index][field] = value;
    setCustomHeaders(newHeaders);
  };

  // Sync headers to form data
  useEffect(() => {
    const headersObj = customHeaders.reduce((acc, { key, value }) => {
      if (key) acc[key] = value;
      return acc;
    }, {} as Record<string, string>);
    updateFormData({
      endpoint: { ...formData.endpoint, headers: headersObj },
    });
  }, [customHeaders]);

  // Auto-detect framework from URL
  useEffect(() => {
    const url = formData.endpoint.url.toLowerCase();
    let detectedFramework: string | null = null;

    if (url.includes('sagemaker')) {
      detectedFramework = 'sagemaker';
    } else if (url.includes('torchserve') || url.includes(':8080/predictions')) {
      detectedFramework = 'torch';
    } else if (url.includes('tensorflow') || url.includes(':8501')) {
      detectedFramework = 'tensorflow';
    }

    if (detectedFramework && detectedFramework !== formData.framework) {
      // Show hint but don't auto-change
      console.log('Detected framework:', detectedFramework);
    }
  }, [formData.endpoint.url]);

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-2">Endpoint Configuration</h3>
        <p className="text-sm text-muted-foreground">
          Configure how ARCXA connects to your model
        </p>
      </div>

      <div className="space-y-4">
        {/* Protocol */}
        <div className="space-y-2">
          <Label>Protocol <span className="text-error">*</span></Label>
          <div className="grid grid-cols-3 gap-2">
            {(['http', 'grpc', 'lambda'] as const).map((protocol) => (
              <Button
                key={protocol}
                variant={formData.endpoint.protocol === protocol ? 'default' : 'outline'}
                onClick={() =>
                  updateFormData({
                    endpoint: { ...formData.endpoint, protocol },
                  })
                }
                className="justify-start"
              >
                {formData.endpoint.protocol === protocol && (
                  <Check className="h-4 w-4 mr-2" />
                )}
                {protocol.toUpperCase()}
              </Button>
            ))}
          </div>
        </div>

        {/* Endpoint URL */}
        <div className="space-y-2">
          <Label htmlFor="endpoint-url">
            Endpoint URL <span className="text-error">*</span>
          </Label>
          <div className="flex gap-2">
            <Input
              id="endpoint-url"
              value={formData.endpoint.url}
              onChange={(e) =>
                updateFormData({
                  endpoint: { ...formData.endpoint, url: e.target.value },
                })
              }
              placeholder={
                formData.endpoint.protocol === 'http'
                  ? 'https://api.company.com/models/my-model'
                  : formData.endpoint.protocol === 'grpc'
                  ? 'grpc://model-server:8500'
                  : 'arn:aws:lambda:region:account:function:name'
              }
              className="font-mono text-sm"
            />
            <Button
              variant="outline"
              onClick={handleTest}
              disabled={!formData.endpoint.url || testEndpointMutation.isPending}
            >
              {testEndpointMutation.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Testing...
                </>
              ) : (
                'Test'
              )}
            </Button>
          </div>

          {/* Test Result */}
          {testResult && (
            <Alert variant={testResult.success ? 'default' : 'destructive'} className="mt-2">
              <AlertDescription className="flex items-center gap-2">
                {testResult.success ? (
                  <>
                    <Check className="h-4 w-4 text-success" />
                    <span>
                      Endpoint reachable
                      {testResult.latency_ms && ` (${testResult.latency_ms}ms)`}
                    </span>
                  </>
                ) : (
                  <>
                    <X className="h-4 w-4" />
                    <span>{testResult.error || 'Connection failed'}</span>
                  </>
                )}
              </AlertDescription>
            </Alert>
          )}

          <p className="text-xs text-muted-foreground">
            {formData.endpoint.protocol === 'http' && 'HTTPS recommended for production'}
            {formData.endpoint.protocol === 'grpc' && 'Ensure gRPC server is accessible'}
            {formData.endpoint.protocol === 'lambda' && 'Must be a valid Lambda ARN'}
          </p>
        </div>

        {/* Timeout */}
        <div className="space-y-2">
          <Label htmlFor="timeout">Request Timeout (ms)</Label>
          <Input
            id="timeout"
            type="number"
            value={formData.endpoint.timeout_ms}
            onChange={(e) =>
              updateFormData({
                endpoint: { ...formData.endpoint, timeout_ms: parseInt(e.target.value) || 30000 },
              })
            }
            min="1000"
            max="300000"
            step="1000"
          />
          <p className="text-xs text-muted-foreground">
            Default: 30s for HTTP, 60s for SageMaker, 120s for Lambda
          </p>
        </div>

        {/* Advanced Options */}
        <Collapsible open={showAdvanced} onOpenChange={setShowAdvanced}>
          <CollapsibleTrigger className="flex items-center gap-2 text-sm font-semibold text-foreground hover:text-entity transition-colors">
            <ChevronDown className={cn('h-4 w-4 transition-transform', showAdvanced && 'rotate-180')} />
            Advanced Options
          </CollapsibleTrigger>
          <CollapsibleContent className="space-y-4 mt-4">
            {/* Custom Headers */}
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <Label>Custom Headers</Label>
                <Button variant="outline" size="sm" onClick={addHeader} className="gap-2">
                  <Plus className="h-3 w-3" />
                  Add Header
                </Button>
              </div>

              {customHeaders.length === 0 ? (
                <p className="text-xs text-muted-foreground">No custom headers configured</p>
              ) : (
                <div className="space-y-2">
                  {customHeaders.map((header, index) => (
                    <div key={index} className="flex gap-2">
                      <Input
                        placeholder="Header name"
                        value={header.key}
                        onChange={(e) => updateHeader(index, 'key', e.target.value)}
                        className="flex-1 font-mono text-sm"
                      />
                      <Input
                        placeholder="Value"
                        value={header.value}
                        onChange={(e) => updateHeader(index, 'value', e.target.value)}
                        className="flex-1 font-mono text-sm"
                      />
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => removeHeader(index)}
                        className="px-2"
                      >
                        <Trash2 className="h-4 w-4 text-error" />
                      </Button>
                    </div>
                  ))}
                </div>
              )}
              <p className="text-xs text-muted-foreground">
                Common headers: Authorization, X-API-Key, Content-Type
              </p>
            </div>
          </CollapsibleContent>
        </Collapsible>
      </div>
    </div>
  );
}
