/**
 * Enhanced Datasource Configuration Wizard
 *
 * Premium enterprise wizard with:
 * - Contextual progress stepper
 * - Smooth transitions and animations
 * - Smart defaults and validation
 * - Success celebration
 * - Oracle Redwood + Microsoft Fluent design
 */

import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Switch } from '@/components/ui/switch';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Database,
  ChevronRight,
  ChevronLeft,
  Loader2,
  Check,
  AlertCircle,
  Sparkles,
  Info,
  CheckCircle2,
  Eye,
  EyeOff,
  type LucideIcon,
} from 'lucide-react';
import { toast } from 'sonner';
import {
  useAvailablePlugins,
  useRegisterDatasource,
} from '@/hooks/useDatasources';
import type { AvailablePlugin } from '@/api/types';
import { DatasourceTypeSelectorEnhanced } from './DatasourceTypeSelectorEnhanced';
import { mapPluginNameToBackendType } from '@/api/datasources';
import { storeDatasourceCredentials } from '@/api/secrets';

interface DatasourceWizardEnhancedProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

interface StepInfo {
  id: number;
  title: string;
  description: string;
  icon: LucideIcon;
}

type DatasourceFormValue = string | number | boolean | undefined;

interface WizardConfigField {
  type: 'string' | 'number' | 'boolean';
  label?: string;
  placeholder?: string;
  description?: string;
  required?: boolean;
  secret?: boolean;
  credential?: boolean;
}

function normalizeConfiguredValue(
  value: DatasourceFormValue,
  schema?: WizardConfigField
): DatasourceFormValue {
  if (value === undefined) {
    return undefined;
  }

  if (schema?.type === 'string' && typeof value === 'string') {
    if (schema.credential) {
      return value === '' ? undefined : value;
    }

    const trimmed = value.trim();
    if (!schema.required && trimmed === '') {
      return undefined;
    }
    return trimmed;
  }

  return value;
}

const STEPS: StepInfo[] = [
  {
    id: 1,
    title: 'Select Type',
    description: 'Choose a connector',
    icon: Database,
  },
  {
    id: 2,
    title: 'Basic Info',
    description: 'Name and settings',
    icon: Info,
  },
  {
    id: 3,
    title: 'Configure',
    description: 'Connection details',
    icon: ChevronRight,
  },
  {
    id: 4,
    title: 'Review',
    description: 'Review and save',
    icon: CheckCircle2,
  },
];

// Smart defaults for common connectors
const CONNECTOR_DEFAULTS: Record<string, Record<string, DatasourceFormValue>> = {
  PostgreSQL: {
    host: 'localhost',
    port: 5432,
    database: 'postgres',
  },
  Oracle: {
    host: 'localhost',
    port: 1521,
    sid: 'XE',
  },
  'IBM DB2': {
    host: 'localhost',
    port: 50000,
    database: 'SAMPLE',
  },
  'SAP HANA': {
    host: 'localhost',
    port: 30015,
    database: '',
  },
  Snowflake: {
    account: '',
    warehouse: 'COMPUTE_WH',
    database: '',
  },
};

