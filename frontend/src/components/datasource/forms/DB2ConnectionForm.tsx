/**
 * DB2 Connection Form
 * Form for configuring IBM DB2 database connections
 */

import React from 'react';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, CheckCircle2, XCircle, Eye, EyeOff } from 'lucide-react';
import type { DB2ConnectionConfig } from '@/types/discovery';

interface DB2ConnectionFormProps {
  config: Partial<DB2ConnectionConfig>;
  onChange: (config: Partial<DB2ConnectionConfig>) => void;
  onTest?: () => void;
  testStatus?: 'idle' | 'testing' | 'success' | 'error';
  testError?: string;
}

export function DB2ConnectionForm({
  config,
  onChange,
  onTest,
  testStatus = 'idle',
  testError,
}: DB2ConnectionFormProps) {
  const [showPassword, setShowPassword] = React.useState(false);

  const handleChange = (field: keyof DB2ConnectionConfig, value: any) => {
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
          <Label htmlFor="db2-host">
            Hostname <span className="text-destructive">*</span>
          </Label>
          <Input
            id="db2-host"
            type="text"
            placeholder="e.g., db2.example.com"
            value={config.host || ''}
            onChange={(e) => handleChange('host', e.target.value)}
            required
          />
          <p className="text-xs text-muted-foreground">
            DB2 server hostname or IP address
          </p>
        </div>

        {/* Port */}
        <div className="space-y-2">
          <Label htmlFor="db2-port">
            Port <span className="text-destructive">*</span>
          </Label>
          <Input
            id="db2-port"
            type="number"
            placeholder="50000"
            value={config.port || 50000}
            onChange={(e) => handleChange('port', parseInt(e.target.value))}
            required
          />
          <p className="text-xs text-muted-foreground">Default: 50000</p>
        </div>
      </div>

      {/* Database Name */}
      <div className="space-y-2">
        <Label htmlFor="db2-database">
          Database Name <span className="text-destructive">*</span>
        </Label>
        <Input
          id="db2-database"
          type="text"
          placeholder="e.g., SAMPLE, PROD, TESTDB"
          value={config.database || ''}
          onChange={(e) => handleChange('database', e.target.value)}
          required
        />
        <p className="text-xs text-muted-foreground">
          DB2 database name (catalog name)
        </p>
      </div>

      {/* Schema (optional) */}
      <div className="space-y-2">
        <Label htmlFor="db2-schema">Schema (Optional)</Label>
        <Input
          id="db2-schema"
          type="text"
          placeholder="e.g., DB2INST1, MYSCHEMA"
          value={config.schema || ''}
          onChange={(e) => handleChange('schema', e.target.value)}
        />
        <p className="text-xs text-muted-foreground">
          Leave blank to discover all accessible schemas
        </p>
      </div>

      {/* Username */}
      <div className="space-y-2">
        <Label htmlFor="db2-username">
          Username <span className="text-destructive">*</span>
        </Label>
        <Input
          id="db2-username"
          type="text"
          placeholder="e.g., db2inst1, db2admin"
          value={config.username || ''}
          onChange={(e) => handleChange('username', e.target.value)}
          required
          autoComplete="off"
        />
      </div>

      {/* Password */}
      <div className="space-y-2">
        <Label htmlFor="db2-password">
          Password <span className="text-destructive">*</span>
        </Label>
        <div className="relative">
          <Input
            id="db2-password"
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
            Connection: {config.database}@{config.host}:{config.port || 50000}
            {config.schema && ` (Schema: ${config.schema})`}
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
          <strong>Tip:</strong> Ensure the DB2 database is accessible and the
          user has CONNECT, SELECT privileges on system catalog tables
          (SYSCAT.TABLES, SYSCAT.COLUMNS, etc.).
        </AlertDescription>
      </Alert>
    </div>
  );
}
