/**
 * Datasource Configuration Wizard
 * Multi-step wizard for registering new datasources
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
import { ChevronRight, ChevronLeft, Loader2, AlertCircle } from 'lucide-react';
import { toast } from 'sonner';
import { useAvailablePlugins, useRegisterDatasource } from '@/hooks/useDatasources';
import type { AvailablePlugin } from '@/api/types';
import { DatasourceTypeSelector } from './DatasourceTypeSelector';
import { mapPluginNameToBackendType } from '@/api/datasources';
import {
  buildDatasourceSecretRef,
  storeDatasourceCredentials,
} from '@/api/secrets';

interface DatasourceWizardProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
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

function isOraclePlugin(plugin: AvailablePlugin | null): boolean {
  if (!plugin) {
    return false;
  }

  return plugin.source_type === 'Oracle' || plugin.name === 'Oracle';
}

function looksLikeRawOdbcConnectionString(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) {
    return true;
  }

  return /(^|;)\s*[^=;]+\s*=/.test(trimmed);
}

function getFieldPresentation(
  plugin: AvailablePlugin | null,
  key: string,
  schema: WizardConfigField
): WizardConfigField {
  if (!isOraclePlugin(plugin)) {
    return schema;
  }

  switch (key) {
    case 'metadata.odbc_driver':
      return {
        ...schema,
        label: 'Oracle ODBC Driver Override',
        placeholder: 'Oracle in instantclient_23_0',
        description:
          'Optional registered Oracle ODBC driver name. Leave the raw ODBC connection string blank when using this path.',
      };
    case 'metadata.odbc_dsn':
      return {
        ...schema,
        label: 'Oracle ODBC DSN Override',
        placeholder: 'OracleInDocker',
        description:
          'Optional registered ODBC DSN. Leave the raw ODBC connection string blank when using DSN resolution.',
      };
    case 'metadata.odbc_connection_string':
      return {
        ...schema,
        label: 'Raw Oracle ODBC Connection String',
        placeholder: 'Driver={Oracle in instantclient_23_0};DBQ=localhost:1521/XE;',
        description:
          'Optional full raw ODBC connection string. This overrides the Oracle host/serviceName/sid fields and takes precedence over ODBC DSN and driver overrides.',
      };
    case 'metadata.odbc_options':
      return {
        ...schema,
        label: 'Extra Oracle ODBC Options',
        placeholder: 'StatementCacheSize=50;FetchBufferSize=65536;',
        description:
          'Optional extra ODBC segments appended to the resolved connection string.',
      };
    default:
      return schema;
  }
}

export function DatasourceWizard({ open, onOpenChange }: DatasourceWizardProps) {
  const [step, setStep] = useState(1);
  const [selectedPlugin, setSelectedPlugin] = useState<AvailablePlugin | null>(null);
  const [datasourceName, setDatasourceName] = useState('');
  const [config, setConfig] = useState<Record<string, DatasourceFormValue>>({});
  const [encryptionEnabled, setEncryptionEnabled] = useState(false);

  const { data: plugins, isLoading: loadingPlugins } = useAvailablePlugins();
  const registerDatasource = useRegisterDatasource();

  const handleReset = () => {
    setStep(1);
    setSelectedPlugin(null);
    setDatasourceName('');
    setConfig({});
    setEncryptionEnabled(false);
  };

  const handleClose = () => {
    handleReset();
    onOpenChange(false);
  };

  const handleNext = () => {
    if (step === 1 && !selectedPlugin) {
      toast.error('Please select a plugin');
      return;
    }
    if (step === 2 && !datasourceName.trim()) {
      toast.error('Please enter a datasource name');
      return;
    }
    if (step === 3 && selectedPlugin && isOraclePlugin(selectedPlugin)) {
      const rawConnectionStringValue = normalizeConfiguredValue(
        config['metadata.odbc_connection_string'],
        (
          selectedPlugin.config_schema as Record<string, WizardConfigField>
        )['metadata.odbc_connection_string']
      );

      if (
        typeof rawConnectionStringValue === 'string' &&
        rawConnectionStringValue !== '' &&
        !looksLikeRawOdbcConnectionString(rawConnectionStringValue)
      ) {
        toast.error(
          'Enter a full ODBC connection string like Driver={Oracle in instantclient_23_0};DBQ=localhost:1521/XE; or leave this blank to use the DSN/driver fields.'
        );
        return;
      }
    }
    setStep(step + 1);
  };

  const handleBack = () => {
    setStep(step - 1);
  };

  const handleRegister = async () => {
    if (!selectedPlugin || !datasourceName.trim()) {
      toast.error('Missing required fields');
      return;
    }

    const backendSourceType = selectedPlugin.source_type || mapPluginNameToBackendType(selectedPlugin.name);
    const secretRef = buildDatasourceSecretRef(datasourceName);

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

    try {
      await registerDatasource.mutateAsync({
        title: datasourceName,
        sourceType: backendSourceType,
        connection: {
          secretRef,
          config: {
            type: backendSourceType,
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

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-3xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Add Datasource</DialogTitle>
          <DialogDescription>
            Configure a new datasource connection in {step} of 4
          </DialogDescription>
        </DialogHeader>

        <div className="mt-4">
          {/* Progress Indicator */}
          <div className="flex items-center gap-2 mb-6">
            {[1, 2, 3, 4].map((s) => (
              <div
                key={s}
                className={`flex-1 h-1 rounded-full ${
                  s <= step ? 'bg-primary' : 'bg-muted'
                }`}
              />
            ))}
          </div>

          {/* Step 1: Select Plugin */}
          {step === 1 && (
            <DatasourceTypeSelector
              plugins={plugins || []}
              selectedPlugin={selectedPlugin}
              onSelectPlugin={setSelectedPlugin}
              isLoading={loadingPlugins}
            />
          )}

          {/* Step 2: Basic Info */}
          {step === 2 && selectedPlugin && (
            <div className="space-y-4">
              <div>
                <h3 className="text-lg font-semibold mb-2">Basic Information</h3>
                <p className="text-sm text-muted-foreground mb-4">
                  Configure basic datasource settings
                </p>
              </div>

              <div className="space-y-4">
                <div>
                  <Label>Datasource Name</Label>
                  <Input
                    value={datasourceName}
                    onChange={(e) => setDatasourceName(e.target.value)}
                    placeholder="e.g., production-postgres"
                    className="mt-1"
                  />
                  <p className="text-xs text-muted-foreground mt-1">
                    Unique identifier for this datasource
                  </p>
                </div>

                <div>
                  <Label>Selected Plugin</Label>
                  <div className="mt-1 p-3 bg-muted rounded-sm">
                    <p className="text-sm font-medium">{selectedPlugin.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {selectedPlugin.description}
                    </p>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Step 3: Connection Configuration */}
          {step === 3 && selectedPlugin && (
            <div className="space-y-4">
              <div>
                <h3 className="text-lg font-semibold mb-2">Connection Configuration</h3>
                <p className="text-sm text-muted-foreground mb-4">
                  Configure connection parameters for {selectedPlugin.name}
                </p>
              </div>

              <div className="space-y-4">
                {/* Dynamic config based on schema */}
                {Object.keys(selectedPlugin.config_schema).length > 0 ? (
                  Object.entries(
                    selectedPlugin.config_schema as Record<string, WizardConfigField>
                  ).map(([key, schema]) => {
                    const fieldSchema = getFieldPresentation(selectedPlugin, key, schema);

                    return (
                    <div key={key}>
                      <Label>{fieldSchema.label || key}</Label>
                      {fieldSchema.type === 'string' && (
                        <Input
                          value={typeof config[key] === 'boolean' ? '' : (config[key] ?? '')}
                          onChange={(e) => setConfig({ ...config, [key]: e.target.value })}
                          placeholder={fieldSchema.placeholder || ''}
                          type={fieldSchema.secret ? 'password' : 'text'}
                          className="mt-1"
                        />
                      )}
                      {fieldSchema.type === 'number' && (
                        <Input
                          type="number"
                          value={typeof config[key] === 'boolean' ? '' : (config[key] ?? '')}
                          onChange={(e) =>
                            setConfig({
                              ...config,
                              [key]:
                                e.target.value === ''
                                  ? undefined
                                  : parseInt(e.target.value, 10),
                            })
                          }
                          placeholder={fieldSchema.placeholder || ''}
                          className="mt-1"
                        />
                      )}
                      {fieldSchema.type === 'boolean' && (
                        <div className="mt-1 flex items-center justify-between rounded-sm border px-3 py-2">
                          <span className="text-sm text-muted-foreground">
                            {fieldSchema.description || 'Toggle this setting'}
                          </span>
                          <Switch
                            checked={Boolean(config[key])}
                            onCheckedChange={(checked) =>
                              setConfig({ ...config, [key]: checked })
                            }
                          />
                        </div>
                      )}
                      {fieldSchema.description && (
                        <p className="text-xs text-muted-foreground mt-1">
                          {fieldSchema.description}
                        </p>
                      )}
                    </div>
                  );
                  })
                ) : (
                  <div>
                    <Label>Configuration (JSON)</Label>
                    <Textarea
                      value={JSON.stringify(config, null, 2)}
                      onChange={(e) => {
                        try {
                          setConfig(JSON.parse(e.target.value));
                        } catch (error) {
                          // Invalid JSON, ignore
                        }
                      }}
                      className="font-mono text-sm h-64 mt-1"
                      placeholder='{\n  "host": "localhost",\n  "port": 5432,\n  "database": "mydb"\n}'
                    />
                  </div>
                )}

                {/* SSL/TLS Encryption Toggle */}
                <div className="flex items-center justify-between space-x-2 pt-4 border-t">
                  <div className="space-y-0.5">
                    <Label htmlFor="encryption-toggle">Enable SSL/TLS Encryption</Label>
                    <p className="text-xs text-muted-foreground">
                      Secure the connection with SSL/TLS encryption
                    </p>
                  </div>
                  <Switch
                    id="encryption-toggle"
                    checked={encryptionEnabled}
                    onCheckedChange={setEncryptionEnabled}
                  />
                </div>
              </div>
            </div>
          )}

          {/* Step 4: Review & Register */}
          {step === 4 && selectedPlugin && (
            <div className="space-y-4">
              <div>
                <h3 className="text-lg font-semibold mb-2">Review & Register</h3>
                <p className="text-sm text-muted-foreground mb-4">
                  Review your configuration and register the datasource
                </p>
              </div>

              <div className="space-y-4">
                {/* Security Notice */}
                <Card className="border-blue-200 bg-blue-50">
                  <CardContent className="pt-4">
                    <div className="flex gap-3">
                      <div className="flex-shrink-0">
                        <AlertCircle className="h-5 w-5 text-blue-600" />
                      </div>
                      <div className="text-sm">
                        <p className="font-medium text-blue-900 mb-1">Secure Connection Storage</p>
                        <p className="text-blue-700">
                          Your connection credentials will be securely stored in the secret vault.
                          The connection will be validated during registration.
                        </p>
                      </div>
                    </div>
                  </CardContent>
                </Card>

                {/* Summary */}
                <Card>
                  <CardHeader>
                    <CardTitle className="text-base">Configuration Summary</CardTitle>
                  </CardHeader>
                  <CardContent className="space-y-2 text-sm">
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Name:</span>
                      <span className="font-medium">{datasourceName}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Plugin:</span>
                      <span className="font-medium">{selectedPlugin.name}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-muted-foreground">Version:</span>
                      <span className="font-medium">{selectedPlugin.version}</span>
                    </div>
                  </CardContent>
                </Card>
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="mt-6">
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
                    <>Register Datasource</>
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
