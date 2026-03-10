import React, { useRef } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Separator } from '@/components/ui/separator';
import { Badge } from '@/components/ui/badge';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Settings as SettingsIcon,
  Server,
  Bell,
  Download,
  Upload,
  Palette,
  Database,
  Shield,
  CheckCircle,
  Loader2,
  AlertCircle,
  RefreshCw,
} from 'lucide-react';
import { motion } from 'framer-motion';
import { useSettings, useHealthCheck, useTestConnection } from '@/hooks/useSettings';
import { toast } from 'sonner';
import { ClusterManagement } from '@/components/cluster/ClusterManagement';

export function Settings() {
  const { settings, updateSettings, resetSettings, exportSettings, importSettings } = useSettings();
  const { data: healthData, isLoading: healthLoading, error: healthError } = useHealthCheck();
  const testConnection = useTestConnection();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleImport = () => {
    fileInputRef.current?.click();
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      importSettings(file);
      // Reset file input
      e.target.value = '';
    }
  };

  const handleSave = () => {
    toast.success('✅ Settings saved successfully');
  };

  // Determine connection status
  const isConnected = !healthLoading && !healthError && healthData?.status;
  const connectionStatus = healthLoading
    ? { label: 'Checking...', variant: 'secondary' as const, icon: Loader2 }
    : isConnected
    ? { label: 'Connected', variant: 'success' as const, icon: CheckCircle }
    : { label: 'Disconnected', variant: 'destructive' as const, icon: AlertCircle };

  return (
    <div className="space-y-4 pb-8">
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15 }}
        className="pb-4 border-b-2 border-border"
      >
        <div className="flex items-center gap-3 mb-2">
          <SettingsIcon className="h-6 w-6 text-entity" />
          <h1 className="text-2xl font-semibold text-foreground">
            Settings
          </h1>
        </div>
        <p className="text-sm text-muted-foreground">
          Configure platform preferences and integrations
        </p>
      </motion.div>

      <div className="grid gap-4 lg:grid-cols-2">
        {/* API Configuration */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.05 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <div className="flex items-center gap-2">
                <Server className="h-5 w-5 text-entity" />
                <CardTitle>API Configuration</CardTitle>
              </div>
              <CardDescription>
                Configure backend API endpoint and connection settings
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="api-endpoint">API Endpoint</Label>
                <Input
                  id="api-endpoint"
                  value={settings.apiEndpoint}
                  onChange={(e) => updateSettings({ apiEndpoint: e.target.value })}
                  placeholder="http://localhost:8080"
                />
                <p className="text-xs text-muted-foreground">
                  Base URL for the ARCXA platform API
                </p>
              </div>

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Connection Status</Label>
                  <p className="text-sm text-muted-foreground">
                    Current API connection state
                  </p>
                </div>
                <Badge variant={connectionStatus.variant} className="gap-1">
                  <connectionStatus.icon className={`h-3 w-3 ${healthLoading ? 'animate-spin' : ''}`} />
                  {connectionStatus.label}
                </Badge>
              </div>

              <Separator className="bg-border" />

              <div className="space-y-2">
                <Label htmlFor="timeout">Request Timeout (seconds)</Label>
                <Input
                  id="timeout"
                  type="number"
                  value={settings.requestTimeout}
                  onChange={(e) => updateSettings({ requestTimeout: parseInt(e.target.value) || 30 })}
                  min="5"
                  max="120"
                />
              </div>

              <Button
                className="w-full gap-2"
                onClick={() => testConnection.mutate()}
                disabled={testConnection.isPending}
              >
                {testConnection.isPending ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    Testing...
                  </>
                ) : (
                  <>
                    <RefreshCw className="h-4 w-4" />
                    Test Connection
                  </>
                )}
              </Button>
            </CardContent>
          </Card>
        </motion.div>

        {/* Appearance Settings */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.1 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <div className="flex items-center gap-2">
                <Palette className="h-5 w-5 text-model" />
                <CardTitle>Appearance</CardTitle>
              </div>
              <CardDescription>
                Customize the visual appearance of the platform
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Dark Mode</Label>
                  <p className="text-sm text-muted-foreground">
                    Use dark theme (recommended)
                  </p>
                </div>
                <Switch
                  checked={settings.darkMode}
                  onCheckedChange={(checked) => updateSettings({ darkMode: checked })}
                />
              </div>

              <Separator className="bg-border" />

              <div className="space-y-2">
                <Label htmlFor="density">Interface Density</Label>
                <Select
                  value={settings.density}
                  onValueChange={(value: any) => updateSettings({ density: value })}
                >
                  <SelectTrigger id="density">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="compact">Compact</SelectItem>
                    <SelectItem value="comfortable">Comfortable</SelectItem>
                    <SelectItem value="spacious">Spacious</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="font">Font Size</Label>
                <Select
                  value={settings.fontSize}
                  onValueChange={(value: any) => updateSettings({ fontSize: value })}
                >
                  <SelectTrigger id="font">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="small">Small</SelectItem>
                    <SelectItem value="medium">Medium</SelectItem>
                    <SelectItem value="large">Large</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </CardContent>
          </Card>
        </motion.div>

        {/* Notifications */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.15 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <div className="flex items-center gap-2">
                <Bell className="h-5 w-5 text-warning" />
                <CardTitle>Notifications</CardTitle>
              </div>
              <CardDescription>
                Manage notification preferences and alerts
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Enable Notifications</Label>
                  <p className="text-sm text-muted-foreground">
                    Receive platform alerts and updates
                  </p>
                </div>
                <Switch
                  checked={settings.notificationsEnabled}
                  onCheckedChange={(checked) => updateSettings({ notificationsEnabled: checked })}
                />
              </div>

              <Separator className="bg-border" />

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Data Quality Alerts</Label>
                  <p className="text-sm text-muted-foreground">
                    Notify when quality drops below threshold
                  </p>
                </div>
                <Switch
                  checked={settings.dataQualityAlerts}
                  onCheckedChange={(checked) => updateSettings({ dataQualityAlerts: checked })}
                  disabled={!settings.notificationsEnabled}
                />
              </div>

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Fusion Completion</Label>
                  <p className="text-sm text-muted-foreground">
                    Notify when entity merges complete
                  </p>
                </div>
                <Switch
                  checked={settings.fusionAlerts}
                  onCheckedChange={(checked) => updateSettings({ fusionAlerts: checked })}
                  disabled={!settings.notificationsEnabled}
                />
              </div>

              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Model Deployments</Label>
                  <p className="text-sm text-muted-foreground">
                    Notify on new model deployments
                  </p>
                </div>
                <Switch
                  checked={settings.modelDeploymentAlerts}
                  onCheckedChange={(checked) => updateSettings({ modelDeploymentAlerts: checked })}
                  disabled={!settings.notificationsEnabled}
                />
              </div>
            </CardContent>
          </Card>
        </motion.div>

        {/* Data Refresh */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15, delay: 0.2 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <div className="flex items-center gap-2">
                <Database className="h-5 w-5 text-success" />
                <CardTitle>Data Refresh</CardTitle>
              </div>
              <CardDescription>
                Configure automatic data refresh intervals
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label>Auto Refresh</Label>
                  <p className="text-sm text-muted-foreground">
                    Automatically refresh dashboard data
                  </p>
                </div>
                <Switch
                  checked={settings.autoRefresh}
                  onCheckedChange={(checked) => updateSettings({ autoRefresh: checked })}
                />
              </div>

              <Separator className="bg-border" />

              <div className="space-y-2">
                <Label htmlFor="interval">Refresh Interval</Label>
                <Select
                  value={settings.refreshInterval.toString()}
                  onValueChange={(value) => updateSettings({ refreshInterval: parseInt(value) })}
                  disabled={!settings.autoRefresh}
                >
                  <SelectTrigger id="interval">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="15">15 seconds</SelectItem>
                    <SelectItem value="30">30 seconds</SelectItem>
                    <SelectItem value="60">1 minute</SelectItem>
                    <SelectItem value="300">5 minutes</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div className="space-y-2">
                <Label htmlFor="cache">Cache Duration</Label>
                <Select
                  value={settings.cacheDuration.toString()}
                  onValueChange={(value) => updateSettings({ cacheDuration: parseInt(value) })}
                >
                  <SelectTrigger id="cache">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="60">1 minute</SelectItem>
                    <SelectItem value="300">5 minutes</SelectItem>
                    <SelectItem value="900">15 minutes</SelectItem>
                    <SelectItem value="3600">1 hour</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </CardContent>
          </Card>
        </motion.div>
      </div>

      {/* Cluster & Sharding Management */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.25 }}
      >
        <ClusterManagement />
      </motion.div>

      {/* Export/Import Settings */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.3 }}
      >
        <Card className="glass-morphism border-border">
          <CardHeader>
            <div className="flex items-center gap-2">
              <Shield className="h-5 w-5 text-model" />
              <CardTitle>Settings Management</CardTitle>
            </div>
            <CardDescription>
              Export or import your configuration settings
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex gap-4">
              <Button variant="outline" className="flex-1 gap-2" onClick={exportSettings}>
                <Download className="h-4 w-4" />
                Export Settings
              </Button>
              <Button variant="outline" className="flex-1 gap-2" onClick={handleImport}>
                <Upload className="h-4 w-4" />
                Import Settings
              </Button>
              <Button variant="outline" className="flex-1" onClick={resetSettings}>
                Reset to Defaults
              </Button>
            </div>

            {/* Hidden file input for import */}
            <input
              ref={fileInputRef}
              type="file"
              accept="application/json"
              onChange={handleFileChange}
              className="hidden"
            />
          </CardContent>
        </Card>
      </motion.div>

      {/* Save Button */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15, delay: 0.3 }}
        className="flex justify-end"
      >
        <Button size="lg" className="gap-2" onClick={handleSave}>
          <CheckCircle className="h-4 w-4" />
          Save All Settings
        </Button>
      </motion.div>
    </div>
  );
}
