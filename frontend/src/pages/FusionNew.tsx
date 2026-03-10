/**
 * Entity Fusion - Production-Ready UI
 *
 * 4-Phase Workflow: Propose → Review → Commit → Reverse
 * Enterprise-grade entity resolution and deduplication console
 */

import React, { useState, useMemo, useCallback, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';
import { motion, AnimatePresence } from 'framer-motion';
import {
  GitMerge,
  Play,
  Settings2,
  CheckCircle2,
  X,
  Eye,
  AlertTriangle,
  Mail,
  Phone,
  ShieldCheck,
  User,
  Download,
  Undo2,
  Filter,
  ArrowUpDown,
  Zap,
  Clock,
  Check,
  XCircle,
  Loader2
} from 'lucide-react';

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Slider } from '@/components/ui/slider';
import { Progress } from '@/components/ui/progress';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Textarea } from '@/components/ui/textarea';
import { toast } from 'sonner';

import {
  useFusionCandidates,
  useProposeFusion,
  useApproveFusionCandidate,
  useRejectFusionCandidate,
  useResolveFusion,
  useReverseFusion,
  useFusionHistory
} from '@/hooks/useFusion';
import { useDatasets } from '@/hooks/useDatasets';
import type { FusionCandidate, FusionCandidateStatus } from '@/api/types';
import { useAuthStore } from '@/stores/auth';

// ============================================================================
// Types & Constants
// ============================================================================

type FusionMatchRule = 'email' | 'phone' | 'ssn' | 'name';
type SortBy = 'confidence' | 'date' | 'entities';
type SortOrder = 'asc' | 'desc';

const MATCH_RULE_CONFIG = {
  email: {
    icon: Mail,
    label: 'Email Match',
    defaultConfidence: 95,
    description: 'Exact email address matching'
  },
  phone: {
    icon: Phone,
    label: 'Phone Match',
    defaultConfidence: 90,
    description: 'Normalized phone number matching'
  },
  ssn: {
    icon: ShieldCheck,
    label: 'SSN Match',
    defaultConfidence: 99,
    description: 'Social Security Number matching'
  },
  name: {
    icon: User,
    label: 'Name Match',
    defaultConfidence: 70,
    description: 'Fuzzy name similarity matching'
  }
} as const;

function getConfidenceColor(confidence: number): string {
  if (confidence >= 0.90) return 'success';
  if (confidence >= 0.75) return 'warning';
  return 'error';
}

function getConfidenceLabel(confidence: number): string {
  if (confidence >= 0.90) return 'High';
  if (confidence >= 0.75) return 'Medium';
  return 'Low';
}

// ============================================================================
// Main Component
// ============================================================================

