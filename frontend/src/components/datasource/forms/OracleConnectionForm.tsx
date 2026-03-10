/**
 * Oracle Connection Form
 * Form for configuring Oracle database connections
 */

import React from 'react';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, CheckCircle2, XCircle, Eye, EyeOff } from 'lucide-react';
import type { OracleConnectionConfig } from '@/types/discovery';

interface OracleConnectionFormProps {
  config: Partial<OracleConnectionConfig>;
  onChange: (config: Partial<OracleConnectionConfig>) => void;
  onTest?: () => void;
  testStatus?: 'idle' | 'testing' | 'success' | 'error';
  testError?: string;
}

export function OracleConnectionForm({
  config,
  onChange,
  onTest,
  testStatus = 'idle',
  testError,
}: OracleConnectionFormProps) {
  const [showPassword, setShowPassword] = React.useState(false);

  const handleChange = (
    field: keyof OracleConnectionConfig,
    value: string | number | undefined
  ) => {
    onChange({ ...config, [field]: value });
  };

  const isValid = Boolean(
    config.host &&
      config.port &&
      config.serviceName &&
      config.username &&
      config.password
  );

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4">
        {/* Hostname */}
        <div className="space-y-2">
          <Label htmlFor="oracle-host">
            Hostname <span className="text-destructive">*</span>
          </Label>
          <Input
            id="oracle-host"
            type="text"
            placeholder="e.g., oracle.example.com"
            value={config.host || ''}
            onChange={(e) => handleChange('host', e.target.value)}
            required
          />
          <p className="text-xs text-muted-foreground">
            Oracle database server hostname or IP address
          </p>
        </div>

        {/* Port */}
        <div className="space-y-2">
          <Label htmlFor="oracle-port">
            Port <span className="text-destructive">*</span>
          </Label>
          <Input
            id="oracle-port"
            type="number"
            placeholder="1521"
            value={config.port || 1521}
            onChange={(e) => handleChange('port', parseInt(e.target.value))}
            required
          />
          <p className="text-xs text-muted-foreground">Default: 1521</p>
        </div>
      </div>

      {/* Service Name */}
      <div className="space-y-2">
        <Label htmlFor="oracle-service-name">
          Service Name <span className="text-destructive">*</span>
        </Label>
        <Input
          id="oracle-service-name"
          type="text"
          placeholder="e.g., ORCL, XE, or PROD"
          value={config.serviceName || ''}
          onChange={(e) => handleChange('serviceName', e.target.value)}
          required
        />
        <p className="text-xs text-muted-foreground">
          Oracle service name (TNS name)
        </p>
      </div>

      {/* Schema (optional) */}
      <div className="space-y-2">
        <Label htmlFor="oracle-schema">Schema (Optional)</Label>
        <Input
          id="oracle-schema"
          type="text"
          placeholder="e.g., APPS, GL, HR"
          value={config.schema || ''}
          onChange={(e) => handleChange('schema', e.target.value)}
        />
        <p className="text-xs text-muted-foreground">
          Leave blank to discover all accessible schemas
        </p>
      </div>

      {/* Username */}
      <div className="space-y-2">
        <Label htmlFor="oracle-username">
          Username <span className="text-destructive">*</span>
        </Label>
        <Input
          id="oracle-username"
          type="text"
          placeholder="e.g., SYSTEM, APPS"
          value={config.username || ''}
          onChange={(e) => handleChange('username', e.target.value)}
          required
          autoComplete="off"
        />
      </div>

      {/* Password */}
      <div className="space-y-2">
        <Label htmlFor="oracle-password">
          Password <span className="text-destructive">*</span>
        </Label>
        <div className="relative">
          <Input
            id="oracle-password"
            type={showPassword ? 'text' : 'password'}
            placeholder="••••••••"
            value={config.password || ''}
            onChange={(e) => handleChange('password', e.target.value)}
            required
            autoComplete="new-password"
            className="pr-10"
          />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="absolute right-0 top-0 h-full px-3"
            onClick={() => setShowPassword(!showPassword)}
          >
            {showPassword ? (
              <EyeOff className="h-4 w-4" />
            ) : (
              <Eye className="h-4 w-4" />
            )}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Password will be encrypted and stored securely
        </p>
      </div>

      {/* Connection String Preview */}
      {config.host && config.serviceName && (
        <Alert>
          <AlertDescription className="text-xs font-mono">
            Connection String: {config.username || 'username'}@{config.host}:
            {config.port || 1521}/{config.serviceName}
          </AlertDescription>
        </Alert>
      )}

      {/* Test Connection Button */}
      {onTest && (
        <div className="space-y-2">
          <Button
            type="button"
            variant="outline"
            onClick={onTest}
            disabled={!isValid || testStatus === 'testing'}
            className="w-full"
          >
            {testStatus === 'testing' && (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            )}
            {testStatus === 'success' && (
              <CheckCircle2 className="mr-2 h-4 w-4 text-green-600" />
            )}
            {testStatus === 'error' && (
              <XCircle className="mr-2 h-4 w-4 text-destructive" />
            )}
            Test Connection
          </Button>

          {testStatus === 'success' && (
            <Alert className="border-green-200 bg-green-50">
              <CheckCircle2 className="h-4 w-4 text-green-600" />
              <AlertDescription className="text-green-800">
                Connection successful! Ready to discover schema.
              </AlertDescription>
            </Alert>
          )}

          {testStatus === 'error' && testError && (
            <Alert variant="destructive">
              <XCircle className="h-4 w-4" />
              <AlertDescription>{testError}</AlertDescription>
            </Alert>
          )}
        </div>
      )}

      {/* Help Text */}
      <Alert>
        <AlertDescription className="text-xs">
          <strong>Tip:</strong> Ensure the Oracle database is accessible from
          this network and the user has SELECT privileges on system catalog
          views (ALL_TABLES, ALL_TAB_COLUMNS, etc.).
        </AlertDescription>
      </Alert>
    </div>
  );
}
