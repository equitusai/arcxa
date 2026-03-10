import React, { useState } from 'react';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { Slider } from '@/components/ui/slider';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  Shield,
  Zap,
  DollarSign,
  RefreshCw,
  Database,
  ChevronDown,
  Check,
  AlertTriangle,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { WizardFormData } from '../RegisterModelWizard';

interface ResilienceConfigStepProps {
  formData: WizardFormData;
  updateFormData: (data: Partial<WizardFormData>) => void;
}

type ResiliencePreset = 'high-availability' | 'cost-optimized' | 'low-latency' | 'custom';

interface PresetConfig {
  id: ResiliencePreset;
  name: string;
  description: string;
  icon: React.ElementType;
  recommended: 'production' | 'batch' | 'realtime';
  circuitBreaker: {
    enabled: boolean;
    failureThreshold: number;
    successThreshold: number;
    timeoutMs: number;
  };
  retry: {
    enabled: boolean;
    maxAttempts: number;
  };
  cache: {
    enabled: boolean;
    ttlSeconds: number;
  };
}

const PRESETS: PresetConfig[] = [
  {
    id: 'high-availability',
    name: 'High Availability',
    description: 'Strict fault tolerance for production SLAs',
    icon: Shield,
    recommended: 'production',
    circuitBreaker: {
      enabled: true,
      failureThreshold: 3,
      successThreshold: 2,
      timeoutMs: 20000,
    },
    retry: {
      enabled: true,
      maxAttempts: 3,
    },
    cache: {
      enabled: true,
      ttlSeconds: 300,
    },
  },
  {
    id: 'cost-optimized',
    name: 'Cost Optimized',
    description: 'Relaxed thresholds for batch inference',
    icon: DollarSign,
    recommended: 'batch',
    circuitBreaker: {
      enabled: true,
      failureThreshold: 10,
      successThreshold: 5,
      timeoutMs: 60000,
    },
    retry: {
      enabled: true,
      maxAttempts: 2,
    },
    cache: {
      enabled: true,
      ttlSeconds: 600,
    },
  },
  {
    id: 'low-latency',
    name: 'Low Latency',
    description: 'Aggressive timeouts for real-time inference',
    icon: Zap,
    recommended: 'realtime',
    circuitBreaker: {
      enabled: true,
      failureThreshold: 5,
      successThreshold: 3,
      timeoutMs: 10000,
    },
    retry: {
      enabled: true,
      maxAttempts: 2,
    },
    cache: {
      enabled: true,
      ttlSeconds: 180,
    },
  },
];