export function FusionNew() {
  const user = useAuthStore((state) => state.user);
  const [searchParams] = useSearchParams();

  // ========== State Management ==========
  const [selectedRule, setSelectedRule] = useState<FusionMatchRule>('phone');
  const [minConfidence, setMinConfidence] = useState([75]);
  const [dataset, setDataset] = useState('');
  const [autoCommit, setAutoCommit] = useState(false);
  const [autoCommitThreshold, setAutoCommitThreshold] = useState(95);

  // Pre-select dataset from URL query parameter
  useEffect(() => {
    const datasetParam = searchParams.get('dataset');
    if (datasetParam && !dataset) {
      setDataset(datasetParam);
    }
  }, [searchParams, dataset]);

  // UI State
  const [statusFilter, setStatusFilter] = useState<FusionCandidateStatus | 'all'>('all');
  const [sortBy, setSortBy] = useState<SortBy>('confidence');
  const [sortOrder, setSortOrder] = useState<SortOrder>('desc');
  const [selectedCandidates, setSelectedCandidates] = useState<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = useState('');

  // Dialog State
  const [comparisonDialogOpen, setComparisonDialogOpen] = useState(false);
  const [activeCandidate, setActiveCandidate] = useState<FusionCandidate | null>(null);
  const [reviewNotes, setReviewNotes] = useState('');
  const [commitDialogOpen, setCommitDialogOpen] = useState(false);
  const [reverseDialogOpen, setReverseDialogOpen] = useState(false);
  const [reversalReason, setReversalReason] = useState('');
  const [fusionToReverse, setFusionToReverse] = useState<string | null>(null);

  // View State
  const [activeTab, setActiveTab] = useState<'candidates' | 'history'>('candidates');

  // ========== Data Fetching ==========
  const { data: candidatesData, isLoading: isLoadingCandidates, refetch: refetchCandidates } =
    useFusionCandidates(
      statusFilter !== 'all' ? { status: statusFilter } : undefined
    );

  const candidates = candidatesData?.candidates || [];

  const { data: fusionHistory, isLoading: isLoadingHistory } = useFusionHistory({ limit: 100 });

  // Fetch datasets for the selector
  const { data: datasetsData, isLoading: isLoadingDatasets } = useDatasets();

  const proposeMutation = useProposeFusion();
  const approveMutation = useApproveFusionCandidate();
  const rejectMutation = useRejectFusionCandidate();
  const resolveMutation = useResolveFusion();
  const reverseMutation = useReverseFusion();

  // ========== Computed Values ==========
  const filteredCandidates = useMemo(() => {
    let filtered = [...candidates];

    // Filter by confidence
    filtered = filtered.filter(c => c.confidence >= minConfidence[0] / 100);

    // Filter by search
    if (searchQuery) {
      const query = searchQuery.toLowerCase();
      filtered = filtered.filter((c: FusionCandidate) =>
        c.entities.some((e: any) => e.id.toLowerCase().includes(query)) ||
        c.match_value.toLowerCase().includes(query)
      );
    }

    // Sort
    filtered.sort((a, b) => {
      let aVal, bVal;
      switch (sortBy) {
        case 'confidence':
          aVal = a.confidence;
          bVal = b.confidence;
          break;
        case 'date':
          aVal = new Date(a.proposed_at).getTime();
          bVal = new Date(b.proposed_at).getTime();
          break;
        case 'entities':
          aVal = a.entities[0]?.id || '';
          bVal = b.entities[0]?.id || '';
          break;
        default:
          return 0;
      }

      if (sortOrder === 'asc') {
        return aVal > bVal ? 1 : -1;
      } else {
        return aVal < bVal ? 1 : -1;
      }
    });

    return filtered;
  }, [candidates, minConfidence, searchQuery, sortBy, sortOrder]);

  const approvedCandidates = useMemo(
    () => candidates.filter((c: FusionCandidate) => c.status === 'approved'),
    [candidates]
  );

  const stats = useMemo(() => ({
    total: candidates.length,
    proposed: candidates.filter((c: FusionCandidate) => c.status === 'proposed').length,
    approved: candidates.filter((c: FusionCandidate) => c.status === 'approved').length,
    rejected: candidates.filter((c: FusionCandidate) => c.status === 'rejected').length,
    highConfidence: candidates.filter((c: FusionCandidate) => c.confidence >= 0.90).length
  }), [candidates]);

  // ========== Event Handlers ==========
  const handlePropose = useCallback(() => {
    if (!dataset) {
      toast.error('Please select a dataset');
      return;
    }

    proposeMutation.mutate({
      dataset,
      rule: selectedRule,
      min_confidence: minConfidence[0] / 100
    });
  }, [dataset, selectedRule, minConfidence, proposeMutation]);

  const handleApprove = useCallback((candidate: FusionCandidate) => {
    if (!user?.id) {
      toast.error('User not authenticated', {
        description: 'Please log in to approve candidates'
      });
      return;
    }

    approveMutation.mutate({
      candidateId: candidate.candidate_id,
      request: {
        reviewer: user.id,
        notes: reviewNotes || undefined
      }
    }, {
      onSuccess: () => {
        setReviewNotes('');
        setComparisonDialogOpen(false);
        setActiveCandidate(null);
      }
    });
  }, [user, reviewNotes, approveMutation]);

  const handleReject = useCallback((candidate: FusionCandidate) => {
    if (!user?.id) {
      toast.error('User not authenticated', {
        description: 'Please log in to reject candidates'
      });
      return;
    }

    rejectMutation.mutate({
      candidateId: candidate.candidate_id,
      request: {
        reviewer: user.id,
        notes: reviewNotes || undefined
      }
    }, {
      onSuccess: () => {
        setReviewNotes('');
        setComparisonDialogOpen(false);
        setActiveCandidate(null);
      }
    });
  }, [user, reviewNotes, rejectMutation]);

  const handleCommitAll = useCallback(() => {
    if (approvedCandidates.length === 0) {
      toast.error('No approved candidates to commit');
      return;
    }

    // Commit each approved candidate sequentially
    const commitPromises = approvedCandidates.map((candidate: FusionCandidate) =>
      resolveMutation.mutateAsync({
        entities: candidate.entities,  // Pass full entity objects, not just IDs
        rule: candidate.match_rule,
        confidence: candidate.confidence
      })
    );

    Promise.all(commitPromises)
      .then(() => {
        setCommitDialogOpen(false);
        toast.success(`Committed ${approvedCandidates.length} fusions`);
      })
      .catch((error) => {
        toast.error('Some fusions failed to commit', {
          description: error.message
        });
      });
  }, [approvedCandidates, resolveMutation]);

  const handleBulkApprove = useCallback(() => {
    if (!user?.id) {
      toast.error('User not authenticated', {
        description: 'Please log in to approve candidates'
      });
      return;
    }

    if (selectedCandidates.size === 0) {
      toast.error('No candidates selected');
      return;
    }

    const promises = Array.from(selectedCandidates).map(candidateId => {
      return approveMutation.mutateAsync({
        candidateId,
        request: {
          reviewer: user.id,
          notes: 'Bulk approved'
        }
      });
    });

    Promise.all(promises)
      .then(() => {
        setSelectedCandidates(new Set());
        toast.success(`Approved ${selectedCandidates.size} candidates`);
      })
      .catch((error) => {
        toast.error('Bulk approval failed', {
          description: error.message
        });
      });
  }, [user, selectedCandidates, approveMutation]);

  const handleBulkReject = useCallback(() => {
    if (!user?.id) {
      toast.error('User not authenticated', {
        description: 'Please log in to reject candidates'
      });
      return;
    }

    if (selectedCandidates.size === 0) {
      toast.error('No candidates selected');
      return;
    }

    const promises = Array.from(selectedCandidates).map(candidateId => {
      return rejectMutation.mutateAsync({
        candidateId,
        request: {
          reviewer: user.id,
          notes: 'Bulk rejected'
        }
      });
    });

    Promise.all(promises)
      .then(() => {
        setSelectedCandidates(new Set());
        toast.success(`Rejected ${selectedCandidates.size} candidates`);
      })
      .catch((error) => {
        toast.error('Bulk rejection failed', {
          description: error.message
        });
      });
  }, [user, selectedCandidates, rejectMutation]);

  const toggleSelection = useCallback((candidateId: string) => {
    setSelectedCandidates(prev => {
      const next = new Set(prev);
      if (next.has(candidateId)) {
        next.delete(candidateId);
      } else {
        next.add(candidateId);
      }
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    setSelectedCandidates(
      new Set(filteredCandidates.map(c => c.candidate_id))
    );
  }, [filteredCandidates]);

  const clearSelection = useCallback(() => {
    setSelectedCandidates(new Set());
  }, []);

  const handleReverseFusion = useCallback(() => {
    if (!fusionToReverse) return;

    reverseMutation.mutate({
      fusionId: fusionToReverse,
      request: {
        reason: reversalReason || 'Reversed by user'
      }
    }, {
      onSuccess: () => {
        setReverseDialogOpen(false);
        setFusionToReverse(null);
        setReversalReason('');
      }
    });
  }, [fusionToReverse, reversalReason, reverseMutation]);

  // ========== Render ==========
  return (
    <div className="space-y-4 pb-8">
      {/* Page Header */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15 }}
        className="flex flex-col gap-4 pb-4 border-b border-border"
      >
        <div className="flex items-start justify-between">
          <div>
            <h1 className="text-2xl font-semibold text-foreground mb-1">
              Entity Fusion
            </h1>
            <p className="text-sm text-muted-foreground">
              AI-assisted entity resolution and deduplication workflow
            </p>
          </div>

          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              className="gap-2"
              onClick={() => refetchCandidates()}
            >
              <Filter className="h-4 w-4" />
              Refresh
            </Button>
            <Button
              variant="default"
              size="sm"
              className="gap-2"
              onClick={handlePropose}
              disabled={!dataset || proposeMutation.isPending}
            >
              {proposeMutation.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Play className="h-4 w-4" />
              )}
              Find Candidates
            </Button>
          </div>
        </div>

        {/* Stats Bar */}
        <div className="grid grid-cols-5 gap-4">
          <div className="flex flex-col">
            <span className="text-xs text-muted-foreground">Total</span>
            <span className="text-xl font-semibold font-mono">{stats.total}</span>
          </div>
          <div className="flex flex-col">
            <span className="text-xs text-muted-foreground">Proposed</span>
            <span className="text-xl font-semibold font-mono text-info">{stats.proposed}</span>
          </div>
          <div className="flex flex-col">
            <span className="text-xs text-muted-foreground">Approved</span>
            <span className="text-xl font-semibold font-mono text-success">{stats.approved}</span>
          </div>
          <div className="flex flex-col">
            <span className="text-xs text-muted-foreground">Rejected</span>
            <span className="text-xl font-semibold font-mono text-error">{stats.rejected}</span>
          </div>
          <div className="flex flex-col">
            <span className="text-xs text-muted-foreground">High Confidence</span>
            <span className="text-xl font-semibold font-mono">{stats.highConfidence}</span>
          </div>
        </div>
      </motion.div>

      <div className="grid gap-4 lg:grid-cols-[320px,1fr]">
        {/* Left Panel: Rule Configuration */}
        <motion.div
          initial={{ opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.15, delay: 0.05 }}
        >
          <Card className="glass-morphism border-border">
            <CardHeader>
              <div className="flex items-center gap-2">
                <Settings2 className="h-5 w-5 text-info" />
                <CardTitle>Configuration</CardTitle>
              </div>
              <CardDescription>
                Configure matching rules and threshold
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {/* Dataset Selector */}
              <div className="space-y-2">
                <Label htmlFor="dataset">Dataset</Label>
                <Select value={dataset} onValueChange={setDataset} disabled={isLoadingDatasets}>
                  <SelectTrigger id="dataset">
                    <SelectValue placeholder={isLoadingDatasets ? "Loading datasets..." : "Select dataset..."} />
                  </SelectTrigger>
                  <SelectContent>
                    {datasetsData?.datasets && datasetsData.datasets.length > 0 ? (
                      datasetsData.datasets.map((ds) => (
                        <SelectItem key={ds.id} value={ds.id}>
                          {ds.name}
                          {ds.quality_score !== undefined && (
                            <span className="ml-2 text-xs text-muted-foreground">
                              ({ds.quality_score}% quality)
                            </span>
                          )}
                        </SelectItem>
                      ))
                    ) : (
                      <SelectItem value="_none" disabled>
                        No datasets available
                      </SelectItem>
                    )}
                  </SelectContent>
                </Select>
                {datasetsData?.datasets && datasetsData.datasets.length > 0 && dataset && (
                  <p className="text-xs text-muted-foreground">
                    {datasetsData.datasets.find(d => d.id === dataset)?.description || ''}
                  </p>
                )}
              </div>

              <Separator />

              {/* Matching Rule Selector */}
              <div className="space-y-3">
                <Label>Matching Rule</Label>
                {(Object.entries(MATCH_RULE_CONFIG) as Array<[FusionMatchRule, typeof MATCH_RULE_CONFIG[FusionMatchRule]]>).map(
                  ([rule, config]) => {
                    const Icon = config.icon;
                    return (
                      <button
                        key={rule}
                        onClick={() => setSelectedRule(rule)}
                        className={`
                          w-full flex items-start gap-3 p-3 rounded-sm border
                          transition-all duration-150
                          ${
                            selectedRule === rule
                              ? 'border-info bg-info/5'
                              : 'border-border hover:border-border-emphasis bg-white'
                          }
                        `}
                      >
                        <Icon className={`h-5 w-5 mt-0.5 ${selectedRule === rule ? 'text-info' : 'text-muted-foreground'}`} />
                        <div className="flex-1 text-left">
                          <div className="text-sm font-medium">{config.label}</div>
                          <div className="text-xs text-muted-foreground">
                            {config.description}
                          </div>
                          <div className="text-xs text-muted-foreground mt-1">
                            Default: {config.defaultConfidence}% confidence
                          </div>
                        </div>
                        {selectedRule === rule && (
                          <CheckCircle2 className="h-5 w-5 text-info" />
                        )}
                      </button>
                    );
                  }
                )}
              </div>

              <Separator />

              {/* Confidence Threshold */}
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label>Min Confidence</Label>
                  <span className="text-sm font-mono font-medium">{minConfidence[0]}%</span>
                </div>
                <Slider
                  value={minConfidence}
                  onValueChange={setMinConfidence}
                  min={50}
                  max={99}
                  step={1}
                  className="w-full"
                />
                <p className="text-xs text-muted-foreground">
                  Only show candidates above this threshold
                </p>
              </div>

              <Separator />

              {/* Auto-Commit Settings */}
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label htmlFor="auto-commit">Auto-Commit</Label>
                  <Switch
                    id="auto-commit"
                    checked={autoCommit}
                    onCheckedChange={setAutoCommit}
                  />
                </div>
                {autoCommit && (
                  <div className="pl-2 space-y-2">
                    <Label className="text-xs">Threshold: {autoCommitThreshold}%</Label>
                    <Slider
                      value={[autoCommitThreshold]}
                      onValueChange={(val) => setAutoCommitThreshold(val[0])}
                      min={90}
                      max={99}
                      step={1}
                    />
                    <p className="text-xs text-muted-foreground">
                      Auto-commit fusions ≥{autoCommitThreshold}%
                    </p>
                  </div>
                )}
              </div>
            </CardContent>
          </Card>
        </motion.div>

        {/* Right Panel: Candidate Queue */}
        <motion.div
          initial={{ opacity: 0, x: 8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.15, delay: 0.1 }}
          className="space-y-4"
        >
          {/* Command Bar */}
          <Card className="glass-morphism border-border">
            <CardContent className="p-4">
              <div className="flex items-center justify-between gap-4">
                <div className="flex items-center gap-2 flex-1">
                  <Input
                    placeholder="Search by entity ID or match value..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="max-w-sm"
                  />
                  <Select value={statusFilter} onValueChange={(v) => setStatusFilter(v as any)}>
                    <SelectTrigger className="w-[140px]">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All Status</SelectItem>
                      <SelectItem value="proposed">Proposed</SelectItem>
                      <SelectItem value="approved">Approved</SelectItem>
                      <SelectItem value="rejected">Rejected</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="gap-2"
                    onClick={() => {
                      setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc');
                    }}
                  >
                    <ArrowUpDown className="h-4 w-4" />
                    {sortOrder === 'desc' ? 'Desc' : 'Asc'}
                  </Button>

                  {approvedCandidates.length > 0 && (
                    <Button
                      variant="default"
                      size="sm"
                      className="gap-2"
                      onClick={() => setCommitDialogOpen(true)}
                    >
                      <Zap className="h-4 w-4" />
                      Commit {approvedCandidates.length}
                    </Button>
                  )}
                </div>
              </div>

              {/* Bulk Actions Toolbar */}
              <AnimatePresence>
                {selectedCandidates.size > 0 && (
                  <motion.div
                    initial={{ opacity: 0, height: 0 }}
                    animate={{ opacity: 1, height: 'auto' }}
                    exit={{ opacity: 0, height: 0 }}
                    className="flex items-center justify-between gap-2 pt-3 mt-3 border-t border-border"
                  >
                    <span className="text-sm text-muted-foreground">
                      {selectedCandidates.size} selected
                    </span>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={clearSelection}
                      >
                        Clear
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={selectAll}
                      >
                        Select All ({filteredCandidates.length})
                      </Button>
                      <Button
                        variant="destructive"
                        size="sm"
                        className="gap-2"
                        onClick={handleBulkReject}
                        disabled={rejectMutation.isPending}
                      >
                        <XCircle className="h-4 w-4" />
                        Reject All
                      </Button>
                      <Button
                        variant="default"
                        size="sm"
                        className="gap-2"
                        onClick={handleBulkApprove}
                        disabled={approveMutation.isPending}
                      >
                        <CheckCircle2 className="h-4 w-4" />
                        Approve All
                      </Button>
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </CardContent>
          </Card>

          {/* Candidate List */}
          <Card className="glass-morphism border-border">
            <CardHeader>
              <CardTitle>
                Fusion Candidates
                {filteredCandidates.length > 0 && (
                  <span className="ml-2 text-sm font-normal text-muted-foreground">
                    ({filteredCandidates.length} shown)
                  </span>
                )}
              </CardTitle>
              <CardDescription>
                Review and approve potential duplicates
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3">
              {isLoadingCandidates ? (
                <div className="flex items-center justify-center py-12">
                  <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                </div>
              ) : filteredCandidates.length === 0 ? (
                <div className="text-center py-12">
                  <GitMerge className="h-12 w-12 text-muted-foreground mx-auto mb-4 opacity-50" />
                  <h3 className="text-lg font-semibold mb-2">No Candidates</h3>
                  <p className="text-sm text-muted-foreground mb-4">
                    {candidates.length === 0
                      ? 'Run matching rules to find duplicates'
                      : 'No candidates match current filters'}
                  </p>
                  {candidates.length > 0 && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        setStatusFilter('all');
                        setMinConfidence([50]);
                        setSearchQuery('');
                      }}
                    >
                      Clear Filters
                    </Button>
                  )}
                </div>
              ) : (
                filteredCandidates.map((candidate, index) => (
                  <FusionCandidateCard
                    key={candidate.candidate_id}
                    candidate={candidate}
                    index={index}
                    isSelected={selectedCandidates.has(candidate.candidate_id)}
                    onToggleSelection={() => toggleSelection(candidate.candidate_id)}
                    onCompare={() => {
                      setActiveCandidate(candidate);
                      setComparisonDialogOpen(true);
                    }}
                    onApprove={() => handleApprove(candidate)}
                    onReject={() => handleReject(candidate)}
                    isApproving={approveMutation.isPending}
                    isRejecting={rejectMutation.isPending}
                  />
                ))
              )}
            </CardContent>
          </Card>
        </motion.div>
      </div>

      {/* Comparison Dialog */}
      <ComparisonDialog
        open={comparisonDialogOpen}
        onOpenChange={setComparisonDialogOpen}
        candidate={activeCandidate}
        reviewNotes={reviewNotes}
        onReviewNotesChange={setReviewNotes}
        onApprove={() => activeCandidate && handleApprove(activeCandidate)}
        onReject={() => activeCandidate && handleReject(activeCandidate)}
        isApproving={approveMutation.isPending}
        isRejecting={rejectMutation.isPending}
      />

      {/* Commit Confirmation Dialog */}
      <AlertDialog open={commitDialogOpen} onOpenChange={setCommitDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Commit {approvedCandidates.length} Fusions?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently merge the following entity pairs:
              <div className="mt-3 max-h-48 overflow-y-auto space-y-1 p-2 bg-background-secondary rounded-sm">
                {approvedCandidates.slice(0, 10).map(c => (
                  <div key={c.candidate_id} className="text-xs font-mono">
                    {c.entities[0]?.id} ↔ {c.entities[1]?.id}
                  </div>
                ))}
                {approvedCandidates.length > 10 && (
                  <div className="text-xs text-muted-foreground">
                    ... and {approvedCandidates.length - 10} more
                  </div>
                )}
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleCommitAll}
              disabled={resolveMutation.isPending}
            >
              {resolveMutation.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  Committing...
                </>
              ) : (
                <>
                  <Zap className="h-4 w-4 mr-2" />
                  Commit All
                </>
              )}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

