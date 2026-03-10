/**
 * Datasource Wizard Demo / Integration Example
 *
 * This file demonstrates how to integrate the enhanced datasource wizard
 * into your application. Replace your existing DatasourceWizard usage
 * with this pattern.
 */

import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Plus } from 'lucide-react';
import { DatasourceWizardEnhanced } from './DatasourceWizardEnhanced';

/**
 * Example 1: Simple Button Trigger
 *
 * Most common use case - a button that opens the wizard
 */
export function SimpleDatasourceWizardExample() {
  const [wizardOpen, setWizardOpen] = useState(false);

  return (
    <>
      <Button onClick={() => setWizardOpen(true)}>
        <Plus className="h-4 w-4 mr-2" />
        Add Data Source
      </Button>

      <DatasourceWizardEnhanced
        open={wizardOpen}
        onOpenChange={setWizardOpen}
      />
    </>
  );
}

/**
 * Example 2: With Page Header Integration
 *
 * Common pattern for datasources list page
 */
export function DatasourcesPageWithWizard() {
  const [wizardOpen, setWizardOpen] = useState(false);

  return (
    <div className="space-y-6">
      {/* Page Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Data Sources</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Manage your connected data systems
          </p>
        </div>

        <Button onClick={() => setWizardOpen(true)}>
          <Plus className="h-4 w-4 mr-2" />
          Add Data Source
        </Button>
      </div>

      {/* Your datasources list/grid here */}
      <div className="grid grid-cols-3 gap-4">
        {/* ... datasource cards ... */}
      </div>

      {/* Wizard Modal */}
      <DatasourceWizardEnhanced
        open={wizardOpen}
        onOpenChange={setWizardOpen}
      />
    </div>
  );
}

/**
 * Example 3: Migration from Original Wizard
 *
 * Drop-in replacement for existing DatasourceWizard usage
 */
export function MigratedExample() {
  const [isWizardOpen, setIsWizardOpen] = useState(false);

  // BEFORE:
  // import { DatasourceWizard } from '@/components/datasource/DatasourceWizard';

  // AFTER:
  // import { DatasourceWizardEnhanced as DatasourceWizard } from '@/components/datasource/DatasourceWizardEnhanced';

  return (
    <>
      <button onClick={() => setIsWizardOpen(true)}>
        Add Datasource
      </button>

      {/* Same props interface - no changes required! */}
      <DatasourceWizardEnhanced
        open={isWizardOpen}
        onOpenChange={setIsWizardOpen}
      />
    </>
  );
}

/**
 * Example 4: Feature Flag Pattern
 *
 * For gradual rollout or A/B testing
 */
import { DatasourceWizard } from './DatasourceWizard';

export function FeatureFlaggedExample() {
  const [wizardOpen, setWizardOpen] = useState(false);

  // Read from environment variable or feature flag service
  const useEnhancedWizard = import.meta.env.VITE_ENHANCED_WIZARD === 'true';

  return (
    <>
      <Button onClick={() => setWizardOpen(true)}>
        Add Data Source
      </Button>

      {useEnhancedWizard ? (
        <DatasourceWizardEnhanced
          open={wizardOpen}
          onOpenChange={setWizardOpen}
        />
      ) : (
        <DatasourceWizard
          open={wizardOpen}
          onOpenChange={setWizardOpen}
        />
      )}
    </>
  );
}

/**
 * Example 5: With Success Callback
 *
 * If you need to perform actions after successful registration
 */
export function WithCallbackExample() {
  const [wizardOpen, setWizardOpen] = useState(false);

  const handleWizardClose = (success: boolean) => {
    setWizardOpen(false);

    if (success) {
      // Perform post-registration actions
      console.log('Datasource registered successfully');
      // Could refresh list, show celebration, etc.
    }
  };

  return (
    <>
      <Button onClick={() => setWizardOpen(true)}>
        Add Data Source
      </Button>

      <DatasourceWizardEnhanced
        open={wizardOpen}
        onOpenChange={setWizardOpen}
      />
    </>
  );
}

/**
 * Integration Notes:
 *
 * 1. The enhanced wizard is a drop-in replacement - same props interface
 * 2. Uses existing hooks (useAvailablePlugins, useRegisterDatasource, etc.)
 * 3. No backend changes required - 100% compatible
 * 4. Preserves all validation logic and error handling
 * 5. React Query automatically updates datasource list on success
 */

/**
 * TypeScript Usage:
 *
 * The wizard props are simple:
 *
 * interface DatasourceWizardEnhancedProps {
 *   open: boolean;              // Controls modal visibility
 *   onOpenChange: (open: boolean) => void;  // Callback when user closes
 * }
 *
 * All other data comes from React Query hooks automatically.
 */