export function ResilienceConfigStep({ formData, updateFormData }: ResilienceConfigStepProps) {
  const [selectedPreset, setSelectedPreset] = useState<ResiliencePreset>('high-availability');
  const [showAdvanced, setShowAdvanced] = useState(false);

  const applyPreset = (preset: PresetConfig) => {
    setSelectedPreset(preset.id);
    updateFormData({
      circuitBreaker: preset.circuitBreaker,
      retry: preset.retry,
      cache: preset.cache,
    });
  };

  const handleCustomChange = () => {
    if (selectedPreset !== 'custom') {
      setSelectedPreset('custom');
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-2">
          Resilience Configuration
        </h3>
        <p className="text-sm text-muted-foreground">
          Configure fault tolerance, retry logic, and caching
        </p>
      </div>

      {/* Preset Selector */}
      <div>
        <Label className="text-base mb-3 block">Choose a Profile</Label>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {PRESETS.map((preset) => (
            <Card
              key={preset.id}
              className={cn(
                'cursor-pointer transition-all hover:shadow-md',
                selectedPreset === preset.id
                  ? 'border-entity bg-entity/5'
                  : 'hover:border-entity/50'
              )}
              onClick={() => applyPreset(preset)}
            >
              <CardContent className="p-4">
                <div className="flex items-start gap-3">
                  <div
                    className={cn(
                      'p-2 rounded-md',
                      selectedPreset === preset.id
                        ? 'bg-entity text-white'
                        : 'bg-muted'
                    )}
                  >
                    <preset.icon className="h-5 w-5" />
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <h4 className="font-semibold text-sm">{preset.name}</h4>
                      {selectedPreset === preset.id && (
                        <Check className="h-4 w-4 text-entity" />
                      )}
                    </div>
                    <p className="text-xs text-muted-foreground mb-2">
                      {preset.description}
                    </p>
                    <Badge variant="outline" className="text-[10px]">
                      {preset.recommended}
                    </Badge>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      </div>

      {/* Circuit Breaker */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Shield className="h-5 w-5 text-entity" />
              <CardTitle className="text-base">Circuit Breaker</CardTitle>
            </div>
            <Switch
              checked={formData.circuitBreaker.enabled}
              onCheckedChange={(checked) => {
                handleCustomChange();
                updateFormData({
                  circuitBreaker: { ...formData.circuitBreaker, enabled: checked },
                });
              }}
            />
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            Automatically stop requests to failing endpoints
          </p>
        </CardHeader>

        {formData.circuitBreaker.enabled && (
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-sm">Failure Threshold</Label>
                <Badge variant="outline">{formData.circuitBreaker.failureThreshold} failures</Badge>
              </div>
              <Slider
                value={[formData.circuitBreaker.failureThreshold]}
                onValueChange={([value]) => {
                  handleCustomChange();
                  updateFormData({
                    circuitBreaker: {
                      ...formData.circuitBreaker,
                      failureThreshold: value,
                    },
                  });
                }}
                min={1}
                max={20}
                step={1}
                className="w-full"
              />
              <p className="text-xs text-muted-foreground">
                Circuit opens after this many consecutive failures
              </p>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-sm">Success Threshold</Label>
                <Badge variant="outline">{formData.circuitBreaker.successThreshold} successes</Badge>
              </div>
              <Slider
                value={[formData.circuitBreaker.successThreshold]}
                onValueChange={([value]) => {
                  handleCustomChange();
                  updateFormData({
                    circuitBreaker: {
                      ...formData.circuitBreaker,
                      successThreshold: value,
                    },
                  });
                }}
                min={1}
                max={10}
                step={1}
                className="w-full"
              />
              <p className="text-xs text-muted-foreground">
                Required successes to close circuit from half-open state
              </p>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-sm">Circuit Timeout</Label>
                <Badge variant="outline">{formData.circuitBreaker.timeoutMs / 1000}s</Badge>
              </div>
              <Slider
                value={[formData.circuitBreaker.timeoutMs / 1000]}
                onValueChange={([value]) => {
                  handleCustomChange();
                  updateFormData({
                    circuitBreaker: {
                      ...formData.circuitBreaker,
                      timeoutMs: value * 1000,
                    },
                  });
                }}
                min={5}
                max={120}
                step={5}
                className="w-full"
              />
              <p className="text-xs text-muted-foreground">
                Wait time before attempting to close circuit
              </p>
            </div>

            {/* Visual Circuit State Diagram */}
            <div className="p-3 bg-muted rounded-md border border-border">
              <div className="flex items-center justify-between text-xs">
                <div className="flex flex-col items-center gap-1">
                  <div className="w-12 h-12 rounded-full bg-success/20 border-2 border-success flex items-center justify-center">
                    <Check className="h-5 w-5 text-success" />
                  </div>
                  <span className="font-semibold">Closed</span>
                  <span className="text-muted-foreground text-[10px]">Normal</span>
                </div>

                <div className="flex-1 flex items-center justify-center">
                  <div className="text-center">
                    <AlertTriangle className="h-4 w-4 mx-auto text-error mb-1" />
                    <span className="text-[10px] text-error">
                      {formData.circuitBreaker.failureThreshold} failures →
                    </span>
                  </div>
                </div>

                <div className="flex flex-col items-center gap-1">
                  <div className="w-12 h-12 rounded-full bg-error/20 border-2 border-error flex items-center justify-center">
                    <Shield className="h-5 w-5 text-error" />
                  </div>
                  <span className="font-semibold">Open</span>
                  <span className="text-muted-foreground text-[10px]">
                    {formData.circuitBreaker.timeoutMs / 1000}s wait
                  </span>
                </div>
              </div>
            </div>
          </CardContent>
        )}
      </Card>

      {/* Retry Policy */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <RefreshCw className="h-5 w-5 text-entity" />
              <CardTitle className="text-base">Retry Policy</CardTitle>
            </div>
            <Switch
              checked={formData.retry.enabled}
              onCheckedChange={(checked) => {
                handleCustomChange();
                updateFormData({
                  retry: { ...formData.retry, enabled: checked },
                });
              }}
            />
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            Automatically retry failed requests with exponential backoff
          </p>
        </CardHeader>

        {formData.retry.enabled && (
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-sm">Max Retry Attempts</Label>
                <Badge variant="outline">{formData.retry.maxAttempts} attempts</Badge>
              </div>
              <Slider
                value={[formData.retry.maxAttempts]}
                onValueChange={([value]) => {
                  handleCustomChange();
                  updateFormData({
                    retry: { ...formData.retry, maxAttempts: value },
                  });
                }}
                min={1}
                max={5}
                step={1}
                className="w-full"
              />
            </div>

            {/* Retry Timeline Preview */}
            <div className="p-3 bg-muted rounded-md border border-border">
              <p className="text-xs font-semibold mb-2">Exponential Backoff Timeline</p>
              <div className="space-y-1">
                {Array.from({ length: formData.retry.maxAttempts }, (_, i) => {
                  const delay = Math.pow(2, i) * 1000; // 1s, 2s, 4s, 8s, 16s
                  return (
                    <div key={i} className="flex items-center gap-2 text-xs">
                      <Badge variant="outline" className="w-20 justify-center">
                        Attempt {i + 1}
                      </Badge>
                      <div className="flex-1 h-1 bg-entity/30 rounded" />
                      <span className="text-muted-foreground w-12 text-right">
                        +{delay / 1000}s
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </CardContent>
        )}
      </Card>

      {/* Response Caching */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Database className="h-5 w-5 text-entity" />
              <CardTitle className="text-base">Response Caching</CardTitle>
            </div>
            <Switch
              checked={formData.cache.enabled}
              onCheckedChange={(checked) => {
                handleCustomChange();
                updateFormData({
                  cache: { ...formData.cache, enabled: checked },
                });
              }}
            />
          </div>
          <p className="text-xs text-muted-foreground mt-1">
            Cache identical requests to reduce latency and costs
          </p>
        </CardHeader>

        {formData.cache.enabled && (
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label className="text-sm">Cache TTL (Time-to-Live)</Label>
                <Badge variant="outline">
                  {formData.cache.ttlSeconds < 60
                    ? `${formData.cache.ttlSeconds}s`
                    : `${Math.round(formData.cache.ttlSeconds / 60)}m`}
                </Badge>
              </div>
              <Slider
                value={[formData.cache.ttlSeconds]}
                onValueChange={([value]) => {
                  handleCustomChange();
                  updateFormData({
                    cache: { ...formData.cache, ttlSeconds: value },
                  });
                }}
                min={60}
                max={3600}
                step={60}
                className="w-full"
              />
              <p className="text-xs text-muted-foreground">
                How long to cache responses (60s - 60m)
              </p>
            </div>

            <div className="p-3 bg-muted rounded-md border border-border">
              <p className="text-xs">
                <span className="font-semibold">Cache Key:</span>{' '}
                <span className="font-mono text-[10px]">
                  hash(model_id + input_features)
                </span>
              </p>
            </div>
          </CardContent>
        )}
      </Card>

      {/* Custom Configuration Indicator */}
      {selectedPreset === 'custom' && (
        <div className="flex items-center gap-2 p-3 bg-warning/10 border border-warning/20 rounded-md">
          <AlertTriangle className="h-4 w-4 text-warning" />
          <span className="text-sm text-foreground">
            Using custom configuration. Reset to preset for recommended defaults.
          </span>
        </div>
      )}
    </div>
  );
}
