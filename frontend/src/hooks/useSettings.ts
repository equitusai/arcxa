/**
 * Settings Hook
 * Manages application settings with localStorage persistence
 */

import { useState, useEffect, useCallback } from 'react';
import { useQuery, useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import * as healthApi from '@/api/health';

// Settings interface
export interface AppSettings {
  // API Configuration
  apiEndpoint: string;
  requestTimeout: number;

  // Appearance
  darkMode: boolean;
  density: 'compact' | 'comfortable' | 'spacious';
  fontSize: 'small' | 'medium' | 'large';

  // Notifications
  notificationsEnabled: boolean;
  dataQualityAlerts: boolean;
  fusionAlerts: boolean;
  modelDeploymentAlerts: boolean;

  // Data Refresh
  autoRefresh: boolean;
  refreshInterval: number; // in seconds
  cacheDuration: number; // in seconds
}

const SETTINGS_KEY = 'graphica_settings';

const DEFAULT_SETTINGS: AppSettings = {
  apiEndpoint: import.meta.env.VITE_API_BASE_URL || 'http://localhost:8080/api/v1',
  requestTimeout: 30,
  darkMode: true,
  density: 'comfortable',
  fontSize: 'medium',
  notificationsEnabled: true,
  dataQualityAlerts: true,
  fusionAlerts: true,
  modelDeploymentAlerts: false,
  autoRefresh: true,
  refreshInterval: 30,
  cacheDuration: 300,
};

export function useSettings() {
  const [settings, setSettingsState] = useState<AppSettings>(() => {
    // Load from localStorage on mount
    const saved = localStorage.getItem(SETTINGS_KEY);
    if (saved) {
      try {
        return { ...DEFAULT_SETTINGS, ...JSON.parse(saved) };
      } catch (e) {
        console.error('Failed to parse settings:', e);
        return DEFAULT_SETTINGS;
      }
    }
    return DEFAULT_SETTINGS;
  });

  // Save to localStorage whenever settings change
  useEffect(() => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  }, [settings]);

  // Apply dark mode to document
  useEffect(() => {
    const root = document.documentElement;
    if (settings.darkMode) {
      root.classList.add('dark');
    } else {
      root.classList.remove('dark');
    }
  }, [settings.darkMode]);

  // Update settings
  const updateSettings = useCallback((updates: Partial<AppSettings>) => {
    setSettingsState(prev => ({ ...prev, ...updates }));
  }, []);

  // Reset to defaults
  const resetSettings = useCallback(() => {
    setSettingsState(DEFAULT_SETTINGS);
    toast.success('Settings reset to defaults');
  }, []);

  // Export settings as JSON
  const exportSettings = useCallback(() => {
    const blob = new Blob([JSON.stringify(settings, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `arcxa-settings-${new Date().toISOString().split('T')[0]}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    toast.success('Settings exported');
  }, [settings]);

  // Import settings from file
  const importSettings = useCallback((file: File) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        const imported = JSON.parse(e.target?.result as string);
        setSettingsState({ ...DEFAULT_SETTINGS, ...imported });
        toast.success('Settings imported successfully');
      } catch (error) {
        toast.error('Failed to import settings');
      }
    };
    reader.readAsText(file);
  }, []);

  return {
    settings,
    updateSettings,
    resetSettings,
    exportSettings,
    importSettings,
  };
}

// Health check hook
export function useHealthCheck() {
  return useQuery({
    queryKey: ['health', 'status'],
    queryFn: healthApi.getHealth,
    refetchInterval: 30000, // Check every 30 seconds
    retry: 1,
  });
}

// Test connection mutation
export function useTestConnection() {
  return useMutation({
    mutationFn: async () => {
      const result = await healthApi.getHealth();
      return result;
    },
    onSuccess: () => {
      toast.success('✅ Connection successful');
    },
    onError: () => {
      toast.error('❌ Connection failed');
    },
  });
}
