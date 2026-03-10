/**
 * FieldMappingStep Component
 *
 * Wizard step for AI-powered field mapping to ontology terms.
 * Optional step that can be skipped for quick imports.
 */

import { useState, useEffect } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Checkbox } from '@/components/ui/checkbox';
import { FieldMappingTable } from './FieldMappingTable';
import { useFieldMapping } from '@/hooks/useFieldMapping';
import { Loader2, Sparkles, AlertCircle, ArrowRight, Check } from 'lucide-react';
import { toast } from 'sonner';
import { useAuthStore } from '@/stores/auth';
import type { Datasource } from '@/api/types';
import type { FieldMappingDecision, MappingAction } from '@/api/field-mapping';

interface FieldMappingStepProps {
  datasource: Datasource;
  tableName: string;
  onComplete: (sessionId: string | null) => void;
  onSkip: () => void;
}

export function FieldMappingStep({
  datasource,
  tableName,
  onComplete,
  onSkip,
}: FieldMappingStepProps) {
  const [enabled, setEnabled] = useState(false);
  const [decisions, setDecisions] = useState<Map<string, FieldMappingDecision>>(new Map());
  const [autoApproveThreshold, setAutoApproveThreshold] = useState(0.95);

  // Get current user from auth store
  const userId = useAuthStore((state) => state.user?.id || 'anonymous');

  const {
    session,
    loading,
    error,
    currentStep,
    startAnalysis,
    submitReview,
    applyToRdf,
    skip,
    isAnalyzing,
    isReviewing,
    isApplying,
  } = useFieldMapping({
    onAnalysisComplete: () => {
      toast.success('Field analysis complete!');
    },
    onReviewComplete: () => {
      toast.success('Mappings reviewed successfully');
    },
    onApplyComplete: () => {
      toast.success('Mappings applied to knowledge graph');
    },
    onError: (err) => {
      toast.error(err.message);
    },
  });

  // Start analysis when enabled
  const handleEnable = async () => {
    setEnabled(true);

    try {
      await startAnalysis(datasource.id, {
        tables: [tableName],
        sample_size: 1000,
        auto_approve_threshold: autoApproveThreshold,
        min_confidence: 0.5,
        max_candidates: 10,
        user_id: userId,
      });
    } catch (err) {
      setEnabled(false);
    }
  };

  // Handle field mapping decision
  const handleActionChange = (
    fieldId: string,
    action: MappingAction,
    selectedUri?: string,
    notes?: string
  ) => {
    const newDecisions = new Map(decisions);
    newDecisions.set(fieldId, {
      field_id: fieldId,
      action,
      selected_mapping: selectedUri,
      notes,
    });
    setDecisions(newDecisions);
  };

  // Bulk approve all pending
  const handleBulkApprove = () => {
    if (!session) return;

    const newDecisions = new Map(decisions);
    session.tables.forEach(table => {
      table.field_mappings
        .filter(fm => fm.approval_status === 'pending')
        .forEach(fm => {
          newDecisions.set(fm.field_id, {
            field_id: fm.field_id,
            action: 'approve',
          });
        });
    });
    setDecisions(newDecisions);
  };

  // Bulk reject all pending
  const handleBulkReject = () => {
    if (!session) return;

    const newDecisions = new Map(decisions);
    session.tables.forEach(table => {
      table.field_mappings
        .filter(fm => fm.approval_status === 'pending')
        .forEach(fm => {
          newDecisions.set(fm.field_id, {
            field_id: fm.field_id,
            action: 'reject',
          });
        });
    });
    setDecisions(newDecisions);
  };

  // Apply mappings and move to next step
  const handleApplyMappings = async () => {
    if (!session) return;

    try {
      // Submit review decisions
      await submitReview(
        session.session_id,
        Array.from(decisions.values()),
        userId,
        true // finalize = true
      );

      // Apply to RDF store
      await applyToRdf(session.session_id, true);

      // Move to next step with session ID
      onComplete(session.session_id);
    } catch (err) {
      // Error already handled by hook
    }
  };

  // Skip field mapping
  const handleSkip = () => {
    skip();
    onSkip();
  };

  // Get all field mappings
  const allMappings = session?.tables.flatMap(t => t.field_mappings) || [];

  // Check if ready to proceed
  const readyToApply = session && decisions.size > 0;

  // Debug logging
  console.log('[FieldMappingStep] State:', {
    enabled,
    currentStep,
    isAnalyzing,
    isReviewing,
    isApplying,
    hasSession: !!session,
    sessionStatus: session?.status,
    error: error?.message,
    mappingsCount: allMappings.length,
  });

  return (
    <div className="space-y-4">
      {/* Intro Card - Collapsed State */}
      {!enabled && (
        <Card className="border-2">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Sparkles className="h-5 w-5 text-primary" />
              Semantic Field Mapping (Optional)
            </CardTitle>
            <CardDescription>
              Map your fields to standard ontology terms for enhanced knowledge graph integration.
              AI will suggest mappings based on field names, types, and sample data.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <Alert className="bg-blue-50 border-blue-200">
              <Sparkles className="h-4 w-4 text-blue-600" />
              <AlertDescription className="text-blue-800 text-sm">
                <strong>Optional Feature:</strong> AI field mapping enhances your datasets with semantic annotations.
                You can skip this step and continue without mapping if needed.
              </AlertDescription>
            </Alert>

            <div className="bg-muted/50 rounded-lg p-4 space-y-2">
              <h4 className="font-medium text-sm">Benefits:</h4>
              <ul className="text-sm text-muted-foreground space-y-1 ml-4">
                <li>• Automatic semantic annotations</li>
                <li>• Better data integration across sources</li>
                <li>• Enhanced searchability and discoverability</li>
                <li>• Knowledge graph-ready datasets</li>
              </ul>
            </div>

            <div className="flex items-center space-x-2">
              <Checkbox
                id="auto-approve"
                checked={autoApproveThreshold === 0.95}
                onCheckedChange={(checked) => setAutoApproveThreshold(checked ? 0.95 : 1.0)}
              />
              <label
                htmlFor="auto-approve"
                className="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
              >
                Auto-approve high confidence mappings (≥95%)
              </label>
            </div>

            <div className="flex gap-3 pt-2">
              <Button onClick={handleEnable} className="flex-1">
                <Sparkles className="h-4 w-4 mr-2" />
                Enable AI Field Mapping
              </Button>
              <Button variant="outline" onClick={handleSkip}>
                Skip Mapping
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Analyzing State */}
      {enabled && isAnalyzing && (
        <Card>
          <CardContent className="pt-6">
            <div className="flex flex-col items-center justify-center py-12">
              <Loader2 className="h-12 w-12 animate-spin text-primary mb-4" />
              <h3 className="text-lg font-semibold mb-2">Analyzing Fields with AI...</h3>
              <p className="text-sm text-muted-foreground text-center max-w-md">
                Our AI is analyzing your table structure, field names, and sample data
                to suggest ontology mappings. This typically takes 2-5 seconds.
              </p>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Error State */}
      {error && (
        <Alert variant="destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {error.message.includes('Mapping engine not initialized') ||
             error.message.includes('not initialized') ? (
              <>
                <strong>Field Mapping Service Not Available</strong>
                <p className="mt-1 text-sm">
                  The mapping engine is not currently initialized on the backend server.
                  You can continue without field mapping and add semantic annotations later.
                </p>
                <div className="mt-3 flex gap-2">
                  <Button size="sm" variant="outline" onClick={handleSkip}>
                    Continue Without Mapping
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setEnabled(false)}>
                    Back
                  </Button>
                </div>
              </>
            ) : error.message.includes('404') || error.message.includes('Not Found') ? (
              <>
                <strong>API Endpoint Not Found</strong>
                <p className="mt-1 text-sm">
                  The field mapping endpoint was not found. This may indicate a version mismatch.
                  You can continue without field mapping for now.
                </p>
                <div className="mt-3 flex gap-2">
                  <Button size="sm" variant="outline" onClick={handleSkip}>
                    Continue Without Mapping
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setEnabled(false)}>
                    Back
                  </Button>
                </div>
              </>
            ) : (
              <>
                {error.message}
                <div className="mt-2 flex gap-2">
                  <Button size="sm" variant="outline" onClick={() => window.location.reload()}>
                    Try Again
                  </Button>
                  <Button size="sm" variant="ghost" onClick={handleSkip}>
                    Skip Mapping
                  </Button>
                </div>
              </>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Review State */}
      {enabled && session && isReviewing && (
        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Review Field Mappings</CardTitle>
              <CardDescription>
                AI has analyzed <strong>{tableName}</strong> from <strong>{datasource.name}</strong>.
                Review and approve the suggested ontology mappings below.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <FieldMappingTable
                mappings={allMappings}
                onActionChange={handleActionChange}
                onBulkApprove={handleBulkApprove}
                onBulkReject={handleBulkReject}
              />
            </CardContent>
          </Card>

          {/* Action Buttons */}
          <div className="flex justify-between">
            <Button variant="outline" onClick={handleSkip}>
              Skip & Continue Without Mapping
            </Button>
            <div className="flex gap-2">
              <Button
                variant="default"
                onClick={handleApplyMappings}
                disabled={!readyToApply || isApplying}
              >
                {isApplying ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Applying Mappings...
                  </>
                ) : (
                  <>
                    <Check className="h-4 w-4 mr-2" />
                    Apply Mappings ({decisions.size} fields)
                    <ArrowRight className="h-4 w-4 ml-2" />
                  </>
                )}
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Applying State */}
      {isApplying && (
        <Card>
          <CardContent className="pt-6">
            <div className="flex flex-col items-center justify-center py-8">
              <Loader2 className="h-10 w-10 animate-spin text-primary mb-3" />
              <h3 className="font-semibold mb-1">Applying Mappings to Knowledge Graph...</h3>
              <p className="text-sm text-muted-foreground">
                Storing semantic relationships as RDF triples
              </p>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