// ============================================================================
// Sub-Components
// ============================================================================

interface FusionCandidateCardProps {
  candidate: FusionCandidate;
  index: number;
  isSelected: boolean;
  onToggleSelection: () => void;
  onCompare: () => void;
  onApprove: () => void;
  onReject: () => void;
  isApproving: boolean;
  isRejecting: boolean;
}

function FusionCandidateCard({
  candidate,
  index,
  isSelected,
  onToggleSelection,
  onCompare,
  onApprove,
  onReject,
  isApproving,
  isRejecting
}: FusionCandidateCardProps) {
  const confidenceColor = getConfidenceColor(candidate.confidence);
  const confidenceLabel = getConfidenceLabel(candidate.confidence);
  const confidencePercent = Math.round(candidate.confidence * 100);

  const entity1 = candidate.entities[0];
  const entity2 = candidate.entities[1];

  const statusConfig = {
    proposed: { color: 'bg-info/10 text-info border-info/20', label: 'Proposed' },
    approved: { color: 'bg-success/10 text-success border-success/20', label: 'Approved' },
    rejected: { color: 'bg-error/10 text-error border-error/20', label: 'Rejected' },
    committed: { color: 'bg-secondary/10 text-secondary border-secondary/20', label: 'Committed' }
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.15, delay: index * 0.02 }}
      className={`
        p-4 rounded-sm border-2 transition-all duration-150
        ${isSelected ? 'border-info bg-info/5' : 'border-border bg-white hover:border-border-emphasis'}
      `}
    >
      {/* Header */}
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-3">
          <input
            type="checkbox"
            checked={isSelected}
            onChange={onToggleSelection}
            className="h-4 w-4 rounded border-border"
          />
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm font-medium text-entity">
              {entity1?.id || 'N/A'}
            </span>
            <GitMerge className="h-4 w-4 text-muted-foreground" />
            <span className="font-mono text-sm font-medium text-entity">
              {entity2?.id || 'N/A'}
            </span>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Progress
            value={confidencePercent}
            className={`w-16 h-2 [&>div]:bg-${confidenceColor}`}
          />
          <Badge variant={confidenceColor as any} className="text-xs">
            {confidencePercent}%
          </Badge>
        </div>
      </div>

      {/* Match Info */}
      <div className={`mb-3 p-2 rounded-sm border ${
        confidenceColor === 'success' ? 'bg-success/5 border-success/20' :
        confidenceColor === 'warning' ? 'bg-warning/5 border-warning/20' :
        'bg-error/5 border-error/20'
      }`}>
        <div className="text-xs text-muted-foreground mb-0.5">
          Match: {candidate.match_rule}
        </div>
        <div className="font-mono text-sm text-foreground">
          {candidate.match_value}
        </div>
      </div>

      {/* Metadata */}
      <div className="flex items-center gap-4 text-xs text-muted-foreground mb-3">
        <span className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          {new Date(candidate.proposed_at).toLocaleString()}
        </span>
        {candidate.reviewed_by && (
          <span>Reviewed by {candidate.reviewed_by}</span>
        )}
        <Badge className={statusConfig[candidate.status].color}>
          {statusConfig[candidate.status].label}
        </Badge>
      </div>

      {/* Actions */}
      <div className="flex gap-2">
        {candidate.status === 'proposed' && (
          <>
            <Button
              size="sm"
              variant="default"
              className="flex-1 gap-2"
              onClick={onApprove}
              disabled={isApproving}
            >
              {isApproving ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <CheckCircle2 className="h-4 w-4" />
              )}
              Approve
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="flex-1 gap-2"
              onClick={onCompare}
            >
              <Eye className="h-4 w-4" />
              Compare
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={onReject}
              disabled={isRejecting}
            >
              {isRejecting ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <X className="h-4 w-4" />
              )}
            </Button>
          </>
        )}
        {candidate.status !== 'proposed' && (
          <Button
            size="sm"
            variant="outline"
            className="w-full gap-2"
            onClick={onCompare}
          >
            <Eye className="h-4 w-4" />
            View Details
          </Button>
        )}
      </div>

      {candidate.review_notes && (
        <div className="mt-3 pt-3 border-t border-border-subtle">
          <div className="text-xs text-muted-foreground mb-1">Review Notes:</div>
          <div className="text-xs text-foreground-secondary italic">
            "{candidate.review_notes}"
          </div>
        </div>
      )}
    </motion.div>
  );
}

