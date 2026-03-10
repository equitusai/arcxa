/**
 * PostgreSQL Connection Form
 * Form for configuring PostgreSQL database connections
 */

import React from 'react';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, CheckCircle2, XCircle, Eye, EyeOff } from 'lucide-react';
import type { PostgreSQLConnectionConfig } from '@/types/discovery';

interface PostgreSQLConnectionFormProps {
  config: Partial<PostgreSQLConnectionConfig>;
  onChange: (config: Partial<PostgreSQLConnectionConfig>) => void;
  onTest?: () => void;
  testStatus?: 'idle' | 'testing' | 'success' | 'error';
  testError?: string;
}

export function PostgreSQLConnectionForm({
  config,
  onChange,
  onTest,
  testStatus = 'idle',
  testError,
}: PostgreSQLConnectionFormProps) {
  const [showPassword, setShowPassword] = React.useState(false);

  const handleChange = (field: keyof PostgreSQLConnectionConfig, value: any) => {
    onChange({ ...config, [field]: value });
  };

  const isValid = Boolean(
    config.host &&
      config.port &&
      config.database &&
      config.username &&
      config.password
  );

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4">
        {/* Hostname */}
        <div className="space-y-2">
          <Label htmlFor="pg-host">
            Hostname <span className="text-destructive">*</span>
          </Label>
          <Input
            id="pg-host"
            type="text"
            placeholder="e.g., localhost, postgres.example.com"
            value={config.host || ''}
            onChange={(e) => handleChange('host', e.target.value)}
            required
          />
          <p className="text-xs text-muted-foreground">
            PostgreSQL server hostname or IP address
          </p>
        </div>

        {/* Port */}
        <div className="space-y-2">
          <Label htmlFor="pg-port">
            Port <span className="text-destructive">*</span>
          </Label>
          <Input
            id="pg-port"
            type="number"
            placeholder="5432"
            value={config.port || 5432}
            onChange={(e) => handleChange('port', parseInt(e.target.value))}
            required
          />
          <p className="text-xs text-muted-foreground">Default: 5432</p>
        </div>
      </div>

      {/* Database Name */}
      <div className="space-y-2">
        <Label htmlFor="pg-database">
          Database Name <span className="text-destructive">*</span>
        </Label>
        <Input
          id="pg-database"
          type="text"
          placeholder="e.g., postgres, myapp, production"
          value={config.database || ''}
          onChange={(e) => handleChange('database', e.target.value)}
          required
        />
        <p className="text-xs text-muted-foreground">
          PostgreSQL database name
        </p>
      </div>

      {/* Schema (optional) */}
      <div className="space-y-2">
        <Label htmlFor="pg-schema">Schema (Optional)</Label>
        <Input
          id="pg-schema"
          type="text"
          placeholder="e.g., public, app_schema"
          value={config.schema || ''}
          onChange={(e) => handleChange('schema', e.target.value)}
        />
        <p className="text-xs text-muted-foreground">
          Leave blank to discover all accessible schemas (default: public)
        </p>
      </div>

      {/* Username */}
      <div className="space-y-2">
        <Label htmlFor="pg-username">
          Username <span className="text-destructive">*</span>
        </Label>
        <Input
          id="pg-username"
          type="text"
          placeholder="e.g., postgres, appuser"
          value={config.username || ''}
          onChange={(e) => handleChange('username', e.target.value)}
          required
          autoComplete="off"
        />
      </div>

      {/* Password */}
      <div className="space-y-2">
        <Label htmlFor="pg-password">
          Password <span className="text-destructive">*</span>
        </Label>
        <div className="relative">
          <Input
            id="pg-password"
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
      {config.host && config.database && (
        <Alert>
          <AlertDescription className="text-xs font-mono">
            postgresql://{config.username || 'user'}@{config.host}:
            {config.port || 5432}/{config.database}
            {config.schema && `?currentSchema=${config.schema}`}
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
          <strong>Tip:</strong> Ensure the PostgreSQL server allows connections
          from this host (check pg_hba.conf) and the user has SELECT privileges
          on information_schema and pg_catalog.
        </AlertDescription>
      </Alert>
    </div>
  );
}
