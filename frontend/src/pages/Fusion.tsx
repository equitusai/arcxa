/**
 * Entity Fusion - Enterprise-grade duplicate resolution system
 *
 * Oracle Redwood × Microsoft Fluent Design
 *
 * Designed for data stewards managing entity deduplication at massive scale:
 * - Thousands of ontologies, millions of datasets
 * - Hundreds of thousands of entities per dataset
 * - Multi-user concurrent operations
 * - 24/7 operational resilience
 *
 * @author Graphica UX Designer Agent
 */

import React, { useState, useCallback, useMemo, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  GitMerge,
  Search,
  Filter,
  TrendingDown,
  Clock,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Play,
  ChevronRight,
  Archive,
  Settings,
  BarChart3,
  Users,
  Download,
  RefreshCw,
  Zap,
  Database,
  Shield,
  FileText,
  Calendar,
  ArrowRight,
  Sparkles,
  Eye,
  ThumbsUp,
  ThumbsDown,
  History,
  Undo2,
  Info,
  ChevronDown,
  Loader2,
  Target,
} from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Slider } from '@/components/ui/slider';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Progress } from '@/components/ui/progress';
import { Separator } from '@/components/ui/separator';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { toast } from 'sonner';

// API & Hooks
import {
  useFusionCandidates,
  useProposeFusion,
  useApproveFusionCandidate,
  useRejectFusionCandidate,
  useResolveFusion,
  useReverseFusion,
  useFusionHistory,
} from '@/hooks/useFusion';
import { listDatasets } from '@/api/datasets';
import type { FusionCandidate, Dataset } from '@/api/types';

// ============================================================================
// Types & Constants
// ============================================================================

type MatchRule = 'email' | 'phone' | 'ssn' | 'name' | 'custom';

interface DatasetOption {
  id: string;
  name: string;
  entity_count: number;
  quality_score?: number;
  fusion_candidates?: number;
}

const MATCH_RULES: { value: MatchRule; label: string; description: string; icon: React.ReactNode }[] = [
  {
    value: 'email',
    label: 'Email Match',
    description: 'Most reliable - exact email addresses',
    icon: <Shield className="h-4 w-4" />,
  },
  {
    value: 'phone',
    label: 'Phone Match',
    description: 'Normalized phone numbers',
    icon: <Shield className="h-4 w-4" />,
  },
  {
    value: 'ssn',
    label: 'SSN Match',
    description: 'Social Security Number (PII)',
    icon: <Shield className="h-4 w-4" />,
  },
  {
    value: 'name',
    label: 'Name Similarity',
    description: 'Fuzzy matching on full names',
    icon: <AlertTriangle className="h-4 w-4" />,
  },
];

const DEFAULT_CONFIDENCE = 85;
const MIN_CONFIDENCE = 60;
const MAX_CONFIDENCE = 99;

// Confidence thresholds
const CONFIDENCE_HIGH = 95;
const CONFIDENCE_MEDIUM = 80;

// ============================================================================
// Utility Functions
// ============================================================================

const getConfidenceColor = (confidence: number) => {
  const percentage = confidence * 100;
  if (percentage >= CONFIDENCE_HIGH) return 'text-emerald-600 dark:text-emerald-400';
  if (percentage >= CONFIDENCE_MEDIUM) return 'text-amber-600 dark:text-amber-400';
  return 'text-rose-600 dark:text-rose-400';
};

const getConfidenceBadgeVariant = (confidence: number): 'success' | 'default' | 'destructive' => {
  const percentage = confidence * 100;
  if (percentage >= CONFIDENCE_HIGH) return 'success';
  if (percentage >= CONFIDENCE_MEDIUM) return 'default';
  return 'destructive';
};

const getConfidenceLabel = (confidence: number): string => {
  const percentage = confidence * 100;
  if (percentage >= CONFIDENCE_HIGH) return 'High Confidence';
  if (percentage >= CONFIDENCE_MEDIUM) return 'Medium Confidence';
  return 'Low Confidence';
};

const formatNumber = (num: number): string => {
  return new Intl.NumberFormat('en-US').format(num);
};

const formatRelativeTime = (date: string): string => {
  const now = new Date().getTime();
  const then = new Date(date).getTime();
  const diffMs = now - then;
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  return `${diffDays}d ago`;
};

// ============================================================================
// Main Component
// ============================================================================