interface ComparisonDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  candidate: FusionCandidate | null;
  reviewNotes: string;
  onReviewNotesChange: (notes: string) => void;
  onApprove: () => void;
  onReject: () => void;
  isApproving: boolean;
  isRejecting: boolean;
}

function ComparisonDialog({
  open,
  onOpenChange,
  candidate,
  reviewNotes,
  onReviewNotesChange,
  onApprove,
  onReject,
  isApproving,
  isRejecting
}: ComparisonDialogProps) {
  if (!candidate) return null;

  const entity1 = candidate.entities[0] || {};
  const entity2 = candidate.entities[1] || {};

  // Get all unique keys from both entities
  const allKeys = Array.from(
    new Set([...Object.keys(entity1), ...Object.keys(entity2)])
  ).filter(key => key !== 'id');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            Comparing Entities
            <span className="font-mono text-sm text-muted-foreground">
              {entity1.id} ↔ {entity2.id}
            </span>
          </DialogTitle>
          <DialogDescription>
            Match: {candidate.match_rule} | Confidence: {Math.round(candidate.confidence * 100)}%
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {/* Side-by-Side Comparison */}
          <div className="grid grid-cols-2 gap-4">
            {/* Entity 1 */}
            <Card className="border-border">
              <CardHeader className="pb-3">
                <CardTitle className="text-sm font-mono">{entity1.id}</CardTitle>
                <CardDescription>Source Entity</CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                {allKeys.map(key => (
                  <div key={key} className="flex justify-between text-xs">
                    <span className="text-muted-foreground">{key}:</span>
                    <span className="font-mono">
                      {entity1[key]?.toString() || <span className="text-muted-foreground">—</span>}
                    </span>
                  </div>
                ))}
              </CardContent>
            </Card>

            {/* Entity 2 */}
            <Card className="border-border">
              <CardHeader className="pb-3">
                <CardTitle className="text-sm font-mono">{entity2.id}</CardTitle>
                <CardDescription>Source Entity</CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                {allKeys.map(key => (
                  <div key={key} className="flex justify-between text-xs">
                    <span className="text-muted-foreground">{key}:</span>
                    <span className="font-mono">
                      {entity2[key]?.toString() || <span className="text-muted-foreground">—</span>}
                    </span>
                  </div>
                ))}
              </CardContent>
            </Card>
          </div>

          {/* Review Notes */}
          <div className="space-y-2">
            <Label htmlFor="review-notes">Review Notes (Optional)</Label>
            <Textarea
              id="review-notes"
              placeholder="Add context for this decision..."
              value={reviewNotes}
              onChange={(e) => onReviewNotesChange(e.target.value)}
              rows={3}
            />
          </div>
        </div>

        <DialogFooter className="gap-2">
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={onReject}
            disabled={isRejecting}
            className="gap-2"
          >
            {isRejecting ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <XCircle className="h-4 w-4" />
            )}
            Reject
          </Button>
          <Button
            variant="default"
            onClick={onApprove}
            disabled={isApproving}
            className="gap-2"
          >
            {isApproving ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <CheckCircle2 className="h-4 w-4" />
            )}
            Approve
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
