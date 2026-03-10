/**
 * Enterprise Semantic Mapper Configuration Form
 *
 * Oracle Redwood × Microsoft Fluent design system
 * AI-powered field-to-ontology mapping with full backend integration
 *
 * Features:
 * - Beautiful ontology selector with search and metadata
 * - Enterprise field mapping table with AI suggestions
 * - Manual mapping dialog with ontology tree navigation
 * - Bulk operations for efficient review
 * - Real-time validation and progress indicators
 * - Complete backend integration
 */

import React, { useState, useMemo, useCallback, useEffect } from 'react';
import {
  Layers,
  AlertCircle,
  Search,
  CheckCircle2,
  XCircle,
  Edit2,
  ChevronRight,
  Info,
  Play,
  RotateCcw,
  CheckSquare,
  XSquare,
  Loader2,
  TrendingUp,
  TrendingDown,
  Minus,
  Eye,
  Zap,
  Filter,
  Download,
  ChevronsUpDown,
  Check,
  Sparkles,
  Trash2,
} from 'lucide-react';
import { useQuery, useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';

// UI Components
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Slider } from '@/components/ui/slider';
import { Badge } from '@/components/ui/badge';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Progress } from '@/components/ui/progress';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { Checkbox } from '@/components/ui/checkbox';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';

// API Imports
import { useOntologies, useOntologyTree } from '@/hooks/useOntologies';
import * as fieldMappingApi from '@/api/field-mapping';
import type {
  FieldMapping,
  MappingSession,
  OntologyCandidate,
  FieldMappingDecision,
  MappingAction,
} from '@/api/field-mapping';
import type { OntologyMetadata, PropertyNode } from '@/api/ontology';
import type { SemanticMapperConfig } from '@/lib/workflow-etl-config';

// ============================================================================
// Component Props
// ============================================================================

export interface SemanticMapperConfigFormProps {
  config?: SemanticMapperConfig;
  onUpdate: (updates: Partial<SemanticMapperConfig>) => void;
  nodeId?: string;
  datasourceId?: string; // Optional: for direct datasource analysis
  upstreamSchema?: Array<{
    name: string;
    type: string;
    sample_values?: string[];
  }>; // Schema from connected upstream nodes
}

// ============================================================================
// Confidence Level Utilities
// ============================================================================

function getConfidenceLevel(confidence: number): 'high' | 'medium' | 'low' {
  if (confidence >= 0.95) return 'high';
  if (confidence >= 0.70) return 'medium';
  return 'low';
}

function getConfidenceBadgeClass(confidence: number): string {
  const level = getConfidenceLevel(confidence);
  switch (level) {
    case 'high':
      return 'bg-[#3A7728] text-white border-[#3A7728]';
    case 'medium':
      return 'bg-[#F5A623] text-white border-[#F5A623]';
    case 'low':
      return 'bg-[#C74634] text-white border-[#C74634]';
  }
}

function getConfidenceIcon(confidence: number) {
  const level = getConfidenceLevel(confidence);
  switch (level) {
    case 'high':
      return <TrendingUp className="w-3 h-3" />;
    case 'medium':
      return <Minus className="w-3 h-3" />;
    case 'low':
      return <TrendingDown className="w-3 h-3" />;
  }
}

// ============================================================================
// Main Component
// ============================================================================