export function DatasourceWizardEnhanced({ open, onOpenChange }: DatasourceWizardEnhancedProps) {
  const [step, setStep] = useState(1);
  const [selectedPlugin, setSelectedPlugin] = useState<AvailablePlugin | null>(null);
  const [datasourceName, setDatasourceName] = useState('');
  const [config, setConfig] = useState<Record<string, DatasourceFormValue>>({});
  const [encryptionEnabled, setEncryptionEnabled] = useState(false);
  const [showPasswords, setShowPasswords] = useState<Record<string, boolean>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});

  const { data: plugins, isLoading: loadingPlugins } = useAvailablePlugins();
  const registerDatasource = useRegisterDatasource();

  const handleReset = () => {
    setStep(1);
    setSelectedPlugin(null);
    setDatasourceName('');
    setConfig({});
    setEncryptionEnabled(false);
    setShowPasswords({});
    setErrors({});
  };

  const handleClose = () => {
    handleReset();
    onOpenChange(false);
  };

  const validateStep = (currentStep: number): boolean => {
    const newErrors: Record<string, string> = {};

    if (currentStep === 1 && !selectedPlugin) {
      toast.error('Please select a connector');
      return false;
    }

    if (currentStep === 2) {
      if (!datasourceName.trim()) {
        newErrors.datasourceName = 'Name is required';
      } else if (datasourceName.length < 3) {
        newErrors.datasourceName = 'Name must be at least 3 characters';
      } else if (!/^[a-z0-9-_]+$/i.test(datasourceName)) {
        newErrors.datasourceName = 'Only letters, numbers, hyphens, and underscores allowed';
      }
    }

    if (currentStep === 3 && selectedPlugin) {
      // Validate required fields
      Object.entries(
        selectedPlugin.config_schema as Record<string, WizardConfigField>
      ).forEach(([key, schema]) => {
        if (schema.required && !config[key]) {
          newErrors[key] = `${schema.label || key} is required`;
        }
      });
    }

    setErrors(newErrors);
    if (Object.keys(newErrors).length > 0) {
      toast.error('Please fix validation errors');
      return false;
    }

    return true;
  };

  const handleNext = () => {
    if (!validateStep(step)) return;
    setStep(step + 1);
  };

  const handleBack = () => {
    setStep(step - 1);
    setErrors({});
  };

  const handleSelectPlugin = (plugin: AvailablePlugin) => {
    setSelectedPlugin(plugin);
    // Apply smart defaults
    const defaults = CONNECTOR_DEFAULTS[plugin.name] || {};
    setConfig(defaults);
    // Suggest a name
    const suggestedName = `${plugin.name.toLowerCase().replace(/\s+/g, '-')}-${Date.now().toString(36).slice(-4)}`;
    setDatasourceName(suggestedName);
  };

  const handleRegister = async () => {
    if (!selectedPlugin || !datasourceName.trim()) {
      toast.error('Please complete all required fields');
      return;
    }

    try {
      const backendType = selectedPlugin.source_type || mapPluginNameToBackendType(selectedPlugin.name);
      const secretRef = `vault://credentials/${datasourceName}`;

      const connectionConfig: Record<string, DatasourceFormValue> = {};
      const credentials: Record<string, string> = {};
      const metadata: Record<string, DatasourceFormValue> = {};
      Object.entries(config).forEach(([key, value]) => {
        const schema = (selectedPlugin.config_schema as Record<string, WizardConfigField>)[key];
        const normalizedValue = normalizeConfiguredValue(value, schema);

        if (schema?.credential) {
          if (normalizedValue !== undefined && normalizedValue !== '') {
            credentials[key] = String(normalizedValue);
          }
        } else if (key.startsWith('metadata.')) {
          const metadataKey = key.replace(/^metadata\./, '');
          if (normalizedValue !== undefined) {
            metadata[metadataKey] = normalizedValue;
          }
        } else {
          if (normalizedValue !== undefined) {
            connectionConfig[key] = normalizedValue;
          }
        }
      });

      if (Object.keys(credentials).length > 0) {
        try {
          await storeDatasourceCredentials(
            secretRef,
            credentials,
            `Credentials for ${datasourceName}`
          );
        } catch (error) {
          toast.error('Failed to store credentials', {
            description: error instanceof Error ? error.message : 'Secret store unavailable',
          });
          return;
        }
      }

      await registerDatasource.mutateAsync({
        title: datasourceName,
        sourceType: backendType,
        connection: {
          secretRef,
          config: {
            type: backendType,
            ...connectionConfig,
          },
          encryptionEnabled,
        },
        metadata: Object.keys(metadata).length > 0 ? metadata : undefined,
      });
      handleClose();
    } catch (error) {
      // Error toast handled by hook
    }
  };

  const togglePasswordVisibility = (field: string) => {
    setShowPasswords((prev) => ({ ...prev, [field]: !prev[field] }));
  };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader className="flex-shrink-0">
          <DialogTitle className="text-xl font-semibold">Add Data Source</DialogTitle>
          <DialogDescription className="text-sm">
            Configure a new data source connection in 4 steps
          </DialogDescription>
        </DialogHeader>

        {/* Enhanced Progress Stepper */}
        <div className="flex-shrink-0 py-4 border-y border-black/8">
          <div className="flex items-center justify-between">
            {STEPS.map((stepInfo, idx) => {
              const isActive = step === stepInfo.id;
              const isCompleted = step > stepInfo.id;

              return (
                <React.Fragment key={stepInfo.id}>
                  <div className="flex items-center gap-3">
                    {/* Step circle */}
                    <div
                      className={`
                        flex items-center justify-center w-10 h-10 rounded-full border-2 transition-all
                        ${
                          isCompleted
                            ? 'bg-primary border-primary text-white'
                            : isActive
                              ? 'bg-white border-primary text-primary'
                              : 'bg-white border-black/20 text-muted-foreground'
                        }
                      `}
                    >
                      {isCompleted ? (
                        <Check className="h-5 w-5" />
                      ) : (
                        <span className="text-sm font-semibold">{stepInfo.id}</span>
                      )}
                    </div>

                    {/* Step label */}
                    <div className="flex-1 min-w-0">
                      <p
                        className={`text-sm font-semibold ${
                          isActive ? 'text-foreground' : 'text-muted-foreground'
                        }`}
                      >
                        {stepInfo.title}
                      </p>
                      <p className="text-xs text-muted-foreground">{stepInfo.description}</p>
                    </div>
                  </div>

                  {/* Connector line */}
                  {idx < STEPS.length - 1 && (
                    <div
                      className={`flex-1 h-0.5 mx-2 transition-all ${
                        step > stepInfo.id ? 'bg-primary' : 'bg-black/10'
                      }`}
                    />
                  )}
                </React.Fragment>
              );
            })}
          </div>
        </div>

        {/* Step Content */}
        <div className="flex-1 overflow-y-auto py-4 animate-in fade-in slide-in-from-bottom-4 duration-300">
          {/* Step 1: Select Plugin */}
          {step === 1 && (
            <DatasourceTypeSelectorEnhanced
              plugins={plugins || []}
              selectedPlugin={selectedPlugin}
              onSelectPlugin={handleSelectPlugin}
              isLoading={loadingPlugins}
            />
          )}

          {/* Step 2: Basic Info */}
          {step === 2 && selectedPlugin && (
            <div className="space-y-6 max-w-2xl mx-auto">
              <div className="text-center">
                <Sparkles className="h-12 w-12 text-primary mx-auto mb-3" />
                <h3 className="text-lg font-semibold mb-1">Basic Information</h3>
                <p className="text-sm text-muted-foreground">
                  Configure basic settings for your {selectedPlugin.name} connection
                </p>
              </div>

              <div className="space-y-4">
                {/* Name */}
                <div>
                  <Label className="text-sm font-medium">
                    Data Source Name <span className="text-destructive">*</span>
                  </Label>
                  <Input
                    value={datasourceName}
                    onChange={(e) => {
                      setDatasourceName(e.target.value);
                      if (errors.datasourceName) {
                        setErrors((prev) => ({ ...prev, datasourceName: '' }));
                      }
                    }}
                    placeholder="e.g., production-postgres"
                    className={`mt-1.5 ${errors.datasourceName ? 'border-destructive' : ''}`}
                  />
                  {errors.datasourceName ? (
                    <p className="text-xs text-destructive mt-1 flex items-center gap-1">
                      <AlertCircle className="h-3 w-3" />
                      {errors.datasourceName}
                    </p>
                  ) : (
                    <p className="text-xs text-muted-foreground mt-1">
                      Unique identifier for this data source (lowercase, no spaces)
                    </p>
                  )}
                </div>

                {/* Selected Plugin Summary */}
                <Card className="border-2">
                  <CardHeader className="pb-3">
                    <CardTitle className="text-sm font-medium">Selected Connector</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="flex items-center justify-between">
                      <div>
                        <p className="text-sm font-semibold text-foreground">
                          {selectedPlugin.name}
                        </p>
                        <p className="text-xs text-muted-foreground mt-0.5">
                          {selectedPlugin.description}
                        </p>
                      </div>
                      <Badge variant="outline" className="text-xs">
                        v{selectedPlugin.version}
                      </Badge>
                    </div>
                  </CardContent>
                </Card>
              </div>
            </div>
          )}

          {/* Step 3: Configuration */}
          {step === 3 && selectedPlugin && (
            <div className="space-y-6 max-w-2xl mx-auto">
              <div className="text-center">
                <Database className="h-12 w-12 text-primary mx-auto mb-3" />
                <h3 className="text-lg font-semibold mb-1">Connection Configuration</h3>
                <p className="text-sm text-muted-foreground">
                  Configure connection parameters for {selectedPlugin.name}
                </p>
              </div>

              <div className="space-y-4">
                {Object.keys(selectedPlugin.config_schema).length > 0 ? (
                  Object.entries(
                    selectedPlugin.config_schema as Record<string, WizardConfigField>
                  ).map(([key, schema]) => (
                    <div key={key}>
                      <Label className="text-sm font-medium">
                        {schema.label || key}
                        {schema.required && <span className="text-destructive ml-1">*</span>}
                      </Label>

                      {schema.type === 'string' && (
                        <div className="relative">
                          <Input
                            value={typeof config[key] === 'boolean' ? '' : (config[key] ?? '')}
                            onChange={(e) => {
                              setConfig({ ...config, [key]: e.target.value });
                              if (errors[key]) {
                                setErrors((prev) => ({ ...prev, [key]: '' }));
                              }
                            }}
                            placeholder={schema.placeholder || ''}
                            type={schema.secret && !showPasswords[key] ? 'password' : 'text'}
                            className={`mt-1.5 ${errors[key] ? 'border-destructive' : ''} ${
                              schema.secret ? 'pr-10' : ''
                            }`}
                          />
                          {schema.secret && (
                            <button
                              type="button"
                              onClick={() => togglePasswordVisibility(key)}
                              className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                            >
                              {showPasswords[key] ? (
                                <EyeOff className="h-4 w-4" />
                              ) : (
                                <Eye className="h-4 w-4" />
                              )}
                            </button>
                          )}
                        </div>
                      )}

                      {schema.type === 'number' && (
                        <Input
                          type="number"
                          value={typeof config[key] === 'boolean' ? '' : (config[key] ?? '')}
                          onChange={(e) => {
                            setConfig({
                              ...config,
                              [key]:
                                e.target.value === ''
                                  ? undefined
                                  : parseInt(e.target.value, 10),
                            });
                            if (errors[key]) {
                              setErrors((prev) => ({ ...prev, [key]: '' }));
                            }
                          }}
                          placeholder={schema.placeholder || ''}
                          className={`mt-1.5 ${errors[key] ? 'border-destructive' : ''}`}
                        />
                      )}

                      {schema.type === 'boolean' && (
                        <div className="flex items-center gap-2 mt-1.5">
                          <Switch
                            checked={config[key] === true}
                            onCheckedChange={(checked) => {
                              setConfig({ ...config, [key]: checked });
                              if (errors[key]) {
                                setErrors((prev) => ({ ...prev, [key]: '' }));
                              }
                            }}
                          />
                          <span className="text-sm text-muted-foreground">
                            {config[key] ? 'Enabled' : 'Disabled'}
                          </span>
                        </div>
                      )}

                      {errors[key] ? (
                        <p className="text-xs text-destructive mt-1 flex items-center gap-1">
                          <AlertCircle className="h-3 w-3" />
                          {errors[key]}
                        </p>
                      ) : (
                        schema.description && (
                          <p className="text-xs text-muted-foreground mt-1">{schema.description}</p>
                        )
                      )}
                    </div>
                  ))
                ) : (
                  <div>
                    <Label className="text-sm font-medium">Configuration (JSON)</Label>
                    <Textarea
                      value={JSON.stringify(config, null, 2)}
                      onChange={(e) => {
                        try {
                          setConfig(JSON.parse(e.target.value));
                        } catch (error) {
                          // Invalid JSON, ignore
                        }
                      }}
                      className="font-mono text-sm h-64 mt-1.5"
                      placeholder='{\n  "host": "localhost",\n  "port": 5432,\n  "database": "mydb"\n}'
                    />
                  </div>
                )}

                {/* SSL/TLS Encryption Toggle */}
                <Card className="border-2 border-primary/20 bg-primary/5">
                  <CardContent className="pt-4">
                    <div className="flex items-center justify-between">
                      <div className="space-y-1">
                        <Label htmlFor="encryption-toggle-enhanced" className="text-sm font-medium">
                          Enable SSL/TLS Encryption
                        </Label>
                        <p className="text-xs text-muted-foreground">
                          Secure your connection with SSL/TLS encryption for enhanced data protection
                        </p>
                      </div>
                      <Switch
                        id="encryption-toggle-enhanced"
                        checked={encryptionEnabled}
                        onCheckedChange={setEncryptionEnabled}
                      />
                    </div>
                  </CardContent>
                </Card>
              </div>
            </div>
          )}

          {/* Step 4: Review & Register */}
          {step === 4 && selectedPlugin && (
            <div className="space-y-6 max-w-2xl mx-auto">
              <div className="text-center">
                <CheckCircle2 className="h-12 w-12 text-primary mx-auto mb-3" />
                <h3 className="text-lg font-semibold mb-1">Review & Register</h3>
                <p className="text-sm text-muted-foreground">
                  Review your configuration and register the data source
                </p>
              </div>

              <div className="space-y-4">
                {/* Security Notice */}
                <Card className="border-2 border-blue-200 bg-blue-50/50">
                  <CardContent className="pt-4">
                    <div className="flex items-start gap-3">
                      <div className="flex-shrink-0 mt-0.5">
                        <Info className="h-5 w-5 text-blue-600" />
                      </div>
                      <div>
                        <p className="text-sm font-semibold text-blue-900 mb-1">
                          Secure Connection Storage
                        </p>
                        <p className="text-sm text-blue-700">
                          Your connection credentials will be securely stored in the secret vault.
                          The datasource will be registered first. Run a connection test before
                          schema discovery or workflow use.
                        </p>
                      </div>
                    </div>
                  </CardContent>
                </Card>

                {/* Configuration Summary */}
                <Card className="border-2">
                  <CardHeader className="pb-3">
                    <CardTitle className="text-base font-semibold">Configuration Summary</CardTitle>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <div className="grid grid-cols-2 gap-3 text-sm">
                      <div>
                        <p className="text-muted-foreground">Name</p>
                        <p className="font-medium text-foreground">{datasourceName}</p>
                      </div>
                      <div>
                        <p className="text-muted-foreground">Connector</p>
                        <p className="font-medium text-foreground">{selectedPlugin.name}</p>
                      </div>
                      <div>
                        <p className="text-muted-foreground">Version</p>
                        <p className="font-medium text-foreground">{selectedPlugin.version}</p>
                      </div>
                    </div>
                  </CardContent>
                </Card>
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <DialogFooter className="flex-shrink-0 border-t border-black/8 pt-4">
          <div className="flex justify-between w-full">
            <div>
              {step > 1 && (
                <Button variant="outline" onClick={handleBack}>
                  <ChevronLeft className="h-4 w-4 mr-1" />
                  Back
                </Button>
              )}
            </div>

            <div className="flex gap-2">
              <Button variant="ghost" onClick={handleClose}>
                Cancel
              </Button>

              {step < 4 ? (
                <Button onClick={handleNext}>
                  Next
                  <ChevronRight className="h-4 w-4 ml-1" />
                </Button>
              ) : (
                <Button
                  onClick={handleRegister}
                  disabled={registerDatasource.isPending}
                >
                  {registerDatasource.isPending ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Registering...
                    </>
                  ) : (
                    <>
                      <CheckCircle2 className="h-4 w-4 mr-2" />
                      Register Data Source
                    </>
                  )}
                </Button>
              )}
            </div>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Badge({
  variant = 'default',
  className = '',
  style,
  children,
}: {
  variant?: 'default' | 'outline' | 'secondary';
  className?: string;
  style?: React.CSSProperties;
  children: React.ReactNode;
}) {
  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${
        variant === 'outline'
          ? 'border border-black/20 bg-transparent'
          : variant === 'secondary'
            ? 'bg-neutral-100 text-neutral-700'
            : 'bg-primary text-white'
      } ${className}`}
      style={style}
    >
      {children}
    </span>
  );
}