export function Fusion() {
  // -------------------------------------------------------------------------
  // State Management
  // -------------------------------------------------------------------------

  // Dashboard state
  const [activeView, setActiveView] = useState<'overview' | 'review' | 'history' | 'metrics'>('overview');

  // Candidate Finder state
  const [datasetSearch, setDatasetSearch] = useState('');
  const [selectedDataset, setSelectedDataset] = useState<DatasetOption | null>(null);
  const [selectedRule, setSelectedRule] = useState<MatchRule>('email');
  const [confidenceThreshold, setConfidenceThreshold] = useState([DEFAULT_CONFIDENCE]);
  const [datasetOptions, setDatasetOptions] = useState<DatasetOption[]>([]);
  const [loadingDatasets, setLoadingDatasets] = useState(false);

  // Review queue state
  const [selectedCandidates, setSelectedCandidates] = useState<Set<string>>(new Set());
  const [comparisonCandidate, setComparisonCandidate] = useState<FusionCandidate | null>(null);
  const [reviewNotes, setReviewNotes] = useState('');
  const [activeReviewer] = useState('sarah.johnson'); // TODO: Get from auth context

  // Commit workflow state
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [commitReason, setCommitReason] = useState('');

  // Reverse fusion state
  const [reverseDialogOpen, setReverseDialogOpen] = useState(false);
  const [reverseReason, setReverseReason] = useState('');
  const [selectedFusionId, setSelectedFusionId] = useState<string | null>(null);

  // -------------------------------------------------------------------------
  // API Queries & Mutations
  // -------------------------------------------------------------------------

  const { data: candidatesData, isLoading: loadingCandidates } = useFusionCandidates({
    status: 'proposed',
    limit: 100,
  });

  const { data: approvedData } = useFusionCandidates({
    status: 'approved',
    limit: 100,
  });

  const { data: historyData } = useFusionHistory({ limit: 50 });

  const proposeFusion = useProposeFusion();
  const approveCandidate = useApproveFusionCandidate();
  const rejectCandidate = useRejectFusionCandidate();
  const resolveFusion = useResolveFusion();
  const reverseFusion = useReverseFusion();

  // -------------------------------------------------------------------------
  // Dataset Search & Selection
  // -------------------------------------------------------------------------

  // Load datasets on mount
  useEffect(() => {
    const fetchDatasets = async () => {
      try {
        setLoadingDatasets(true);
        const response = await listDatasets();
        const options: DatasetOption[] = response.datasets.map((d) => ({
          id: d.id,
          name: d.name,
          entity_count: d.entity_count || d.record_count,
          quality_score: d.quality_score,
          fusion_candidates: d.fusion_candidates,
        }));
        setDatasetOptions(options);
      } catch (error) {
        console.error('Failed to load datasets:', error);
        toast.error('Failed to load datasets');
      } finally {
        setLoadingDatasets(false);
      }
    };

    fetchDatasets();
  }, []);

  // Filter datasets based on search
  const filteredDatasets = useMemo(() => {
    if (!datasetSearch) return datasetOptions.slice(0, 10); // Show top 10 by default

    const searchLower = datasetSearch.toLowerCase();
    return datasetOptions
      .filter((d) => d.name.toLowerCase().includes(searchLower) || d.id.toLowerCase().includes(searchLower))
      .slice(0, 20);
  }, [datasetSearch, datasetOptions]);

  // -------------------------------------------------------------------------
  // Propose Fusion Candidates
  // -------------------------------------------------------------------------

  const handleProposeFusion = useCallback(() => {
    if (!selectedDataset) {
      toast.error('Please select a dataset');
      return;
    }

    proposeFusion.mutate({
      dataset: selectedDataset.id,
      rule: selectedRule,
      min_confidence: confidenceThreshold[0] / 100,
    });
  }, [selectedDataset, selectedRule, confidenceThreshold, proposeFusion]);

  // -------------------------------------------------------------------------
  // Candidate Selection & Bulk Actions
  // -------------------------------------------------------------------------

  const candidates = candidatesData?.candidates || [];
  const approvedCandidates = approvedData?.candidates || [];

  const toggleCandidateSelection = useCallback((candidateId: string) => {
    setSelectedCandidates((prev) => {
      const next = new Set(prev);
      if (next.has(candidateId)) {
        next.delete(candidateId);
      } else {
        next.add(candidateId);
      }
      return next;
    });
  }, []);

  const selectAllVisible = useCallback(() => {
    const allIds = candidates.map((c) => c.candidate_id);
    setSelectedCandidates(new Set(allIds));
  }, [candidates]);

  const clearSelection = useCallback(() => {
    setSelectedCandidates(new Set());
  }, []);

  const selectHighConfidence = useCallback(() => {
    const highConfidenceIds = candidates.filter((c) => c.confidence >= CONFIDENCE_HIGH / 100).map((c) => c.candidate_id);
    setSelectedCandidates(new Set(highConfidenceIds));
  }, [candidates]);

  // -------------------------------------------------------------------------
  // Review Actions (Approve/Reject)
  // -------------------------------------------------------------------------

  const handleBulkApprove = useCallback(async () => {
    if (selectedCandidates.size === 0) {
      toast.error('No candidates selected');
      return;
    }

    try {
      const promises = Array.from(selectedCandidates).map((id) =>
        approveCandidate.mutateAsync({
          candidateId: id,
          request: { reviewer: activeReviewer, notes: reviewNotes || undefined },
        })
      );

      await Promise.all(promises);
      setSelectedCandidates(new Set());
      setReviewNotes('');
      toast.success(`Approved ${promises.length} candidates`);
    } catch (error) {
      console.error('Bulk approve failed:', error);
    }
  }, [selectedCandidates, activeReviewer, reviewNotes, approveCandidate]);

  const handleBulkReject = useCallback(async () => {
    if (selectedCandidates.size === 0) {
      toast.error('No candidates selected');
      return;
    }

    try {
      const promises = Array.from(selectedCandidates).map((id) =>
        rejectCandidate.mutateAsync({
          candidateId: id,
          request: { reviewer: activeReviewer, notes: reviewNotes || undefined },
        })
      );

      await Promise.all(promises);
      setSelectedCandidates(new Set());
      setReviewNotes('');
      toast.success(`Rejected ${promises.length} candidates`);
    } catch (error) {
      console.error('Bulk reject failed:', error);
    }
  }, [selectedCandidates, activeReviewer, reviewNotes, rejectCandidate]);

  const handleApproveOne = useCallback(
    (candidateId: string) => {
      approveCandidate.mutate({
        candidateId,
        request: { reviewer: activeReviewer },
      });
    },
    [activeReviewer, approveCandidate]
  );

  const handleRejectOne = useCallback(
    (candidateId: string) => {
      rejectCandidate.mutate({
        candidateId,
        request: { reviewer: activeReviewer },
      });
    },
    [activeReviewer, rejectCandidate]
  );

  // -------------------------------------------------------------------------
  // Commit Workflow
  // -------------------------------------------------------------------------

  const handleCommitFusions = useCallback(() => {
    if (approvedCandidates.length === 0) {
      toast.error('No approved candidates to commit');
      return;
    }
    setCommitDialogOpen(true);
  }, [approvedCandidates]);

  const confirmCommit = useCallback(async () => {
    try {
      const promises = approvedCandidates.map((candidate) =>
        resolveFusion.mutateAsync({
          entities: candidate.entities,
          rule: candidate.match_rule,
          confidence: candidate.confidence,
        })
      );

      await Promise.all(promises);
      setCommitDialogOpen(false);
      setCommitReason('');
      toast.success(`Committed ${promises.length} fusion operations`);
    } catch (error) {
      console.error('Commit failed:', error);
    }
  }, [approvedCandidates, resolveFusion]);

  // -------------------------------------------------------------------------
  // Reverse Fusion
  // -------------------------------------------------------------------------

  const handleReverseFusion = useCallback(
    (fusionId: string) => {
      setSelectedFusionId(fusionId);
      setReverseDialogOpen(true);
    },
    []
  );

  const confirmReverse = useCallback(async () => {
    if (!selectedFusionId) return;

    try {
      await reverseFusion.mutateAsync({
        fusionId: selectedFusionId,
        request: { reason: reverseReason },
      });
      setReverseDialogOpen(false);
      setReverseReason('');
      setSelectedFusionId(null);
    } catch (error) {
      console.error('Reverse fusion failed:', error);
    }
  }, [selectedFusionId, reverseReason, reverseFusion]);

  // -------------------------------------------------------------------------
  // Computed Metrics
  // -------------------------------------------------------------------------

  const metrics = useMemo(() => {
    const totalCandidates = candidates.length;
    const highConfCount = candidates.filter((c) => c.confidence >= CONFIDENCE_HIGH / 100).length;
    const mediumConfCount = candidates.filter((c) => c.confidence >= CONFIDENCE_MEDIUM / 100 && c.confidence < CONFIDENCE_HIGH / 100).length;
    const lowConfCount = candidates.filter((c) => c.confidence < CONFIDENCE_MEDIUM / 100).length;

    const approvedCount = approvedCandidates.length;
    const avgConfidence = candidates.length > 0 ? candidates.reduce((sum, c) => sum + c.confidence, 0) / candidates.length : 0;

    return {
      totalCandidates,
      highConfCount,
      mediumConfCount,
      lowConfCount,
      approvedCount,
      avgConfidence,
    };
  }, [candidates, approvedCandidates]);

  // -------------------------------------------------------------------------
  // Render: Header
  // -------------------------------------------------------------------------

  const renderHeader = () => (
    <motion.div
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.16 }}
      className="flex flex-col md:flex-row md:items-start md:justify-between gap-4 pb-4 border-b border-black/10 dark:border-white/10"
    >
      <div>
        <div className="flex items-center gap-3 mb-1">
          <div className="p-2 rounded-md bg-emerald-500/10 dark:bg-emerald-400/10">
            <GitMerge className="h-5 w-5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <h1 className="text-2xl font-semibold text-gray-900 dark:text-gray-50">Entity Fusion</h1>
        </div>
        <p className="text-sm text-gray-600 dark:text-gray-400 ml-[52px]">
          Intelligent duplicate resolution for data stewards
        </p>
      </div>

      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" className="gap-2">
          <Download className="h-4 w-4" />
          Export Audit
        </Button>
        <Button variant="outline" size="sm" className="gap-2">
          <Settings className="h-4 w-4" />
          Configure
        </Button>
      </div>
    </motion.div>
  );

  // -------------------------------------------------------------------------
  // Render: Quick Stats
  // -------------------------------------------------------------------------

  const renderQuickStats = () => (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.16, delay: 0.05 }}
      className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4"
    >
      {/* Total Candidates */}
      <Card className="border-black/10 dark:border-white/10">
        <CardContent className="pt-6">
          <div className="flex items-center justify-between mb-2">
            <div className="p-2 rounded-md bg-blue-500/10">
              <Database className="h-4 w-4 text-blue-600 dark:text-blue-400" />
            </div>
            <Badge variant="outline" className="text-xs">
              Proposed
            </Badge>
          </div>
          <div className="text-2xl font-semibold text-gray-900 dark:text-gray-50 mb-1">{formatNumber(metrics.totalCandidates)}</div>
          <p className="text-xs text-gray-600 dark:text-gray-400">Candidates for review</p>
        </CardContent>
      </Card>

      {/* High Confidence */}
      <Card className="border-black/10 dark:border-white/10">
        <CardContent className="pt-6">
          <div className="flex items-center justify-between mb-2">
            <div className="p-2 rounded-md bg-emerald-500/10">
              <CheckCircle2 className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
            </div>
            <Badge variant="success" className="text-xs">
              ≥95%
            </Badge>
          </div>
          <div className="text-2xl font-semibold text-gray-900 dark:text-gray-50 mb-1">{formatNumber(metrics.highConfCount)}</div>
          <p className="text-xs text-gray-600 dark:text-gray-400">High confidence matches</p>
        </CardContent>
      </Card>

      {/* Approved & Ready */}
      <Card className="border-black/10 dark:border-white/10">
        <CardContent className="pt-6">
          <div className="flex items-center justify-between mb-2">
            <div className="p-2 rounded-md bg-violet-500/10">
              <ThumbsUp className="h-4 w-4 text-violet-600 dark:text-violet-400" />
            </div>
            <Badge variant="outline" className="text-xs">
              Ready
            </Badge>
          </div>
          <div className="text-2xl font-semibold text-gray-900 dark:text-gray-50 mb-1">{formatNumber(metrics.approvedCount)}</div>
          <p className="text-xs text-gray-600 dark:text-gray-400">Approved for commit</p>
        </CardContent>
      </Card>

      {/* Avg Confidence */}
      <Card className="border-black/10 dark:border-white/10">
        <CardContent className="pt-6">
          <div className="flex items-center justify-between mb-2">
            <div className="p-2 rounded-md bg-amber-500/10">
              <Target className="h-4 w-4 text-amber-600 dark:text-amber-400" />
            </div>
            <Badge variant="outline" className="text-xs">
              Avg
            </Badge>
          </div>
          <div className="text-2xl font-semibold text-gray-900 dark:text-gray-50 mb-1">{(metrics.avgConfidence * 100).toFixed(0)}%</div>
          <p className="text-xs text-gray-600 dark:text-gray-400">Average confidence score</p>
        </CardContent>
      </Card>
    </motion.div>
  );

  // -------------------------------------------------------------------------
  // Render: Candidate Finder Panel
  // -------------------------------------------------------------------------

  const renderCandidateFinder = () => (
    <Card className="border-black/10 dark:border-white/10">
      <CardHeader>
        <div className="flex items-center gap-2">
          <Search className="h-5 w-5 text-blue-600 dark:text-blue-400" />
          <CardTitle className="text-base">Candidate Finder</CardTitle>
        </div>
        <CardDescription>Configure matching rules and generate candidates</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        {/* Dataset Search */}
        <div className="space-y-3">
          <Label className="text-sm font-medium">Dataset</Label>
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-gray-400" />
            <Input
              placeholder="Search datasets..."
              value={datasetSearch}
              onChange={(e) => setDatasetSearch(e.target.value)}
              className="pl-9 h-10"
            />
          </div>

          {/* Dataset Dropdown */}
          {datasetSearch && (
            <ScrollArea className="h-48 rounded-md border border-black/10 dark:border-white/10 p-2">
              {loadingDatasets ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="h-5 w-5 animate-spin text-gray-400" />
                </div>
              ) : filteredDatasets.length === 0 ? (
                <div className="text-center py-8 text-sm text-gray-500">No datasets found</div>
              ) : (
                <div className="space-y-1">
                  {filteredDatasets.map((dataset) => (
                    <button
                      key={dataset.id}
                      onClick={() => {
                        setSelectedDataset(dataset);
                        setDatasetSearch('');
                      }}
                      className="w-full text-left px-3 py-2 rounded-sm hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                    >
                      <div className="flex items-center justify-between">
                        <div>
                          <div className="text-sm font-medium text-gray-900 dark:text-gray-50">{dataset.name}</div>
                          <div className="text-xs text-gray-500">{formatNumber(dataset.entity_count)} entities</div>
                        </div>
                        {dataset.fusion_candidates !== undefined && dataset.fusion_candidates > 0 && (
                          <Badge variant="outline" className="text-xs">
                            {dataset.fusion_candidates} candidates
                          </Badge>
                        )}
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </ScrollArea>
          )}

          {/* Selected Dataset */}
          {selectedDataset && !datasetSearch && (
            <div className="p-3 rounded-md border border-black/10 dark:border-white/10 bg-blue-500/5">
              <div className="flex items-center justify-between">
                <div>
                  <div className="text-sm font-medium text-gray-900 dark:text-gray-50">{selectedDataset.name}</div>
                  <div className="text-xs text-gray-500">{formatNumber(selectedDataset.entity_count)} entities</div>
                </div>
                <Button variant="ghost" size="sm" onClick={() => setSelectedDataset(null)}>
                  <XCircle className="h-4 w-4" />
                </Button>
              </div>
            </div>
          )}
        </div>

        <Separator />

        {/* Matching Rule */}
        <div className="space-y-3">
          <Label className="text-sm font-medium">Matching Rule</Label>
          <div className="grid grid-cols-1 gap-2">
            {MATCH_RULES.map((rule) => (
              <button
                key={rule.value}
                onClick={() => setSelectedRule(rule.value)}
                className={`p-3 rounded-md border transition-all text-left ${
                  selectedRule === rule.value
                    ? 'border-blue-500 bg-blue-500/10 dark:bg-blue-400/10'
                    : 'border-black/10 dark:border-white/10 hover:border-gray-300 dark:hover:border-gray-600'
                }`}
              >
                <div className="flex items-start gap-3">
                  <div className={`mt-0.5 ${selectedRule === rule.value ? 'text-blue-600 dark:text-blue-400' : 'text-gray-400'}`}>
                    {rule.icon}
                  </div>
                  <div className="flex-1">
                    <div className="text-sm font-medium text-gray-900 dark:text-gray-50">{rule.label}</div>
                    <div className="text-xs text-gray-500 mt-0.5">{rule.description}</div>
                  </div>
                  {selectedRule === rule.value && <CheckCircle2 className="h-4 w-4 text-blue-600 dark:text-blue-400 mt-0.5" />}
                </div>
              </button>
            ))}
          </div>
        </div>

        <Separator />

        {/* Confidence Threshold */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <Label className="text-sm font-medium">Confidence Threshold</Label>
            <span className="text-sm font-mono text-gray-900 dark:text-gray-50">{confidenceThreshold[0]}%</span>
          </div>
          <Slider
            value={confidenceThreshold}
            onValueChange={setConfidenceThreshold}
            min={MIN_CONFIDENCE}
            max={MAX_CONFIDENCE}
            step={1}
            className="w-full"
          />
          <div className="flex items-center justify-between text-xs text-gray-500">
            <span>More candidates</span>
            <span>Higher precision</span>
          </div>
        </div>

        <Separator />

        {/* Action Button */}
        <Button onClick={handleProposeFusion} disabled={!selectedDataset || proposeFusion.isPending} className="w-full gap-2 h-10">
          {proposeFusion.isPending ? (
            <>
              <Loader2 className="h-4 w-4 animate-spin" />
              Finding Candidates...
            </>
          ) : (
            <>
              <Sparkles className="h-4 w-4" />
              Find Candidates
            </>
          )}
        </Button>
      </CardContent>
    </Card>
  );

  // -------------------------------------------------------------------------
  // Render: Review Queue
  // -------------------------------------------------------------------------

  const renderReviewQueue = () => (
    <Card className="border-black/10 dark:border-white/10">
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <div className="flex items-center gap-2">
              <Filter className="h-5 w-5 text-violet-600 dark:text-violet-400" />
              <CardTitle className="text-base">Review Queue</CardTitle>
            </div>
            <CardDescription>
              {metrics.totalCandidates} candidates • {selectedCandidates.size} selected
            </CardDescription>
          </div>

          {/* Bulk Actions */}
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={selectHighConfidence} disabled={metrics.highConfCount === 0}>
              Select High (≥95%)
            </Button>
            <Button variant="outline" size="sm" onClick={selectAllVisible} disabled={candidates.length === 0}>
              Select All
            </Button>
            {selectedCandidates.size > 0 && (
              <Button variant="outline" size="sm" onClick={clearSelection}>
                Clear
              </Button>
            )}
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Bulk Action Bar */}
        <AnimatePresence>
          {selectedCandidates.size > 0 && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              className="p-4 rounded-md bg-violet-500/10 border border-violet-200 dark:border-violet-800"
            >
              <div className="flex items-center justify-between">
                <div className="text-sm font-medium text-gray-900 dark:text-gray-50">
                  {selectedCandidates.size} candidate{selectedCandidates.size > 1 ? 's' : ''} selected
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleBulkReject}
                    disabled={rejectCandidate.isPending}
                    className="gap-2 bg-white dark:bg-gray-900"
                  >
                    <ThumbsDown className="h-4 w-4" />
                    Reject All
                  </Button>
                  <Button
                    size="sm"
                    onClick={handleBulkApprove}
                    disabled={approveCandidate.isPending}
                    className="gap-2"
                  >
                    <ThumbsUp className="h-4 w-4" />
                    Approve All
                  </Button>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Candidates List */}
        {loadingCandidates ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="h-6 w-6 animate-spin text-gray-400" />
          </div>
        ) : candidates.length === 0 ? (
          <div className="text-center py-12">
            <Database className="h-12 w-12 text-gray-300 dark:text-gray-700 mx-auto mb-3" />
            <h3 className="text-sm font-medium text-gray-900 dark:text-gray-50 mb-1">No candidates found</h3>
            <p className="text-sm text-gray-500">Select a dataset and click "Find Candidates" to begin</p>
          </div>
        ) : (
          <ScrollArea className="h-[600px] pr-4">
            <div className="space-y-3">
              {candidates.map((candidate, index) => (
                <motion.div
                  key={candidate.candidate_id}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.16, delay: index * 0.02 }}
                  className={`p-4 rounded-md border transition-all ${
                    selectedCandidates.has(candidate.candidate_id)
                      ? 'border-violet-300 dark:border-violet-700 bg-violet-500/5'
                      : 'border-black/10 dark:border-white/10 hover:border-gray-300 dark:hover:border-gray-600'
                  }`}
                >
                  <div className="flex items-start gap-3">
                    {/* Checkbox */}
                    <Checkbox
                      checked={selectedCandidates.has(candidate.candidate_id)}
                      onCheckedChange={() => toggleCandidateSelection(candidate.candidate_id)}
                      className="mt-1"
                    />

                    {/* Candidate Info */}
                    <div className="flex-1 space-y-3">
                      {/* Header: Entities */}
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-2 text-sm font-mono text-gray-900 dark:text-gray-50">
                          <span>{candidate.entities[0]?.id || 'ENT-???'}</span>
                          <ArrowRight className="h-4 w-4 text-gray-400" />
                          <span>{candidate.entities[1]?.id || 'ENT-???'}</span>
                        </div>
                        <Badge variant={getConfidenceBadgeVariant(candidate.confidence)} className="text-xs">
                          {(candidate.confidence * 100).toFixed(0)}% {getConfidenceLabel(candidate.confidence).split(' ')[0]}
                        </Badge>
                      </div>

                      {/* Match Details */}
                      <div className="flex items-center gap-4 text-xs text-gray-600 dark:text-gray-400">
                        <div className="flex items-center gap-1">
                          <Shield className="h-3 w-3" />
                          <span>Matched on {candidate.match_rule}</span>
                        </div>
                        {candidate.match_value && (
                          <div className="flex items-center gap-1">
                            <Info className="h-3 w-3" />
                            <span className="font-mono">{candidate.match_value}</span>
                          </div>
                        )}
                        <div className="flex items-center gap-1">
                          <Clock className="h-3 w-3" />
                          <span>{formatRelativeTime(candidate.proposed_at)}</span>
                        </div>
                      </div>

                      {/* Confidence Visualization */}
                      <div className="space-y-1">
                        <Progress value={candidate.confidence * 100} className="h-2" />
                      </div>

                      {/* Actions */}
                      <div className="flex items-center gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setComparisonCandidate(candidate)}
                          className="gap-2 flex-1"
                        >
                          <Eye className="h-4 w-4" />
                          Compare
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleRejectOne(candidate.candidate_id)}
                          disabled={rejectCandidate.isPending}
                          className="gap-2"
                        >
                          <ThumbsDown className="h-4 w-4" />
                          Reject
                        </Button>
                        <Button
                          size="sm"
                          onClick={() => handleApproveOne(candidate.candidate_id)}
                          disabled={approveCandidate.isPending}
                          className="gap-2"
                        >
                          <ThumbsUp className="h-4 w-4" />
                          Approve
                        </Button>
                      </div>
                    </div>
                  </div>
                </motion.div>
              ))}
            </div>
          </ScrollArea>
        )}

        {/* Commit Action */}
        {metrics.approvedCount > 0 && (
          <div className="pt-4 border-t border-black/10 dark:border-white/10">
            <Button onClick={handleCommitFusions} className="w-full gap-2 h-10">
              <Zap className="h-4 w-4" />
              Commit {metrics.approvedCount} Approved Fusion{metrics.approvedCount > 1 ? 's' : ''}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );

  // -------------------------------------------------------------------------
  // Render: Fusion History
  // -------------------------------------------------------------------------

  const renderHistory = () => {
    const fusions = historyData?.data || [];

    return (
      <Card className="border-black/10 dark:border-white/10">
        <CardHeader>
          <div className="flex items-center gap-2">
            <History className="h-5 w-5 text-gray-600 dark:text-gray-400" />
            <CardTitle className="text-base">Fusion History</CardTitle>
          </div>
          <CardDescription>{fusions.length} committed operations</CardDescription>
        </CardHeader>
        <CardContent>
          {fusions.length === 0 ? (
            <div className="text-center py-12">
              <Archive className="h-12 w-12 text-gray-300 dark:text-gray-700 mx-auto mb-3" />
              <h3 className="text-sm font-medium text-gray-900 dark:text-gray-50 mb-1">No history yet</h3>
              <p className="text-sm text-gray-500">Committed fusions will appear here</p>
            </div>
          ) : (
            <ScrollArea className="h-[400px]">
              <div className="space-y-3">
                {fusions.map((fusion: any, index: number) => (
                  <motion.div
                    key={fusion.fusion_id || index}
                    initial={{ opacity: 0, x: -8 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ duration: 0.16, delay: index * 0.02 }}
                    className="p-3 rounded-md border border-black/10 dark:border-white/10 hover:border-gray-300 dark:hover:border-gray-600 transition-colors"
                  >
                    <div className="flex items-center justify-between mb-2">
                      <div className="flex items-center gap-2">
                        <div className="p-1.5 rounded-sm bg-emerald-500/10">
                          <CheckCircle2 className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
                        </div>
                        <span className="text-sm font-mono text-gray-900 dark:text-gray-50">
                          {fusion.merged_entity_id || 'ENT-???'}
                        </span>
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => handleReverseFusion(fusion.fusion_id)}
                        className="gap-1 h-7 px-2 text-xs"
                      >
                        <Undo2 className="h-3 w-3" />
                        Reverse
                      </Button>
                    </div>
                    <div className="flex items-center gap-3 text-xs text-gray-600 dark:text-gray-400">
                      <span>{fusion.source_entity_ids?.length || 0} entities merged</span>
                      <span>•</span>
                      <span>{fusion.rule || 'unknown'} rule</span>
                      <span>•</span>
                      <span>{formatRelativeTime(fusion.created_at)}</span>
                    </div>
                  </motion.div>
                ))}
              </div>
            </ScrollArea>
          )}
        </CardContent>
      </Card>
    );
  };

  // -------------------------------------------------------------------------
  // Render: Comparison Dialog
  // -------------------------------------------------------------------------

  const renderComparisonDialog = () => {
    if (!comparisonCandidate) return null;

    const [entity1, entity2] = comparisonCandidate.entities;

    return (
      <Dialog open={!!comparisonCandidate} onOpenChange={() => setComparisonCandidate(null)}>
        <DialogContent className="max-w-4xl max-h-[80vh] overflow-hidden flex flex-col">
          <DialogHeader>
            <DialogTitle>Side-by-Side Comparison</DialogTitle>
            <DialogDescription>
              Compare attributes to verify this is a true duplicate
            </DialogDescription>
          </DialogHeader>

          <div className="flex-1 overflow-auto">
            <div className="grid grid-cols-2 gap-4 p-4">
              {/* Entity 1 */}
              <div className="space-y-3">
                <div className="p-3 rounded-md bg-blue-500/10 border border-blue-200 dark:border-blue-800">
                  <div className="text-sm font-medium text-gray-900 dark:text-gray-50">Entity 1</div>
                  <div className="text-xs font-mono text-gray-600 dark:text-gray-400 mt-1">{entity1?.id}</div>
                </div>
                <div className="space-y-2">
                  {Object.entries(entity1 || {}).map(([key, value]) => {
                    if (key === 'id') return null;
                    return (
                      <div key={key} className="p-2 rounded-sm border border-black/10 dark:border-white/10">
                        <div className="text-xs font-medium text-gray-500 mb-0.5">{key}</div>
                        <div className="text-sm text-gray-900 dark:text-gray-50">{String(value)}</div>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Entity 2 */}
              <div className="space-y-3">
                <div className="p-3 rounded-md bg-violet-500/10 border border-violet-200 dark:border-violet-800">
                  <div className="text-sm font-medium text-gray-900 dark:text-gray-50">Entity 2</div>
                  <div className="text-xs font-mono text-gray-600 dark:text-gray-400 mt-1">{entity2?.id}</div>
                </div>
                <div className="space-y-2">
                  {Object.entries(entity2 || {}).map(([key, value]) => {
                    if (key === 'id') return null;
                    const isDifferent = entity1?.[key] !== value;
                    return (
                      <div
                        key={key}
                        className={`p-2 rounded-sm border ${
                          isDifferent
                            ? 'border-amber-300 dark:border-amber-700 bg-amber-500/5'
                            : 'border-black/10 dark:border-white/10'
                        }`}
                      >
                        <div className="flex items-center justify-between mb-0.5">
                          <div className="text-xs font-medium text-gray-500">{key}</div>
                          {isDifferent && <AlertTriangle className="h-3 w-3 text-amber-600 dark:text-amber-400" />}
                        </div>
                        <div className="text-sm text-gray-900 dark:text-gray-50">{String(value)}</div>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          </div>

          <DialogFooter className="border-t border-black/10 dark:border-white/10 pt-4">
            <Button variant="outline" onClick={() => setComparisonCandidate(null)}>
              Close
            </Button>
            <Button
              variant="outline"
              onClick={() => {
                handleRejectOne(comparisonCandidate.candidate_id);
                setComparisonCandidate(null);
              }}
              className="gap-2"
            >
              <ThumbsDown className="h-4 w-4" />
              Reject
            </Button>
            <Button
              onClick={() => {
                handleApproveOne(comparisonCandidate.candidate_id);
                setComparisonCandidate(null);
              }}
              className="gap-2"
            >
              <ThumbsUp className="h-4 w-4" />
              Approve
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  };

  // -------------------------------------------------------------------------
  // Render: Commit Dialog
  // -------------------------------------------------------------------------

  const renderCommitDialog = () => (
    <Dialog open={commitDialogOpen} onOpenChange={setCommitDialogOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Commit Fusion Operations</DialogTitle>
          <DialogDescription>
            You are about to commit {metrics.approvedCount} fusion operation{metrics.approvedCount > 1 ? 's' : ''}. This action is
            reversible.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <Alert>
            <Info className="h-4 w-4" />
            <AlertTitle>What will happen?</AlertTitle>
            <AlertDescription>
              Approved entity pairs will be merged atomically. Source entities will be replaced with a single resolved entity.
              All operations are logged for audit compliance.
            </AlertDescription>
          </Alert>

          <div className="space-y-2">
            <Label htmlFor="commit-reason">Commit Reason (Optional)</Label>
            <Textarea
              id="commit-reason"
              placeholder="e.g., Weekly customer deduplication batch"
              value={commitReason}
              onChange={(e) => setCommitReason(e.target.value)}
              rows={3}
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setCommitDialogOpen(false)}>
            Cancel
          </Button>
          <Button onClick={confirmCommit} disabled={resolveFusion.isPending} className="gap-2">
            {resolveFusion.isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Committing...
              </>
            ) : (
              <>
                <Zap className="h-4 w-4" />
                Commit Fusions
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );

  // -------------------------------------------------------------------------
  // Render: Reverse Fusion Dialog
  // -------------------------------------------------------------------------

  const renderReverseDialog = () => (
    <Dialog open={reverseDialogOpen} onOpenChange={setReverseDialogOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Reverse Fusion Operation</DialogTitle>
          <DialogDescription>
            This will undo the fusion and restore the original entities. All changes will be logged for audit.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Caution</AlertTitle>
            <AlertDescription>
              Reversing a fusion will split the merged entity back into its source entities. Downstream systems may be affected.
            </AlertDescription>
          </Alert>

          <div className="space-y-2">
            <Label htmlFor="reverse-reason">Reason for Reversal (Required)</Label>
            <Textarea
              id="reverse-reason"
              placeholder="e.g., Incorrect match - entities are actually different customers"
              value={reverseReason}
              onChange={(e) => setReverseReason(e.target.value)}
              rows={3}
              required
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setReverseDialogOpen(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={confirmReverse}
            disabled={!reverseReason || reverseFusion.isPending}
            className="gap-2"
          >
            {reverseFusion.isPending ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Reversing...
              </>
            ) : (
              <>
                <Undo2 className="h-4 w-4" />
                Reverse Fusion
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );

  // -------------------------------------------------------------------------
  // Main Render
  // -------------------------------------------------------------------------

  return (
    <div className="space-y-4 pb-8">
      {renderHeader()}
      {renderQuickStats()}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        {/* Left: Candidate Finder */}
        <motion.div
          initial={{ opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.16, delay: 0.1 }}
        >
          {renderCandidateFinder()}
        </motion.div>

        {/* Center: Review Queue */}
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.16, delay: 0.15 }}
          className="lg:col-span-2"
        >
          {renderReviewQueue()}
        </motion.div>
      </div>

      {/* Bottom: History */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.16, delay: 0.2 }}
      >
        {renderHistory()}
      </motion.div>

      {/* Dialogs */}
      {renderComparisonDialog()}
      {renderCommitDialog()}
      {renderReverseDialog()}
    </div>
  );
}