export function SemanticMapperConfigForm({
  config,
  onUpdate,
  nodeId,
  datasourceId,
  upstreamSchema = []
}: SemanticMapperConfigFormProps) {
  // ============================================================================
  // State Management
  // ============================================================================

  const [selectedOntologyIds, setSelectedOntologyIds] = useState<string[]>(
    config?.target_ontology || []
  );
  const [mappingMode, setMappingMode] = useState<'auto' | 'manual' | 'hybrid'>(
    config?.mapping_mode || 'hybrid'
  );
  const [autoApproveThreshold, setAutoApproveThreshold] = useState<number>(
    config?.auto_approve_threshold ?? 0.95
  );
  const [searchQuery, setSearchQuery] = useState('');
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [mappingSession, setMappingSession] = useState<MappingSession | null>(null);
  const [selectedFields, setSelectedFields] = useState<Set<string>>(new Set());
  const [showManualMappingDialog, setShowManualMappingDialog] = useState(false);
  const [currentFieldForMapping, setCurrentFieldForMapping] = useState<FieldMapping | null>(null);
  const [showDetailsSheet, setShowDetailsSheet] = useState(false);
  const [filterStatus, setFilterStatus] = useState<'all' | 'pending' | 'approved' | 'rejected'>('all');

  // Manual mapping state (for direct field mapping in manual mode)
  const [manualMappings, setManualMappings] = useState<Map<string, {
    uri: string;
    label: string;
    confidence: number;
  }>>(new Map());

  // Load manual mappings from saved config when component mounts or config changes
  // Use field_mappings as dependency to ensure it triggers when mappings are updated
  useEffect(() => {
    console.log('[SemanticMapper] Loading manual mappings from config:', config?.field_mappings);
    if (config?.field_mappings && config.field_mappings.length > 0) {
      const loadedMappings = new Map(
        config.field_mappings.map(fm => [
          fm.source_field,
          {
            uri: fm.ontology_term,
            label: fm.ontology_term.split('/').pop() || fm.ontology_term, // Extract label from URI
            confidence: fm.confidence,
          }
        ])
      );
      console.log('[SemanticMapper] Loaded mappings:', loadedMappings);
      setManualMappings(loadedMappings);
    } else if (config && (!config.field_mappings || config.field_mappings.length === 0)) {
      // If config exists but has no mappings, clear the local state
      console.log('[SemanticMapper] No mappings in config, clearing state');
      setManualMappings(new Map());
    }
  }, [config?.field_mappings]);

  // ============================================================================
  // Data Fetching
  // ============================================================================

  // Fetch all ontologies
  const { data: ontologies = [], isLoading: isLoadingOntologies } = useOntologies(true);

  // Fetch ontology tree for manual mapping
  const { data: ontologyTree } = useOntologyTree(
    selectedOntologyIds[0], // Use first selected ontology for now
    {
      includeProperties: true,
      includeIndividuals: false,
      maxDepth: 10,
    }
  );

  // ============================================================================
  // Computed Values
  // ============================================================================

  const filteredOntologies = useMemo(() => {
    if (!searchQuery) return ontologies;
    const query = searchQuery.toLowerCase();
    return ontologies.filter(
      (ont) =>
        ont.name.toLowerCase().includes(query) ||
        ont.namespace.toLowerCase().includes(query) ||
        ont.tags.some((tag) => tag.toLowerCase().includes(query))
    );
  }, [ontologies, searchQuery]);

  const selectedOntologies = useMemo(
    () => ontologies.filter((ont) => selectedOntologyIds.includes(ont.id)),
    [ontologies, selectedOntologyIds]
  );

  const fieldMappings = useMemo(() => {
    if (!mappingSession) return [];
    return mappingSession.tables.flatMap((table) => table.field_mappings);
  }, [mappingSession]);

  const filteredFieldMappings = useMemo(() => {
    let filtered = fieldMappings;

    // Filter by status
    if (filterStatus !== 'all') {
      filtered = filtered.filter((field) => {
        switch (filterStatus) {
          case 'pending':
            return field.approval_status === 'pending';
          case 'approved':
            return field.approval_status === 'approved' || field.approval_status === 'auto_approved';
          case 'rejected':
            return field.approval_status === 'rejected';
          default:
            return true;
        }
      });
    }

    return filtered;
  }, [fieldMappings, filterStatus]);

  const sessionSummary = useMemo(() => {
    if (!mappingSession) return null;
    return mappingSession.summary;
  }, [mappingSession]);

  const completionPercentage = useMemo(() => {
    if (!sessionSummary) return 0;
    const { total_fields, auto_approved, user_approved, rejected } = sessionSummary;
    if (total_fields === 0) return 0;
    const completed = auto_approved + user_approved + rejected;
    return Math.round((completed / total_fields) * 100);
  }, [sessionSummary]);

  const canAnalyze = useMemo(() => {
    return (
      selectedOntologyIds.length > 0 &&
      datasourceId &&
      !isAnalyzing &&
      !mappingSession
    );
  }, [selectedOntologyIds, datasourceId, isAnalyzing, mappingSession]);

  const canSave = useMemo(() => {
    return (
      mappingSession &&
      sessionSummary &&
      (sessionSummary.auto_approved + sessionSummary.user_approved) > 0
    );
  }, [mappingSession, sessionSummary]);

  // Detect when to show manual mapping mode immediately
  const showManualMappingMode = useMemo(() => {
    return (
      mappingMode === 'manual' &&
      upstreamSchema.length > 0 &&
      selectedOntologyIds.length > 0 &&
      !mappingSession
    );
  }, [mappingMode, upstreamSchema, selectedOntologyIds, mappingSession]);

  // Manual mode: calculate completion
  const manualMappingProgress = useMemo(() => {
    if (!showManualMappingMode && !mappingSession) return { mapped: 0, total: 0, percentage: 0 };

    const total = upstreamSchema.length || 0;
    const mapped = manualMappings.size;
    const percentage = total > 0 ? Math.round((mapped / total) * 100) : 0;

    return { mapped, total, percentage };
  }, [showManualMappingMode, mappingSession, upstreamSchema, manualMappings]);

  // ============================================================================
  // Event Handlers
  // ============================================================================

  const handleOntologyToggle = useCallback((ontologyId: string) => {
    setSelectedOntologyIds((prev) => {
      const newIds = prev.includes(ontologyId)
        ? prev.filter((id) => id !== ontologyId)
        : [...prev, ontologyId];

      onUpdate({ target_ontology: newIds });
      return newIds;
    });
  }, [onUpdate]);

  const handleMappingModeChange = useCallback((mode: 'auto' | 'manual' | 'hybrid') => {
    setMappingMode(mode);
    onUpdate({ mapping_mode: mode });
  }, [onUpdate]);

  const handleThresholdChange = useCallback((value: number[]) => {
    const threshold = value[0] / 100;
    setAutoApproveThreshold(threshold);
    onUpdate({ auto_approve_threshold: threshold });
  }, [onUpdate]);

  const handleAnalyzeFields = useCallback(async () => {
    // Validate requirements
    if (selectedOntologyIds.length === 0) {
      toast.error('Missing configuration', {
        description: 'Please select at least one ontology.',
      });
      return;
    }

    if (!datasourceId && upstreamSchema.length === 0) {
      toast.error('Missing data source', {
        description: 'Please connect this node to a datasource or upstream transformation node.',
      });
      return;
    }

    setIsAnalyzing(true);

    try {
      // If we have a direct datasourceId, use the backend API
      if (datasourceId) {
        // Step 1: Start analysis
        const analysisResponse = await fieldMappingApi.analyzeForMapping(datasourceId, {
          ontology_namespaces: selectedOntologies.map((ont) => ont.namespace),
          auto_approve_threshold: autoApproveThreshold,
          min_confidence: 0.5,
          sample_size: 1000,
          user_id: 'current-user', // TODO: Get from auth context
        });

        toast.success('Field analysis started', {
          description: `Analyzing ${analysisResponse.summary.total_fields} fields...`,
        });

        // Step 2: Fetch full session details
        const sessionResponse = await fieldMappingApi.getMappingSession(
          analysisResponse.session_id
        );

        setMappingSession(sessionResponse);

        onUpdate({
          mapping_session_id: analysisResponse.session_id,
        });

        toast.success('Analysis complete', {
          description: `Found ${analysisResponse.summary.fields_with_candidates} candidate mappings`,
        });
      } else if (upstreamSchema.length > 0) {
        // Use upstream schema - transform to backend API format and get AI suggestions
        toast.info('Analyzing upstream fields', {
          description: `Analyzing ${upstreamSchema.length} fields with AI...`,
        });

        // Transform upstreamSchema to backend field format
        const fields = upstreamSchema.map(field => ({
          name: field.name,
          data_type: field.type,
          sample_values: field.sample_values || [],
        }));

        // Call backend API with upstream fields
        // Use a temporary datasource ID for upstream workflow fields
        const analysisResponse = await fieldMappingApi.analyzeForMapping('upstream-workflow', {
          tables: ['upstream_fields'],
          fields: fields,
          ontology_namespaces: selectedOntologies.map((ont) => ont.namespace),
          auto_approve_threshold: autoApproveThreshold,
          min_confidence: 0.5,
          sample_size: upstreamSchema.length,
          user_id: 'current-user',
        });

        toast.success('Field analysis started', {
          description: `Analyzing ${analysisResponse.summary.total_fields} fields...`,
        });

        // Fetch full session details with AI suggestions
        const sessionResponse = await fieldMappingApi.getMappingSession(
          analysisResponse.session_id
        );

        setMappingSession(sessionResponse);

        onUpdate({
          mapping_session_id: analysisResponse.session_id,
        });

        toast.success('Analysis complete', {
          description: `Found ${analysisResponse.summary.fields_with_candidates} candidate mappings`,
        });
      }
    } catch (error: any) {
      toast.error('Analysis failed', {
        description: error.message || 'Failed to analyze fields for mapping',
      });
    } finally {
      setIsAnalyzing(false);
    }
  }, [datasourceId, upstreamSchema, selectedOntologyIds, selectedOntologies, autoApproveThreshold, onUpdate]);

  const handleApproveField = useCallback((field: FieldMapping) => {
    if (!field.candidates[0]) return;

    setMappingSession((prev) => {
      if (!prev) return prev;

      const updatedTables = prev.tables.map((table) => ({
        ...table,
        field_mappings: table.field_mappings.map((fm) =>
          fm.field_id === field.field_id
            ? {
                ...fm,
                approval_status: 'approved' as const,
                selected_mapping: {
                  ontology_term_uri: field.candidates[0].ontology_term_uri,
                  confidence: field.candidates[0].confidence,
                  was_top_candidate: true,
                },
              }
            : fm
        ),
      }));

      return {
        ...prev,
        tables: updatedTables,
        summary: {
          ...prev.summary,
          user_approved: prev.summary.user_approved + 1,
          pending_review: Math.max(0, prev.summary.pending_review - 1),
        },
      };
    });
  }, []);

  const handleRejectField = useCallback((field: FieldMapping) => {
    setMappingSession((prev) => {
      if (!prev) return prev;

      const updatedTables = prev.tables.map((table) => ({
        ...table,
        field_mappings: table.field_mappings.map((fm) =>
          fm.field_id === field.field_id
            ? {
                ...fm,
                approval_status: 'rejected' as const,
                selected_mapping: undefined,
              }
            : fm
        ),
      }));

      return {
        ...prev,
        tables: updatedTables,
        summary: {
          ...prev.summary,
          rejected: prev.summary.rejected + 1,
          pending_review: Math.max(0, prev.summary.pending_review - 1),
        },
      };
    });
  }, []);

  const handleManualSelect = useCallback((field: FieldMapping) => {
    setCurrentFieldForMapping(field);
    setShowManualMappingDialog(true);
  }, []);

  const handleManualMappingConfirm = useCallback((property: PropertyNode) => {
    if (!currentFieldForMapping) return;

    setMappingSession((prev) => {
      if (!prev) return prev;

      const updatedTables = prev.tables.map((table) => ({
        ...table,
        field_mappings: table.field_mappings.map((fm) =>
          fm.field_id === currentFieldForMapping.field_id
            ? {
                ...fm,
                approval_status: 'modified' as const,
                selected_mapping: {
                  ontology_term_uri: property.uri,
                  confidence: 1.0, // Manual selection = 100% confidence
                  was_top_candidate: false,
                },
              }
            : fm
        ),
      }));

      return {
        ...prev,
        tables: updatedTables,
        summary: {
          ...prev.summary,
          modified: prev.summary.modified + 1,
          pending_review: Math.max(0, prev.summary.pending_review - 1),
        },
      };
    });

    setShowManualMappingDialog(false);
    setCurrentFieldForMapping(null);

    toast.success('Manual mapping applied', {
      description: `${currentFieldForMapping.field_name} → ${property.label}`,
    });
  }, [currentFieldForMapping]);

  const handleBulkApproveHighConfidence = useCallback(() => {
    const highConfidenceFields = fieldMappings.filter(
      (field) =>
        field.approval_status === 'pending' &&
        field.candidates[0] &&
        field.candidates[0].confidence >= 0.95
    );

    if (highConfidenceFields.length === 0) {
      toast.info('No high-confidence fields to approve');
      return;
    }

    highConfidenceFields.forEach(handleApproveField);

    toast.success(`Approved ${highConfidenceFields.length} high-confidence mappings`);
  }, [fieldMappings, handleApproveField]);

  const handleBulkRejectLowConfidence = useCallback(() => {
    const lowConfidenceFields = fieldMappings.filter(
      (field) =>
        field.approval_status === 'pending' &&
        field.candidates[0] &&
        field.candidates[0].confidence < 0.70
    );

    if (lowConfidenceFields.length === 0) {
      toast.info('No low-confidence fields to reject');
      return;
    }

    lowConfidenceFields.forEach(handleRejectField);

    toast.success(`Rejected ${lowConfidenceFields.length} low-confidence mappings`);
  }, [fieldMappings, handleRejectField]);

  const handleResetAll = useCallback(() => {
    setMappingSession(null);
    setSelectedFields(new Set());
    setFilterStatus('all');
    onUpdate({
      mapping_session_id: undefined,
      field_mappings: [],
    });
    toast.info('Mappings reset');
  }, [onUpdate]);

  const handleSaveMappings = useCallback(async () => {
    if (!mappingSession) return;

    try {
      // Build field decisions
      const decisions: FieldMappingDecision[] = fieldMappings
        .filter((field) => field.selected_mapping)
        .map((field) => ({
          field_id: field.field_id,
          action: (field.approval_status === 'modified' ? 'modify' : 'approve') as MappingAction,
          selected_mapping: field.selected_mapping!.ontology_term_uri,
        }));

      // Submit review
      await fieldMappingApi.reviewMappings(mappingSession.session_id, {
        field_mappings: decisions,
        reviewed_by: 'current-user', // TODO: Get from auth context
        finalize: true,
      });

      // Update workflow config
      const finalMappings = fieldMappings
        .filter((field) => field.selected_mapping)
        .map((field) => ({
          source_field: field.field_name,
          ontology_term: field.selected_mapping!.ontology_term_uri,
          confidence: field.selected_mapping!.confidence,
        }));

      onUpdate({
        field_mappings: finalMappings,
      });

      toast.success('Mappings saved successfully', {
        description: `${finalMappings.length} field mappings configured`,
      });
    } catch (error: any) {
      toast.error('Failed to save mappings', {
        description: error.message,
      });
    }
  }, [mappingSession, fieldMappings, onUpdate]);

  // Manual mapping handlers
  const handleManualFieldMapping = useCallback((fieldName: string, property: { uri: string; label: string } | null) => {
    setManualMappings((prev) => {
      const newMap = new Map(prev);
      if (property) {
        newMap.set(fieldName, {
          uri: property.uri,
          label: property.label,
          confidence: 1.0, // Manual mapping = 100% confidence
        });
      } else {
        newMap.delete(fieldName);
      }
      return newMap;
    });
  }, []);

  const handleSaveManualMappings = useCallback(() => {
    if (manualMappings.size === 0) {
      toast.error('No mappings to save', {
        description: 'Please map at least one field before saving.',
      });
      return;
    }

    // Convert manual mappings to config format
    const finalMappings = Array.from(manualMappings.entries()).map(([fieldName, mapping]) => ({
      source_field: fieldName,
      ontology_term: mapping.uri,
      confidence: mapping.confidence,
    }));

    console.log('[SemanticMapper] Saving manual mappings:', finalMappings);

    onUpdate({
      field_mappings: finalMappings,
    });

    toast.success('Manual mappings saved', {
      description: `${finalMappings.length} field mappings configured`,
    });

    // Don't reset state - let useEffect handle loading from config
    // The mappings should persist in the UI after saving
  }, [manualMappings, onUpdate]);

  const handleClearAllManualMappings = useCallback(() => {
    setManualMappings(new Map());
    toast.info('All mappings cleared');
  }, []);

  // ============================================================================
  // Render
  // ============================================================================

  return (
    <TooltipProvider>
      <div className="flex flex-col bg-neutral-50 dark:bg-neutral-900">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 bg-background dark:bg-neutral-800 border-b border-border dark:border-white/10">
          <Layers className="w-4 h-4 text-[#00CC6A]" />
          <h3 className="text-sm font-semibold text-foreground dark:text-neutral-100">Semantic Mapper Configuration</h3>
          {mappingSession && (
            <Badge variant="outline" className="ml-auto text-xs">
              Session: {mappingSession.session_id.slice(0, 8)}
            </Badge>
          )}
        </div>

        <div className="p-4 space-y-6">
            {/* Phase 1: Configuration */}
            {!mappingSession && (
              <>
                {/* Ontology Selector */}
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <Label className="text-xs font-semibold text-foreground">
                      Target Ontologies <span className="text-red-500">*</span>
                    </Label>
                    <span className="text-xs text-muted-foreground">
                      {selectedOntologyIds.length} selected
                    </span>
                  </div>

                  {/* Search */}
                  <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                    <Input
                      type="text"
                      placeholder="Search ontologies by name, namespace, or tags..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      className="pl-9 text-sm"
                    />
                  </div>

                  {/* Ontology List */}
                  <div className="border border-border rounded-lg bg-background max-h-64 overflow-y-auto">
                    {isLoadingOntologies ? (
                      <div className="flex items-center justify-center py-8">
                        <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
                      </div>
                    ) : filteredOntologies.length === 0 ? (
                      <div className="flex flex-col items-center justify-center py-8 text-center">
                        <AlertCircle className="w-8 h-8 text-muted-foreground mb-2" />
                        <p className="text-sm text-muted-foreground">No ontologies found</p>
                        <p className="text-xs text-muted-foreground mt-1">
                          {searchQuery ? 'Try a different search term' : 'Register an ontology first'}
                        </p>
                      </div>
                    ) : (
                      <div className="divide-y divide-black/5">
                        {filteredOntologies.map((ontology) => (
                          <OntologyListItem
                            key={ontology.id}
                            ontology={ontology}
                            isSelected={selectedOntologyIds.includes(ontology.id)}
                            onToggle={handleOntologyToggle}
                          />
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {/* Mapping Mode */}
                <div className="space-y-2">
                  <Label className="text-xs font-semibold text-foreground">Mapping Mode</Label>
                  <Select value={mappingMode} onValueChange={handleMappingModeChange}>
                    <SelectTrigger className="text-sm">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="auto">
                        <div className="flex items-center gap-2">
                          <Zap className="w-3 h-3" />
                          <span>Auto - AI suggests and applies mappings</span>
                        </div>
                      </SelectItem>
                      <SelectItem value="manual">
                        <div className="flex items-center gap-2">
                          <Edit2 className="w-3 h-3" />
                          <span>Manual - User approves all mappings</span>
                        </div>
                      </SelectItem>
                      <SelectItem value="hybrid">
                        <div className="flex items-center gap-2">
                          <CheckCircle2 className="w-3 h-3" />
                          <span>Hybrid - Auto-approve high confidence</span>
                        </div>
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {/* Auto-approve Threshold */}
                {(mappingMode === 'auto' || mappingMode === 'hybrid') && (
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <Label className="text-xs font-semibold text-foreground">
                        Auto-approve Threshold
                      </Label>
                      <Badge variant="outline" className="text-xs font-mono">
                        {Math.round(autoApproveThreshold * 100)}%
                      </Badge>
                    </div>
                    <Slider
                      value={[autoApproveThreshold * 100]}
                      onValueChange={handleThresholdChange}
                      min={50}
                      max={100}
                      step={5}
                      className="w-full"
                    />
                    <div className="flex justify-between text-xs text-muted-foreground">
                      <span>50% - Low confidence</span>
                      <span>95% - High confidence</span>
                    </div>
                    <p className="text-xs text-muted-foreground">
                      Mappings with confidence above this threshold will be automatically approved
                    </p>
                  </div>
                )}

                {/* Configuration Summary */}
                {selectedOntologyIds.length > 0 && (
                  <div className="p-4 bg-blue-50 border border-blue-200 rounded-lg space-y-2">
                    <div className="text-xs font-semibold text-blue-900">Configuration Summary</div>
                    <div className="space-y-1 text-xs text-blue-700">
                      <div className="flex justify-between">
                        <span>Ontologies:</span>
                        <span className="font-medium">{selectedOntologyIds.length}</span>
                      </div>
                      <div className="flex justify-between">
                        <span>Mode:</span>
                        <span className="font-medium capitalize">{mappingMode}</span>
                      </div>
                      {(mappingMode === 'auto' || mappingMode === 'hybrid') && (
                        <div className="flex justify-between">
                          <span>Threshold:</span>
                          <span className="font-medium">{Math.round(autoApproveThreshold * 100)}%</span>
                        </div>
                      )}
                    </div>
                    <Separator className="my-2" />
                    <div className="text-xs text-blue-600 space-y-1">
                      {selectedOntologies.map((ont) => (
                        <div key={ont.id} className="flex items-center gap-1">
                          <ChevronRight className="w-3 h-3" />
                          <span className="font-mono">{ont.namespace}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* Analyze Button - Only show for Auto/Hybrid modes or when no upstream schema */}
                {!showManualMappingMode && (
                  <Button
                    onClick={handleAnalyzeFields}
                    disabled={!canAnalyze}
                    className="w-full"
                    size="lg"
                  >
                    {isAnalyzing ? (
                      <>
                        <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                        Analyzing Fields...
                      </>
                    ) : (
                      <>
                        <Play className="w-4 h-4 mr-2" />
                        Analyze Fields
                      </>
                    )}
                  </Button>
                )}

                {/* Upstream Field Detection Info */}
                {upstreamSchema.length > 0 && !showManualMappingMode && (
                  <div className="flex items-start gap-2 p-3 bg-blue-50 border border-blue-200 rounded text-xs">
                    <Info className="w-4 h-4 text-blue-600 flex-shrink-0 mt-0.5" />
                    <div className="text-blue-800">
                      <div className="font-medium mb-1">
                        Detected {upstreamSchema.length} fields from upstream nodes
                      </div>
                      <div className="text-blue-700">
                        Fields: {upstreamSchema.slice(0, 5).map(f => f.name).join(', ')}
                        {upstreamSchema.length > 5 && ` +${upstreamSchema.length - 5} more`}
                      </div>
                    </div>
                  </div>
                )}

                {!datasourceId && upstreamSchema.length === 0 && !mappingSession && (
                  <div className="flex items-start gap-2 p-3 bg-amber-50 border border-amber-200 rounded text-xs">
                    <AlertCircle className="w-4 h-4 text-amber-600 flex-shrink-0 mt-0.5" />
                    <div className="text-amber-800">
                      <div className="font-medium mb-1">No fields detected</div>
                      <div>
                        Please connect this node to a datasource and ensure it's configured:
                      </div>
                      <ul className="list-disc list-inside mt-1 space-y-0.5 text-amber-700">
                        <li>CSV Source: Click "Scan File" to detect fields</li>
                        <li>DB Extract: Configure connection and table</li>
                        <li>Or connect to an upstream transformation node</li>
                      </ul>
                    </div>
                  </div>
                )}
              </>
            )}

            {/* Manual Mapping Mode - Show fields immediately */}
            {showManualMappingMode && (
              <>
                <Separator />

                {/* Progress Summary */}
                <div className="p-4 bg-card dark:bg-card border border-border rounded-lg space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="text-sm font-semibold text-foreground">Field Mappings</div>
                    <Badge variant="outline" className="text-xs">
                      {manualMappingProgress.mapped}/{manualMappingProgress.total} mapped
                    </Badge>
                  </div>

                  <Progress value={manualMappingProgress.percentage} className="h-2" />

                  <p className="text-xs text-muted-foreground">
                    Select ontology terms for each field. At least one field must be mapped to save.
                  </p>
                </div>

                {/* Manual Mapping Table */}
                <div className="border border-border rounded-lg bg-background overflow-hidden">
                  <div className="overflow-x-auto">
                    <table className="w-full text-xs">
                      <thead className="bg-neutral-100 border-b border-border">
                        <tr>
                          <th className="px-3 py-2 text-left font-semibold text-foreground">
                            Field Name
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground w-24">
                            Type
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground">
                            Ontology Term
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground w-24">
                            Status
                          </th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-black/5">
                        {upstreamSchema.map((field) => (
                          <ManualMappingRow
                            key={field.name}
                            field={field}
                            ontologyTree={ontologyTree}
                            selectedOntologies={selectedOntologies}
                            mapping={manualMappings.get(field.name)}
                            onMappingChange={handleManualFieldMapping}
                          />
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>

                {/* Action Buttons */}
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleClearAllManualMappings}
                    disabled={manualMappings.size === 0}
                  >
                    <Trash2 className="w-3 h-3 mr-1" />
                    Clear All
                  </Button>
                  <Button
                    onClick={handleSaveManualMappings}
                    disabled={manualMappings.size === 0}
                    className="ml-auto"
                    size="lg"
                  >
                    <Download className="w-4 h-4 mr-2" />
                    Save Mappings ({manualMappings.size})
                  </Button>
                </div>
              </>
            )}

            {/* Phase 2: Field Mapping Review */}
            {mappingSession && (
              <>
                {/* Session Summary */}
                <div className="p-4 bg-card dark:bg-card border border-border rounded-lg space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="text-sm font-semibold text-foreground">Mapping Progress</div>
                    <Badge variant="outline" className="text-xs">
                      {completionPercentage}% Complete
                    </Badge>
                  </div>

                  <Progress value={completionPercentage} className="h-2" />

                  <div className="grid grid-cols-2 gap-3 text-xs">
                    <div className="space-y-1">
                      <div className="text-muted-foreground">Total Fields</div>
                      <div className="text-lg font-semibold text-foreground">
                        {sessionSummary?.total_fields || 0}
                      </div>
                    </div>
                    <div className="space-y-1">
                      <div className="text-muted-foreground">With Candidates</div>
                      <div className="text-lg font-semibold text-foreground">
                        {sessionSummary?.fields_with_candidates || 0}
                      </div>
                    </div>
                    <div className="space-y-1">
                      <div className="flex items-center gap-1 text-[#3A7728]">
                        <CheckCircle2 className="w-3 h-3" />
                        <span>Auto-approved</span>
                      </div>
                      <div className="text-lg font-semibold text-[#3A7728]">
                        {sessionSummary?.auto_approved || 0}
                      </div>
                    </div>
                    <div className="space-y-1">
                      <div className="flex items-center gap-1 text-[#F5A623]">
                        <AlertCircle className="w-3 h-3" />
                        <span>Needs Review</span>
                      </div>
                      <div className="text-lg font-semibold text-[#F5A623]">
                        {sessionSummary?.pending_review || 0}
                      </div>
                    </div>
                    <div className="space-y-1">
                      <div className="flex items-center gap-1 text-[#0078D4]">
                        <CheckCircle2 className="w-3 h-3" />
                        <span>User Approved</span>
                      </div>
                      <div className="text-lg font-semibold text-[#0078D4]">
                        {sessionSummary?.user_approved || 0}
                      </div>
                    </div>
                    <div className="space-y-1">
                      <div className="flex items-center gap-1 text-[#C74634]">
                        <XCircle className="w-3 h-3" />
                        <span>Rejected</span>
                      </div>
                      <div className="text-lg font-semibold text-[#C74634]">
                        {sessionSummary?.rejected || 0}
                      </div>
                    </div>
                  </div>
                </div>

                {/* Command Bar */}
                <div className="flex items-center gap-2 p-2 bg-card dark:bg-card border border-border rounded-lg">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleBulkApproveHighConfidence}
                    className="text-xs"
                  >
                    <CheckSquare className="w-3 h-3 mr-1" />
                    Approve High Confidence
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleBulkRejectLowConfidence}
                    className="text-xs"
                  >
                    <XSquare className="w-3 h-3 mr-1" />
                    Reject Low Confidence
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleResetAll}
                    className="text-xs"
                  >
                    <RotateCcw className="w-3 h-3 mr-1" />
                    Reset All
                  </Button>
                  <div className="ml-auto flex items-center gap-2">
                    <Select value={filterStatus} onValueChange={(v: any) => setFilterStatus(v)}>
                      <SelectTrigger className="w-32 h-7 text-xs">
                        <Filter className="w-3 h-3 mr-1" />
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="all">All Fields</SelectItem>
                        <SelectItem value="pending">Pending</SelectItem>
                        <SelectItem value="approved">Approved</SelectItem>
                        <SelectItem value="rejected">Rejected</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>

                {/* Field Mapping Table */}
                <div className="border border-border rounded-lg bg-background overflow-hidden">
                  <div className="overflow-x-auto">
                    <table className="w-full text-xs">
                      <thead className="bg-neutral-100 border-b border-border">
                        <tr>
                          <th className="px-3 py-2 text-left font-semibold text-foreground w-8">
                            <Checkbox />
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground">
                            Field Name
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground">
                            Type
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground">
                            AI Suggestion
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground w-24">
                            Confidence
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground w-24">
                            Status
                          </th>
                          <th className="px-3 py-2 text-left font-semibold text-foreground w-32">
                            Actions
                          </th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-black/5">
                        {filteredFieldMappings.length === 0 ? (
                          <tr>
                            <td colSpan={7} className="px-3 py-8 text-center text-muted-foreground">
                              No fields found matching current filter
                            </td>
                          </tr>
                        ) : (
                          filteredFieldMappings.map((field) => (
                            <FieldMappingRow
                              key={field.field_id}
                              field={field}
                              onApprove={handleApproveField}
                              onReject={handleRejectField}
                              onManualSelect={handleManualSelect}
                            />
                          ))
                        )}
                      </tbody>
                    </table>
                  </div>
                </div>

                {/* Save Button */}
                <div className="flex items-center gap-2">
                  <Button
                    onClick={handleSaveMappings}
                    disabled={!canSave}
                    className="flex-1"
                    size="lg"
                  >
                    <Download className="w-4 h-4 mr-2" />
                    Save Mappings ({(sessionSummary?.auto_approved || 0) + (sessionSummary?.user_approved || 0)} fields)
                  </Button>
                </div>
              </>
            )}
          </div>
        </div>

      {/* Manual Mapping Dialog */}
      <ManualMappingDialog
          open={showManualMappingDialog}
          onOpenChange={setShowManualMappingDialog}
          field={currentFieldForMapping}
          ontologyTree={ontologyTree}
          onConfirm={handleManualMappingConfirm}
        />
    </TooltipProvider>
  );
}

// ============================================================================
// Sub-Components
// ============================================================================

interface OntologyListItemProps {
  ontology: OntologyMetadata;
  isSelected: boolean;
  onToggle: (id: string) => void;
}

function OntologyListItem({ ontology, isSelected, onToggle }: OntologyListItemProps) {
  return (
    <div
      className={`
        px-3 py-3 cursor-pointer hover:bg-muted transition-colors
        ${isSelected ? 'bg-blue-50' : ''}
      `}
      onClick={() => onToggle(ontology.id)}
    >
      <div className="flex items-start gap-3">
        <Checkbox checked={isSelected} className="mt-0.5" />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-foreground">{ontology.name}</span>
            <Badge variant="outline" className="text-xs">
              v{ontology.version}
            </Badge>
          </div>
          <div className="text-xs text-muted-foreground font-mono mt-0.5 truncate">
            {ontology.namespace}
          </div>
          {ontology.description && (
            <div className="text-xs text-muted-foreground mt-1 line-clamp-2">
              {ontology.description}
            </div>
          )}
          {ontology.tags.length > 0 && (
            <div className="flex flex-wrap gap-1 mt-2">
              {ontology.tags.map((tag) => (
                <Badge key={tag} variant="secondary" className="text-xs">
                  {tag}
                </Badge>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

interface FieldMappingRowProps {
  field: FieldMapping;
  onApprove: (field: FieldMapping) => void;
  onReject: (field: FieldMapping) => void;
  onManualSelect: (field: FieldMapping) => void;
}

function FieldMappingRow({ field, onApprove, onReject, onManualSelect }: FieldMappingRowProps) {
  const topCandidate = field.candidates[0];
  const hasMapping = field.selected_mapping || topCandidate;

  return (
    <tr className="hover:bg-muted">
      <td className="px-3 py-2">
        <Checkbox />
      </td>
      <td className="px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="font-mono font-medium text-foreground">{field.field_name}</span>
          <Tooltip>
            <TooltipTrigger>
              <Info className="w-3 h-3 text-muted-foreground" />
            </TooltipTrigger>
            <TooltipContent>
              <div className="text-xs space-y-1">
                <div className="font-semibold">Sample Values:</div>
                {field.sample_values.slice(0, 5).map((val, i) => (
                  <div key={i} className="font-mono text-muted-foreground">{val}</div>
                ))}
              </div>
            </TooltipContent>
          </Tooltip>
        </div>
      </td>
      <td className="px-3 py-2">
        <Badge variant="outline" className="text-xs font-mono">
          {field.data_type}
        </Badge>
      </td>
      <td className="px-3 py-2">
        {hasMapping ? (
          <div className="space-y-1">
            <div className="font-mono text-xs text-blue-600 truncate max-w-xs">
              {field.selected_mapping?.ontology_term_uri || topCandidate?.ontology_term_uri}
            </div>
            {topCandidate && (
              <div className="text-xs text-muted-foreground truncate max-w-xs">
                {topCandidate.explanation}
              </div>
            )}
          </div>
        ) : (
          <span className="text-xs text-muted-foreground">No candidates</span>
        )}
      </td>
      <td className="px-3 py-2">
        {topCandidate && (
          <Badge className={getConfidenceBadgeClass(topCandidate.confidence)}>
            <span className="flex items-center gap-1">
              {getConfidenceIcon(topCandidate.confidence)}
              {Math.round(topCandidate.confidence * 100)}%
            </span>
          </Badge>
        )}
      </td>
      <td className="px-3 py-2">
        <StatusBadge status={field.approval_status} />
      </td>
      <td className="px-3 py-2">
        <div className="flex items-center gap-1">
          {field.approval_status === 'pending' && topCandidate && (
            <>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => onApprove(field)}
                    className="h-6 w-6 p-0"
                  >
                    <CheckCircle2 className="w-3 h-3 text-green-600" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Approve</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => onReject(field)}
                    className="h-6 w-6 p-0"
                  >
                    <XCircle className="w-3 h-3 text-red-600" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Reject</TooltipContent>
              </Tooltip>
            </>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onManualSelect(field)}
                className="h-6 w-6 p-0"
              >
                <Edit2 className="w-3 h-3 text-blue-600" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Manual Select</TooltipContent>
          </Tooltip>
        </div>
      </td>
    </tr>
  );
}

function StatusBadge({ status }: { status: string }) {
  switch (status) {
    case 'pending':
      return <Badge variant="outline" className="text-xs bg-amber-50 text-amber-700 border-amber-200">Pending</Badge>;
    case 'auto_approved':
      return <Badge variant="outline" className="text-xs bg-green-50 text-green-700 border-green-200">Auto</Badge>;
    case 'approved':
      return <Badge variant="outline" className="text-xs bg-blue-50 text-blue-700 border-blue-200">Approved</Badge>;
    case 'rejected':
      return <Badge variant="outline" className="text-xs bg-red-50 text-red-700 border-red-200">Rejected</Badge>;
    case 'modified':
      return <Badge variant="outline" className="text-xs bg-purple-50 text-purple-700 border-purple-200">Modified</Badge>;
    default:
      return <Badge variant="outline" className="text-xs">{status}</Badge>;
  }
}

interface ManualMappingDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  field: FieldMapping | null;
  ontologyTree: any;
  onConfirm: (property: PropertyNode) => void;
}

function ManualMappingDialog({
  open,
  onOpenChange,
  field,
  ontologyTree,
  onConfirm,
}: ManualMappingDialogProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedProperty, setSelectedProperty] = useState<PropertyNode | null>(null);

  const properties = useMemo(() => {
    if (!ontologyTree) return [];
    return ontologyTree.root_properties || [];
  }, [ontologyTree]);

  const filteredProperties = useMemo(() => {
    if (!searchQuery) return properties;
    const query = searchQuery.toLowerCase();
    return properties.filter(
      (prop: PropertyNode) =>
        prop.label.toLowerCase().includes(query) ||
        prop.uri.toLowerCase().includes(query) ||
        prop.comment?.toLowerCase().includes(query)
    );
  }, [properties, searchQuery]);

  const handleConfirm = () => {
    if (selectedProperty) {
      onConfirm(selectedProperty);
      setSearchQuery('');
      setSelectedProperty(null);
    }
  };

  if (!field) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Manual Ontology Mapping</DialogTitle>
          <DialogDescription>
            Select an ontology property for field: <span className="font-mono font-medium">{field.field_name}</span>
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 flex-1 overflow-hidden flex flex-col">
          {/* Search */}
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              type="text"
              placeholder="Search properties..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9"
            />
          </div>

          {/* Properties List */}
          <ScrollArea className="flex-1 border border-border rounded-lg">
            <div className="divide-y divide-black/5">
              {filteredProperties.length === 0 ? (
                <div className="p-8 text-center text-muted-foreground text-sm">
                  No properties found
                </div>
              ) : (
                filteredProperties.map((property: PropertyNode) => (
                  <div
                    key={property.uri}
                    className={`
                      p-3 cursor-pointer hover:bg-muted transition-colors
                      ${selectedProperty?.uri === property.uri ? 'bg-blue-50 border-l-2 border-blue-500' : ''}
                    `}
                    onClick={() => setSelectedProperty(property)}
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex-1">
                        <div className="font-medium text-sm text-foreground">{property.label}</div>
                        <div className="text-xs text-muted-foreground font-mono mt-1">{property.uri}</div>
                        {property.comment && (
                          <div className="text-xs text-muted-foreground mt-2">{property.comment}</div>
                        )}
                        <div className="flex gap-2 mt-2">
                          <Badge variant="secondary" className="text-xs">
                            {property.property_type.replace('_', ' ')}
                          </Badge>
                          {property.domain.length > 0 && (
                            <Badge variant="outline" className="text-xs">
                              Domain: {property.domain.length}
                            </Badge>
                          )}
                          {property.range.length > 0 && (
                            <Badge variant="outline" className="text-xs">
                              Range: {property.range.length}
                            </Badge>
                          )}
                        </div>
                      </div>
                      {selectedProperty?.uri === property.uri && (
                        <CheckCircle2 className="w-5 h-5 text-blue-500 flex-shrink-0" />
                      )}
                    </div>
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleConfirm} disabled={!selectedProperty}>
            Confirm Mapping
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ============================================================================
// Manual Mapping Row Component
// ============================================================================

interface ManualMappingRowProps {
  field: {
    name: string;
    type: string;
    sample_values?: string[];
  };
  ontologyTree: any;
  selectedOntologies: OntologyMetadata[];
  mapping?: {
    uri: string;
    label: string;
    confidence: number;
  };
  onMappingChange: (fieldName: string, property: { uri: string; label: string } | null) => void;
}

function ManualMappingRow({
  field,
  ontologyTree,
  selectedOntologies,
  mapping,
  onMappingChange,
}: ManualMappingRowProps) {
  const [open, setOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const properties = useMemo(() => {
    if (!ontologyTree) return [];
    return ontologyTree.root_properties || [];
  }, [ontologyTree]);

  const filteredProperties = useMemo(() => {
    if (!searchQuery) return properties;
    const query = searchQuery.toLowerCase();
    return properties.filter(
      (prop: PropertyNode) =>
        prop.label.toLowerCase().includes(query) ||
        prop.uri.toLowerCase().includes(query) ||
        (prop.comment && prop.comment.toLowerCase().includes(query))
    );
  }, [properties, searchQuery]);

  // Common recommended properties
  const recommendedProperties = useMemo(() => {
    const commonTerms = [
      'givenName', 'familyName', 'name', 'email', 'telephone',
      'address', 'streetAddress', 'addressLocality', 'postalCode',
      'birthDate', 'identifier', 'description', 'url', 'image'
    ];

    return properties.filter((prop: PropertyNode) =>
      commonTerms.some(term => prop.label.toLowerCase().includes(term.toLowerCase()))
    ).slice(0, 5);
  }, [properties]);

  const handleSelect = (property: PropertyNode | null) => {
    if (property) {
      onMappingChange(field.name, {
        uri: property.uri,
        label: property.label,
      });
    } else {
      onMappingChange(field.name, null);
    }
    setOpen(false);
    setSearchQuery('');
  };

  return (
    <tr className="hover:bg-muted">
      <td className="px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="font-mono font-medium text-foreground text-xs">{field.name}</span>
          {field.sample_values && field.sample_values.length > 0 && (
            <Tooltip>
              <TooltipTrigger>
                <Info className="w-3 h-3 text-muted-foreground" />
              </TooltipTrigger>
              <TooltipContent>
                <div className="text-xs space-y-1 max-w-xs">
                  <div className="font-semibold">Sample Values:</div>
                  {field.sample_values.slice(0, 5).map((val, i) => (
                    <div key={i} className="font-mono text-muted-foreground truncate">{val}</div>
                  ))}
                </div>
              </TooltipContent>
            </Tooltip>
          )}
        </div>
      </td>
      <td className="px-3 py-2">
        <Badge variant="outline" className="text-xs font-mono">
          {field.type}
        </Badge>
      </td>
      <td className="px-3 py-2">
        {/* Ontology Term Selector */}
        <div className="relative">
          <Button
            variant="outline"
            role="combobox"
            aria-expanded={open}
            className="w-full justify-between h-8 text-xs font-normal"
            onClick={() => setOpen(!open)}
          >
            {mapping ? (
              <span className="font-mono text-blue-600 truncate">{mapping.label}</span>
            ) : (
              <span className="text-muted-foreground">Select term...</span>
            )}
            <ChevronsUpDown className="ml-2 h-3 w-3 shrink-0 opacity-50" />
          </Button>

          {open && (
            <div className="absolute z-50 mt-1 w-96 rounded-md border border-border bg-background shadow-lg">
              <div className="p-2 border-b border-black/5">
                <div className="relative">
                  <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-muted-foreground" />
                  <Input
                    placeholder="Search properties..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="pl-7 h-7 text-xs"
                    autoFocus
                  />
                </div>
              </div>

              <ScrollArea className="max-h-64">
                <div className="p-1">
                  {!searchQuery && recommendedProperties.length > 0 && (
                    <>
                      <div className="px-2 py-1.5 text-xs font-semibold text-muted-foreground">
                        Recommended
                      </div>
                      {recommendedProperties.map((property: PropertyNode) => (
                        <div
                          key={property.uri}
                          className={`
                            px-2 py-1.5 text-xs rounded cursor-pointer hover:bg-muted
                            ${mapping?.uri === property.uri ? 'bg-blue-50' : ''}
                          `}
                          onClick={() => handleSelect(property)}
                        >
                          <div className="flex items-center justify-between">
                            <div className="flex-1 min-w-0">
                              <div className="font-medium text-foreground truncate">{property.label}</div>
                              <div className="text-muted-foreground font-mono truncate">{property.uri}</div>
                            </div>
                            {mapping?.uri === property.uri && (
                              <Check className="w-3 h-3 text-blue-600 ml-2 flex-shrink-0" />
                            )}
                          </div>
                        </div>
                      ))}
                      <Separator className="my-1" />
                    </>
                  )}

                  {filteredProperties.length === 0 ? (
                    <div className="px-2 py-6 text-center text-xs text-muted-foreground">
                      No properties found
                    </div>
                  ) : (
                    <>
                      {!searchQuery && (
                        <div className="px-2 py-1.5 text-xs font-semibold text-muted-foreground">
                          All Properties
                        </div>
                      )}
                      {filteredProperties.map((property: PropertyNode) => (
                        <div
                          key={property.uri}
                          className={`
                            px-2 py-1.5 text-xs rounded cursor-pointer hover:bg-muted
                            ${mapping?.uri === property.uri ? 'bg-blue-50' : ''}
                          `}
                          onClick={() => handleSelect(property)}
                        >
                          <div className="flex items-center justify-between">
                            <div className="flex-1 min-w-0">
                              <div className="font-medium text-foreground truncate">{property.label}</div>
                              <div className="text-muted-foreground font-mono truncate">{property.uri}</div>
                              {property.comment && (
                                <div className="text-muted-foreground mt-1 line-clamp-2">{property.comment}</div>
                              )}
                            </div>
                            {mapping?.uri === property.uri && (
                              <Check className="w-3 h-3 text-blue-600 ml-2 flex-shrink-0" />
                            )}
                          </div>
                        </div>
                      ))}
                    </>
                  )}
                </div>
              </ScrollArea>

              {mapping && (
                <>
                  <Separator />
                  <div className="p-2">
                    <Button
                      variant="ghost"
                      size="sm"
                      className="w-full h-7 text-xs text-red-600 hover:text-red-700 hover:bg-red-50"
                      onClick={() => handleSelect(null)}
                    >
                      <XCircle className="w-3 h-3 mr-1" />
                      Clear Selection
                    </Button>
                  </div>
                </>
              )}
            </div>
          )}

          {/* Backdrop to close dropdown when clicking outside */}
          {open && (
            <div
              className="fixed inset-0 z-40"
              onClick={() => setOpen(false)}
            />
          )}
        </div>
      </td>
      <td className="px-3 py-2">
        {mapping ? (
          <Badge className="bg-[#3A7728] text-white border-[#3A7728] text-xs">
            <CheckCircle2 className="w-3 h-3 mr-1" />
            Mapped
          </Badge>
        ) : (
          <Badge variant="outline" className="text-xs text-muted-foreground">
            Unmapped
          </Badge>
        )}
      </td>
    </tr>
  );
}
