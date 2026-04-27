import React, { useEffect, useMemo, useState } from 'react';
import {
  History,
  KeyRound,
  Loader2,
  RefreshCw,
  ScrollText,
  ShieldCheck,
} from 'lucide-react';

import type {
  SosContractApprovalRequest,
  SosContractSignature,
  SosDataContract,
  SosPolicyApprovalRequest,
  SosPolicyAttestation,
  SosPolicyRecord,
  SosReconcileResponse,
} from '@/api/sosValidation';
import {
  useReconcileSosRuntime,
  useRotateSosContractSigningKey,
  useRotateSosPolicySigningKey,
  useSosContractApprovalRequests,
  useSosContractSignatures,
  useSosContractSigningKeyStatus,
  useSosContracts,
  useSosPolicies,
  useSosPolicyApprovalRequests,
  useSosPolicyAttestations,
  useSosPolicySigningKeyStatus,
} from '@/hooks/useSosValidation';
import { hasPermission, useAuthStore } from '@/stores/auth';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';

const AUDIT_LIMIT = 10;

export function SosOperationsPanel() {
  const user = useAuthStore((state) => state.user);
  const canOperate = hasPermission(user, 'Admin');

  const { data: contractsResponse, isLoading: isLoadingContracts, error: contractsError } =
    useSosContracts();
  const { data: policiesResponse, isLoading: isLoadingPolicies, error: policiesError } =
    useSosPolicies();
  const { data: contractKeyStatus, isLoading: isLoadingContractKey } =
    useSosContractSigningKeyStatus();
  const { data: policyKeyStatus, isLoading: isLoadingPolicyKey } =
    useSosPolicySigningKeyStatus();

  const [selectedContractId, setSelectedContractId] = useState('');
  const [selectedPolicyId, setSelectedPolicyId] = useState('');
  const [includeOntologySync, setIncludeOntologySync] = useState(true);
  const [contractRotateReason, setContractRotateReason] = useState('');
  const [policyRotateReason, setPolicyRotateReason] = useState('');
  const [policyTrustMode, setPolicyTrustMode] = useState('');
  const [policyTrustProvider, setPolicyTrustProvider] = useState('');
  const [policyExternalKeyRef, setPolicyExternalKeyRef] = useState('');
  const [policyTrustAttestationRef, setPolicyTrustAttestationRef] = useState('');
  const [lastReconcile, setLastReconcile] = useState<SosReconcileResponse | null>(null);

  const contracts = useMemo(() => contractsResponse ?? [], [contractsResponse]);
  const policies = useMemo(() => policiesResponse?.policies ?? [], [policiesResponse]);
  const governanceOverview = useMemo(
    () => buildGovernanceOverview({
      contracts,
      policies,
      contractKeyStatus,
      policyKeyStatus,
    }),
    [contractKeyStatus, contracts, policies, policyKeyStatus]
  );

  useEffect(() => {
    if (!selectedContractId && contracts.length > 0) {
      setSelectedContractId(contracts[0].contract_id);
    }
  }, [contracts, selectedContractId]);

  useEffect(() => {
    if (!selectedPolicyId && policies.length > 0) {
      setSelectedPolicyId(policies[0].policy_id);
    }
  }, [policies, selectedPolicyId]);

  const selectedContract = contracts.find((contract) => contract.contract_id === selectedContractId);
  const selectedPolicy = policies.find((policy) => policy.policy_id === selectedPolicyId);

  const contractApprovals = useSosContractApprovalRequests(
    selectedContractId || null,
    { limit: AUDIT_LIMIT }
  );
  const contractSignatures = useSosContractSignatures(selectedContractId || null, AUDIT_LIMIT);
  const policyApprovals = useSosPolicyApprovalRequests(selectedPolicyId || null, {
    limit: AUDIT_LIMIT,
  });
  const policyAttestations = useSosPolicyAttestations(selectedPolicyId || null, AUDIT_LIMIT);

  const reconcile = useReconcileSosRuntime();
  const rotateContractKey = useRotateSosContractSigningKey();
  const rotatePolicyKey = useRotateSosPolicySigningKey();

  const handleReconcile = async () => {
    const result = await reconcile.mutateAsync({
      include_ontology_sync: includeOntologySync,
    });
    setLastReconcile(result);
  };

  const handleRotateContractKey = async () => {
    await rotateContractKey.mutateAsync({
      reason: contractRotateReason.trim() || undefined,
    });
    setContractRotateReason('');
  };

  const handleRotatePolicyKey = async () => {
    await rotatePolicyKey.mutateAsync({
      reason: policyRotateReason.trim() || undefined,
      trust_mode: policyTrustMode.trim() || undefined,
      trust_provider: policyTrustProvider.trim() || undefined,
      external_key_ref: policyExternalKeyRef.trim() || undefined,
      trust_attestation_ref: policyTrustAttestationRef.trim() || undefined,
    });
    setPolicyRotateReason('');
  };

  return (
    <div className="space-y-4">
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.25fr),minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <RefreshCw className="h-4 w-4" />
              Recovery Controls
            </CardTitle>
            <CardDescription>
              Run the admin-only SoS reconcile path when you need to repair graph drift, verify a
              restart replay, or re-sync ontology-backed SoS assets.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between gap-4 rounded-sm border border-border bg-background p-3">
              <div className="space-y-1">
                <div className="font-medium text-foreground">Include ontology sync</div>
                <div className="text-xs text-muted-foreground">
                  Keep this on for the canonical operator path. Turn it off only when you want a
                  graph-only rebuild.
                </div>
              </div>
              <Switch
                checked={includeOntologySync}
                onCheckedChange={setIncludeOntologySync}
                aria-label="Include ontology sync"
              />
            </div>

            <div className="flex flex-wrap items-center gap-3">
              <Button onClick={handleReconcile} disabled={!canOperate || reconcile.isPending}>
                {reconcile.isPending ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="mr-2 h-4 w-4" />
                )}
                Run Reconcile
              </Button>
              {!canOperate && (
                <Badge variant="warning">Admin or service role required for runtime reconcile</Badge>
              )}
            </div>

            {lastReconcile ? (
              <div className="rounded-sm border border-border bg-background-secondary p-3">
                <div className="mb-3 flex flex-wrap items-center gap-2">
                  <Badge variant="success">Last reconcile completed</Badge>
                  <Badge variant="outline">{lastReconcile.duration_ms} ms</Badge>
                  <Badge variant="outline">
                    {lastReconcile.graph_reconcile_performed ? 'Graph rebuilt' : 'Graph skipped'}
                  </Badge>
                  <Badge variant="outline">
                    {lastReconcile.ontology_sync_performed ? 'Ontology synced' : 'Ontology skipped'}
                  </Badge>
                </div>
                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                  <InfoBlock label="Triggered By" value={lastReconcile.triggered_by} />
                  <InfoBlock
                    label="Systems / Interfaces"
                    value={`${lastReconcile.system_count} / ${lastReconcile.interface_count}`}
                  />
                  <InfoBlock
                    label="Contracts / Policies"
                    value={`${lastReconcile.contract_count} / ${lastReconcile.policy_count}`}
                  />
                  <InfoBlock label="Completed" value={formatTimestamp(lastReconcile.completed_at)} />
                </div>
              </div>
            ) : (
              <EmptyAuditState
                title="No reconcile run recorded in this session"
                description="Run reconcile here when you want an operator-visible confirmation of the recovery path."
              />
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ShieldCheck className="h-4 w-4" />
              Surface Health
            </CardTitle>
            <CardDescription>
              Keep the signing-key lifecycle and governance state visible without dropping down to
              raw API calls.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <KeyStatusCard
              title="Contract Signing Key"
              status={contractKeyStatus}
              isLoading={isLoadingContractKey}
              canRotate={canOperate}
              rotationReason={contractRotateReason}
              onRotationReasonChange={setContractRotateReason}
              onRotate={handleRotateContractKey}
              isRotating={rotateContractKey.isPending}
            />
            <KeyStatusCard
              title="Policy Signing Key"
              status={policyKeyStatus}
              isLoading={isLoadingPolicyKey}
              canRotate={canOperate}
              rotationReason={policyRotateReason}
              onRotationReasonChange={setPolicyRotateReason}
              onRotate={handleRotatePolicyKey}
              isRotating={rotatePolicyKey.isPending}
              trustMode={policyTrustMode}
              trustProvider={policyTrustProvider}
              externalKeyRef={policyExternalKeyRef}
              trustAttestationRef={policyTrustAttestationRef}
              onTrustModeChange={setPolicyTrustMode}
              onTrustProviderChange={setPolicyTrustProvider}
              onExternalKeyRefChange={setPolicyExternalKeyRef}
              onTrustAttestationRefChange={setPolicyTrustAttestationRef}
            />
          </CardContent>
        </Card>
      </div>

      {(contractsError || policiesError) && (
        <Alert variant="destructive">
          <ScrollText className="h-4 w-4" />
          <AlertTitle>Governance audit data could not be loaded</AlertTitle>
          <AlertDescription>
            {contractsError && <p>Contracts: {getErrorMessage(contractsError)}</p>}
            {policiesError && <p>Policies: {getErrorMessage(policiesError)}</p>}
          </AlertDescription>
        </Alert>
      )}

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.15fr),minmax(0,0.85fr)]">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ShieldCheck className="h-4 w-4" />
              Governance Overview
            </CardTitle>
            <CardDescription>
              A thin operational summary across contract and policy rollout state. Use it to spot
              pending approvals, unsigned approved contracts, or active policies that are not yet
              fully attested.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            {isLoadingContracts || isLoadingPolicies ? (
              <LoadingState label="Loading aggregate SoS governance state..." />
            ) : (
              <>
                <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
                  <MetricCard
                    label="Pending Queue"
                    value={String(governanceOverview.summary.pendingApprovals)}
                    detail="Contracts or policies waiting on reviewer action"
                  />
                  <MetricCard
                    label="Protected Revisions"
                    value={String(governanceOverview.summary.protectedAssets)}
                    detail="Signed contracts plus attested policies"
                  />
                  <MetricCard
                    label="Needs Attention"
                    value={String(governanceOverview.summary.needsAttention)}
                    detail="Approved-but-unprotected or rollout-active-but-unattested"
                  />
                  <MetricCard
                    label="Keys Due"
                    value={String(governanceOverview.summary.keysDue)}
                    detail="Signing keys whose next rotation date has passed"
                  />
                </div>

                <GovernanceHealthTable rows={governanceOverview.healthRows} />
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <History className="h-4 w-4" />
              Recent Trust Feed
            </CardTitle>
            <CardDescription>
              Latest contract signatures and policy attestations, sorted into one operator-facing
              feed without leaving the SoS workspace.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {isLoadingContracts || isLoadingPolicies ? (
              <LoadingState label="Loading recent trust activity..." />
            ) : governanceOverview.recentTrustEvents.length === 0 ? (
              <EmptyAuditState
                title="No signature or attestation activity yet"
                description="Once contracts are signed or policies are attested, the latest events appear here."
              />
            ) : (
              <RecentTrustFeed events={governanceOverview.recentTrustEvents} />
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <ScrollText className="h-4 w-4" />
            Pending Approval Queue
          </CardTitle>
          <CardDescription>
            Consolidated queue across contracts and policies. This view stays thin and derived from
            the catalog summaries; use the detailed audit panels below when you need request-level
            evidence and revision history.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {isLoadingContracts || isLoadingPolicies ? (
            <LoadingState label="Loading pending approval queue..." />
          ) : governanceOverview.pendingApprovals.length === 0 ? (
            <EmptyAuditState
              title="No pending approvals"
              description="Current SoS governance records do not show any contracts or policies waiting on review."
            />
          ) : (
            <PendingApprovalQueueTable items={governanceOverview.pendingApprovals} />
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 xl:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <History className="h-4 w-4" />
              Contract Governance Audit
            </CardTitle>
            <CardDescription>
              Review approval requests, evidence, and immutable signatures for a concrete contract
              revision.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <AuditSelector
              id="sos-operations-contract-id"
              label="Contract"
              value={selectedContractId}
              onChange={setSelectedContractId}
              placeholder="contract-id"
              listId="sos-operations-contract-options"
            />
            <datalist id="sos-operations-contract-options">
              {contracts.map((contract) => (
                <option
                  key={contract.contract_id}
                  value={contract.contract_id}
                  label={`${contract.contract_name} (${contract.provider_interface_id} -> ${contract.consumer_interface_id})`}
                />
              ))}
            </datalist>

            {isLoadingContracts ? (
              <LoadingState label="Loading SoS contracts..." />
            ) : selectedContract ? (
              <>
                <ContractSummary contract={selectedContract} />
                <GovernanceSection
                  title="Approval Requests"
                  count={contractApprovals.data?.total ?? 0}
                  loading={contractApprovals.isLoading}
                  emptyTitle="No approval requests yet"
                  emptyDescription="Once contract approval workflows begin, requests and evidence land here."
                >
                  <ApprovalRequestsTable requests={contractApprovals.data?.requests ?? []} />
                </GovernanceSection>
                <GovernanceSection
                  title="Signatures"
                  count={contractSignatures.data?.total ?? 0}
                  loading={contractSignatures.isLoading}
                  emptyTitle="No signatures yet"
                  emptyDescription="Signed contract revisions expose their immutable attestation material here."
                >
                  <ContractSignaturesTable signatures={contractSignatures.data?.signatures ?? []} />
                </GovernanceSection>
              </>
            ) : (
              <EmptyAuditState
                title="No contract selected"
                description="Choose a contract id to inspect its approval workflow and signature history."
              />
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <KeyRound className="h-4 w-4" />
              Policy Governance Audit
            </CardTitle>
            <CardDescription>
              Review rollout state, approval requests, evidence, and approval attestations for one
              persisted policy revision stream.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <AuditSelector
              id="sos-operations-policy-id"
              label="Policy"
              value={selectedPolicyId}
              onChange={setSelectedPolicyId}
              placeholder="policy-id"
              listId="sos-operations-policy-options"
            />
            <datalist id="sos-operations-policy-options">
              {policies.map((policy) => (
                <option
                  key={policy.policy_id}
                  value={policy.policy_id}
                  label={`${policy.policy_name} (${policy.target_type})`}
                />
              ))}
            </datalist>

            {isLoadingPolicies ? (
              <LoadingState label="Loading SoS policies..." />
            ) : selectedPolicy ? (
              <>
                <PolicySummary policy={selectedPolicy} />
                <GovernanceSection
                  title="Approval Requests"
                  count={policyApprovals.data?.total ?? 0}
                  loading={policyApprovals.isLoading}
                  emptyTitle="No approval requests yet"
                  emptyDescription="Policy rollout requests and their attached evidence show up here."
                >
                  <ApprovalRequestsTable requests={policyApprovals.data?.requests ?? []} />
                </GovernanceSection>
                <GovernanceSection
                  title="Attestations"
                  count={policyAttestations.data?.total ?? 0}
                  loading={policyAttestations.isLoading}
                  emptyTitle="No attestations yet"
                  emptyDescription="Approved policy revisions publish their cryptographic attestation history here."
                >
                  <PolicyAttestationsTable
                    attestations={policyAttestations.data?.attestations ?? []}
                  />
                </GovernanceSection>
              </>
            ) : (
              <EmptyAuditState
                title="No policy selected"
                description="Choose a policy id to inspect rollout approvals and attestation history."
              />
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

type PendingApprovalQueueItem = {
  assetType: 'Contract' | 'Policy';
  assetId: string;
  assetName: string;
  revision: number;
  requestedBy: string;
  requestedAt: string;
  approvalStatus: string;
  lifecycleState: string;
  attentionReason: string;
};

type TrustFeedEvent = {
  id: string;
  assetType: 'Contract' | 'Policy';
  assetId: string;
  assetName: string;
  revisionRef: string;
  actor: string;
  occurredAt: string;
  verificationLabel: string;
  verificationVariant: 'success' | 'warning';
  trustDetail: string;
  sourceLabel: string;
};

type GovernanceHealthRow = {
  label: 'Contracts' | 'Policies';
  total: number;
  pending: number;
  approved: number;
  protectedCount: number;
  attention: number;
};

function MetricCard(props: {
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="rounded-sm border border-border bg-background p-3">
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {props.label}
      </div>
      <div className="mt-2 text-2xl font-semibold text-foreground">{props.value}</div>
      <div className="mt-1 text-xs text-muted-foreground">{props.detail}</div>
    </div>
  );
}

function GovernanceHealthTable({ rows }: { rows: GovernanceHealthRow[] }) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Surface</TableHead>
          <TableHead>Total</TableHead>
          <TableHead>Pending</TableHead>
          <TableHead>Approved</TableHead>
          <TableHead>Protected</TableHead>
          <TableHead>Attention</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((row) => (
          <TableRow key={row.label}>
            <TableCell className="font-medium text-foreground">{row.label}</TableCell>
            <TableCell>{row.total}</TableCell>
            <TableCell>{row.pending}</TableCell>
            <TableCell>{row.approved}</TableCell>
            <TableCell>{row.protectedCount}</TableCell>
            <TableCell>
              <Badge variant={row.attention > 0 ? 'warning' : 'success'}>
                {row.attention}
              </Badge>
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function PendingApprovalQueueTable({ items }: { items: PendingApprovalQueueItem[] }) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Asset</TableHead>
          <TableHead>Requested</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Lifecycle</TableHead>
          <TableHead>Attention</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {items.map((item) => (
          <TableRow key={`${item.assetType}:${item.assetId}:${item.revision}`}>
            <TableCell className="align-top">
              <div className="space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <div className="font-medium text-foreground">{item.assetName}</div>
                  <Badge variant="outline">{item.assetType}</Badge>
                </div>
                <div className="text-xs text-muted-foreground">
                  {item.assetId} @ rev {item.revision}
                </div>
              </div>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              <div>{item.requestedBy}</div>
              <div>{formatTimestamp(item.requestedAt)}</div>
            </TableCell>
            <TableCell className="align-top">
              <Badge variant={statusBadgeVariant(item.approvalStatus)}>{item.approvalStatus}</Badge>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {item.lifecycleState}
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {item.attentionReason}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function RecentTrustFeed({ events }: { events: TrustFeedEvent[] }) {
  return (
    <div className="space-y-3">
      {events.map((event) => (
        <div
          key={event.id}
          className="rounded-sm border border-border bg-background-secondary p-3"
        >
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <div className="font-medium text-foreground">{event.assetName}</div>
                <Badge variant="outline">{event.assetType}</Badge>
                <Badge variant={event.verificationVariant}>{event.verificationLabel}</Badge>
              </div>
              <div className="text-sm text-muted-foreground">{event.revisionRef}</div>
              <div className="text-xs text-muted-foreground">
                {event.assetId} · {event.trustDetail}
              </div>
            </div>
            <div className="text-right text-xs text-muted-foreground">
              <div>{event.actor}</div>
              <div>{formatTimestamp(event.occurredAt)}</div>
              <div>{event.sourceLabel}</div>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function KeyStatusCard(props: {
  title: string;
  status: {
    signing_key_ref?: string | null;
    signing_key_source: string;
    signing_key_version?: string | null;
    key_fingerprint: string;
    supports_rotation: boolean;
    owner?: string | null;
    updated_at?: string | null;
    rotation_next_due_at?: string | null;
    trust_mode?: string;
    trust_provider?: string | null;
    external_key_ref?: string | null;
    trust_attestation_ref?: string | null;
  } | null | undefined;
  isLoading: boolean;
  canRotate: boolean;
  rotationReason: string;
  onRotationReasonChange: (value: string) => void;
  onRotate: () => void;
  isRotating: boolean;
  trustMode?: string;
  trustProvider?: string;
  externalKeyRef?: string;
  trustAttestationRef?: string;
  onTrustModeChange?: (value: string) => void;
  onTrustProviderChange?: (value: string) => void;
  onExternalKeyRefChange?: (value: string) => void;
  onTrustAttestationRefChange?: (value: string) => void;
}) {
  return (
    <div className="rounded-sm border border-border bg-background p-3">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div className="font-medium text-foreground">{props.title}</div>
        <Badge variant={props.status?.supports_rotation ? 'success' : 'secondary'}>
          {props.status?.supports_rotation ? 'Rotation supported' : 'Rotation fixed'}
        </Badge>
      </div>

      {props.isLoading ? (
        <LoadingState label={`Loading ${props.title.toLowerCase()}...`} compact />
      ) : props.status ? (
        <div className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <InfoBlock label="Key Ref" value={props.status.signing_key_ref ?? 'inline/env-managed'} />
            <InfoBlock label="Source" value={props.status.signing_key_source} />
            <InfoBlock
              label="Version"
              value={props.status.signing_key_version ?? 'unversioned'}
            />
            <InfoBlock
              label="Fingerprint"
              value={shortenFingerprint(props.status.key_fingerprint)}
            />
            <InfoBlock label="Owner" value={props.status.owner ?? 'unassigned'} />
            <InfoBlock
              label="Updated"
              value={props.status.updated_at ? formatTimestamp(props.status.updated_at) : 'unknown'}
            />
            {props.status.trust_mode && <InfoBlock label="Trust Mode" value={props.status.trust_mode} />}
            {props.status.trust_provider && (
              <InfoBlock label="Trust Provider" value={props.status.trust_provider} />
            )}
            {props.status.rotation_next_due_at && (
              <InfoBlock
                label="Next Rotation"
                value={formatTimestamp(props.status.rotation_next_due_at)}
              />
            )}
          </div>

          <div className="space-y-2">
            <Label htmlFor={`${props.title}-reason`}>Rotation Note</Label>
            <Input
              id={`${props.title}-reason`}
              value={props.rotationReason}
              onChange={(event) => props.onRotationReasonChange(event.target.value)}
              placeholder="Optional operator note"
            />
          </div>

          {props.onTrustModeChange && (
            <div className="grid gap-2 md:grid-cols-2">
              <Input
                value={props.trustMode ?? ''}
                onChange={(event) => props.onTrustModeChange?.(event.target.value)}
                placeholder="Trust mode (for example software or external_reference)"
              />
              <Input
                value={props.trustProvider ?? ''}
                onChange={(event) => props.onTrustProviderChange?.(event.target.value)}
                placeholder="Trust provider"
              />
              <Input
                value={props.externalKeyRef ?? ''}
                onChange={(event) => props.onExternalKeyRefChange?.(event.target.value)}
                placeholder="External key ref"
              />
              <Input
                value={props.trustAttestationRef ?? ''}
                onChange={(event) => props.onTrustAttestationRefChange?.(event.target.value)}
                placeholder="External trust attestation ref"
              />
            </div>
          )}

          <Button
            variant="outline"
            onClick={props.onRotate}
            disabled={!props.canRotate || props.isRotating || !props.status.supports_rotation}
          >
            {props.isRotating ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="mr-2 h-4 w-4" />
            )}
            Rotate Key
          </Button>
        </div>
      ) : (
        <EmptyAuditState
          title="No signing key status available"
          description="The coordinator has not exposed signing-key metadata yet."
          compact
        />
      )}
    </div>
  );
}

function ContractSummary({ contract }: { contract: SosDataContract }) {
  return (
    <div className="rounded-sm border border-border bg-background-secondary p-3">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="font-medium text-foreground">{contract.contract_name}</div>
        <Badge variant={contract.approved ? 'success' : 'warning'}>
          {contract.approved ? 'Approved' : 'Pending approval'}
        </Badge>
        <Badge variant={contract.signed ? 'success' : 'secondary'}>
          {contract.signed ? 'Signed' : 'Unsigned'}
        </Badge>
        <Badge variant="outline">{contract.lifecycle_state ?? 'lifecycle-unknown'}</Badge>
        <Badge variant="outline">{contract.approval_status ?? 'approval-unknown'}</Badge>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        <InfoBlock label="Contract Id" value={contract.contract_id} />
        <InfoBlock label="Revision" value={String(contract.revision ?? 1)} />
        <InfoBlock label="Provider" value={contract.provider_interface_id} />
        <InfoBlock label="Consumer" value={contract.consumer_interface_id} />
        <InfoBlock label="Approved By" value={contract.approved_by ?? 'not yet approved'} />
        <InfoBlock label="Signed By" value={contract.signed_by ?? 'not yet signed'} />
      </div>
    </div>
  );
}

function PolicySummary({ policy }: { policy: SosPolicyRecord }) {
  return (
    <div className="rounded-sm border border-border bg-background-secondary p-3">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="font-medium text-foreground">{policy.policy_name}</div>
        <Badge variant={policy.active ? 'success' : 'secondary'}>
          {policy.active ? 'Participating' : 'Inactive'}
        </Badge>
        <Badge variant="outline">{policy.lifecycle_state ?? 'lifecycle-unknown'}</Badge>
        <Badge variant="outline">{policy.approval_status ?? 'approval-unknown'}</Badge>
        <Badge variant="outline">{policy.target_type}</Badge>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        <InfoBlock label="Policy Id" value={policy.policy_id} />
        <InfoBlock label="Revision" value={String(policy.revision ?? 1)} />
        <InfoBlock label="Stages" value={policy.stages.join(', ')} />
        <InfoBlock label="Approved By" value={policy.approved_by ?? 'not yet approved'} />
        <InfoBlock
          label="Requested By"
          value={policy.approval_requested_by ?? 'no approval request yet'}
        />
        <InfoBlock
          label="Attestation"
          value={policy.attestation?.attestation_verified ? 'verified' : 'not yet attested'}
        />
      </div>
    </div>
  );
}

function GovernanceSection(props: {
  title: string;
  count: number;
  loading: boolean;
  emptyTitle: string;
  emptyDescription: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-3 rounded-sm border border-border bg-background p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="font-medium text-foreground">{props.title}</div>
        <Badge variant="secondary">{props.count}</Badge>
      </div>
      {props.loading ? (
        <LoadingState label={`Loading ${props.title.toLowerCase()}...`} compact />
      ) : props.count === 0 ? (
        <EmptyAuditState
          title={props.emptyTitle}
          description={props.emptyDescription}
          compact
        />
      ) : (
        props.children
      )}
    </div>
  );
}

function ApprovalRequestsTable(props: {
  requests: Array<SosContractApprovalRequest | SosPolicyApprovalRequest>;
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Request</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Requested</TableHead>
          <TableHead>Target State</TableHead>
          <TableHead>Evidence</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {props.requests.map((request) => (
          <TableRow key={request.request_id}>
            <TableCell className="align-top">
              <div className="space-y-1">
                <div className="font-medium text-foreground">{request.request_id}</div>
                <div className="text-xs text-muted-foreground">
                  {request.note ?? 'No request note'}
                </div>
              </div>
            </TableCell>
            <TableCell className="align-top">
              <Badge variant={statusBadgeVariant(request.status)}>{request.status}</Badge>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              <div>{request.requested_by}</div>
              <div>{formatTimestamp(request.requested_at)}</div>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {request.requested_lifecycle_state}
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {request.evidence.length}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function ContractSignaturesTable({ signatures }: { signatures: SosContractSignature[] }) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Revision</TableHead>
          <TableHead>Signer</TableHead>
          <TableHead>Verification</TableHead>
          <TableHead>Key</TableHead>
          <TableHead>Evidence</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {signatures.map((signature) => (
          <TableRow key={signature.signature_id}>
            <TableCell className="align-top">
              <div className="font-medium text-foreground">{signature.contract_revision_ref}</div>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              <div>{signature.signed_by}</div>
              <div>{formatTimestamp(signature.signed_at)}</div>
            </TableCell>
            <TableCell className="align-top">
              <Badge variant={signature.signature_verified ? 'success' : 'warning'}>
                {signature.signature_verified ? 'Verified' : 'Unverified'}
              </Badge>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {shortenFingerprint(signature.key_fingerprint)}
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {signature.evidence_ids.length}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function PolicyAttestationsTable({ attestations }: { attestations: SosPolicyAttestation[] }) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Revision</TableHead>
          <TableHead>Trust</TableHead>
          <TableHead>Attested By</TableHead>
          <TableHead>Verification</TableHead>
          <TableHead>Evidence</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {attestations.map((attestation) => (
          <TableRow key={attestation.attestation_id}>
            <TableCell className="align-top">
              <div className="font-medium text-foreground">{attestation.policy_revision_ref}</div>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              <div>{attestation.trust_mode}</div>
              <div>{attestation.trust_provider ?? attestation.signing_key_source}</div>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              <div>{attestation.attested_by}</div>
              <div>{formatTimestamp(attestation.attested_at)}</div>
            </TableCell>
            <TableCell className="align-top">
              <Badge variant={attestation.attestation_verified ? 'success' : 'warning'}>
                {attestation.attestation_verified ? 'Verified' : 'Unverified'}
              </Badge>
            </TableCell>
            <TableCell className="align-top text-sm text-muted-foreground">
              {attestation.evidence_ids.length}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function AuditSelector(props: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  listId: string;
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor={props.id}>{props.label}</Label>
      <Input
        id={props.id}
        list={props.listId}
        value={props.value}
        onChange={(event) => props.onChange(event.target.value)}
        placeholder={props.placeholder}
        autoComplete="off"
      />
    </div>
  );
}

function InfoBlock({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-sm border border-border bg-background p-2">
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {label}
      </div>
      <div className="mt-1 break-words text-sm text-foreground">{value}</div>
    </div>
  );
}

function LoadingState({ label, compact = false }: { label: string; compact?: boolean }) {
  return (
    <div
      className={`flex items-center gap-2 text-sm text-muted-foreground ${
        compact ? 'py-1' : 'rounded-sm border border-dashed border-border px-3 py-6'
      }`}
    >
      <Loader2 className="h-4 w-4 animate-spin" />
      <span>{label}</span>
    </div>
  );
}

function EmptyAuditState(props: {
  title: string;
  description: string;
  compact?: boolean;
}) {
  return (
    <div
      className={`rounded-sm border border-dashed border-border bg-background p-3 ${
        props.compact ? '' : 'text-center'
      }`}
    >
      <div className="font-medium text-foreground">{props.title}</div>
      <div className="mt-1 text-sm text-muted-foreground">{props.description}</div>
    </div>
  );
}

function formatTimestamp(value: string): string {
  return new Date(value).toLocaleString();
}

function shortenFingerprint(value: string): string {
  if (value.length <= 24) {
    return value;
  }

  return `${value.slice(0, 12)}...${value.slice(-8)}`;
}

function statusBadgeVariant(status: string): 'success' | 'warning' | 'secondary' | 'destructive' {
  switch (status.toLowerCase()) {
    case 'approved':
    case 'completed':
      return 'success';
    case 'rejected':
      return 'destructive';
    case 'pending':
    case 'requested':
      return 'warning';
    default:
      return 'secondary';
  }
}

function getErrorMessage(error: unknown): string {
  const apiError = error as {
    message?: string;
    response?: {
      data?: {
        message?: string;
        error?: string;
      };
    };
  };

  return (
    apiError.response?.data?.message ||
    apiError.response?.data?.error ||
    apiError.message ||
    'Request failed'
  );
}

function buildGovernanceOverview(input: {
  contracts: SosDataContract[];
  policies: SosPolicyRecord[];
  contractKeyStatus:
    | {
        rotation_next_due_at?: string | null;
      }
    | null
    | undefined;
  policyKeyStatus:
    | {
        rotation_next_due_at?: string | null;
      }
    | null
    | undefined;
}) {
  const pendingApprovals: PendingApprovalQueueItem[] = [
    ...input.contracts
      .filter((contract) => isPendingApproval(contract.approval_status))
      .map((contract) => ({
        assetType: 'Contract' as const,
        assetId: contract.contract_id,
        assetName: contract.contract_name,
        revision: contract.revision ?? 1,
        requestedBy: contract.approval_requested_by ?? 'unknown operator',
        requestedAt: contract.approval_requested_at ?? contract.updated_at,
        approvalStatus: contract.approval_status ?? 'pending',
        lifecycleState: contract.lifecycle_state ?? 'lifecycle-unknown',
        attentionReason: contract.signed
          ? 'Pending review despite an existing signature state'
          : 'Approval must clear before signing can proceed',
      })),
    ...input.policies
      .filter((policy) => isPendingApproval(policy.approval_status))
      .map((policy) => ({
        assetType: 'Policy' as const,
        assetId: policy.policy_id,
        assetName: policy.policy_name,
        revision: policy.revision ?? 1,
        requestedBy: policy.approval_requested_by ?? 'unknown operator',
        requestedAt: policy.approval_requested_at ?? policy.updated_at,
        approvalStatus: policy.approval_status ?? 'pending',
        lifecycleState: policy.lifecycle_state ?? 'lifecycle-unknown',
        attentionReason: policy.active
          ? 'Participating policy still needs rollout approval'
          : 'Draft rollout is waiting on reviewer action',
      })),
  ].sort(
    (left, right) =>
      new Date(right.requestedAt).getTime() - new Date(left.requestedAt).getTime()
  );

  const recentTrustEvents: TrustFeedEvent[] = [
    ...input.contracts
      .filter((contract) => Boolean(contract.signature))
      .map((contract) => {
        const signature = contract.signature as SosContractSignature;

        return {
          id: `contract-signature:${signature.signature_id}`,
          assetType: 'Contract' as const,
          assetId: contract.contract_id,
          assetName: contract.contract_name,
          revisionRef: signature.contract_revision_ref,
          actor: signature.signed_by,
          occurredAt: signature.signed_at,
          verificationLabel: signature.signature_verified ? 'Verified' : 'Unverified',
          verificationVariant: signature.signature_verified ? ('success' as const) : ('warning' as const),
          trustDetail: `${signature.signature_algorithm} via ${signature.signing_key_source}`,
          sourceLabel: shortenFingerprint(signature.key_fingerprint),
        };
      }),
    ...input.policies
      .filter((policy) => Boolean(policy.attestation))
      .map((policy) => {
        const attestation = policy.attestation as SosPolicyAttestation;

        return {
          id: `policy-attestation:${attestation.attestation_id}`,
          assetType: 'Policy' as const,
          assetId: policy.policy_id,
          assetName: policy.policy_name,
          revisionRef: attestation.policy_revision_ref,
          actor: attestation.attested_by,
          occurredAt: attestation.attested_at,
          verificationLabel: attestation.attestation_verified ? 'Verified' : 'Unverified',
          verificationVariant: attestation.attestation_verified
            ? ('success' as const)
            : ('warning' as const),
          trustDetail: `${attestation.trust_mode} via ${attestation.trust_provider ?? attestation.signing_key_source}`,
          sourceLabel: shortenFingerprint(attestation.key_fingerprint),
        };
      }),
  ]
    .sort((left, right) => new Date(right.occurredAt).getTime() - new Date(left.occurredAt).getTime())
    .slice(0, 8);

  const contractHealth = {
    label: 'Contracts' as const,
    total: input.contracts.length,
    pending: input.contracts.filter((contract) => isPendingApproval(contract.approval_status)).length,
    approved: input.contracts.filter((contract) => isApproved(contract.approval_status) || contract.approved)
      .length,
    protectedCount: input.contracts.filter((contract) => contract.signed || contract.signature?.signature_verified)
      .length,
    attention: input.contracts.filter(
      (contract) =>
        (isApproved(contract.approval_status) || contract.approved) &&
        !(contract.signed || contract.signature?.signature_verified)
    ).length,
  };

  const policyHealth = {
    label: 'Policies' as const,
    total: input.policies.length,
    pending: input.policies.filter((policy) => isPendingApproval(policy.approval_status)).length,
    approved: input.policies.filter((policy) => isApproved(policy.approval_status)).length,
    protectedCount: input.policies.filter((policy) => policy.attestation?.attestation_verified).length,
    attention: input.policies.filter(
      (policy) => isRolloutActive(policy) && !policy.attestation?.attestation_verified
    ).length,
  };

  return {
    pendingApprovals,
    recentTrustEvents,
    healthRows: [contractHealth, policyHealth] satisfies GovernanceHealthRow[],
    summary: {
      pendingApprovals: pendingApprovals.length,
      protectedAssets: contractHealth.protectedCount + policyHealth.protectedCount,
      needsAttention: contractHealth.attention + policyHealth.attention,
      keysDue:
        countPastDue(input.contractKeyStatus?.rotation_next_due_at) +
        countPastDue(input.policyKeyStatus?.rotation_next_due_at),
    },
  };
}

function isPendingApproval(status?: string | null): boolean {
  if (!status) {
    return false;
  }

  return ['pending', 'requested', 'in_review'].includes(status.toLowerCase());
}

function isApproved(status?: string | null): boolean {
  if (!status) {
    return false;
  }

  return ['approved', 'completed'].includes(status.toLowerCase());
}

function isRolloutActive(policy: SosPolicyRecord): boolean {
  const lifecycleState = policy.lifecycle_state?.toLowerCase();
  return ['active', 'dry_run', 'deprecated'].includes(lifecycleState ?? '');
}

function countPastDue(value?: string | null): number {
  if (!value) {
    return 0;
  }

  return new Date(value).getTime() <= Date.now() ? 1 : 0;
}
