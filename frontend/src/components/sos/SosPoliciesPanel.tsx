import React, { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Loader2,
  PlayCircle,
  Plus,
  Save,
  ScrollText,
  ShieldCheck,
  Trash2,
} from 'lucide-react';

import type {
  SosDataContract,
  SosInterfaceRecord,
  SosPolicyRecord,
  SosSystemRecord,
  SosValidationResponse,
} from '@/api/sosValidation';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Textarea } from '@/components/ui/textarea';
import { cn } from '@/lib/utils';
import {
  useCreateSosPolicy,
  useDeleteSosPolicy,
  useSosContracts,
  useSosInterfaces,
  useSosPolicies,
  useSosSystems,
  useUpdateSosPolicy,
  useValidateSosPolicy,
  useValidateSosPolicyDryRun,
} from '@/hooks/useSosValidation';

const POLICY_TARGET_OPTIONS = [
  { value: 'global', label: 'Global' },
  { value: 'interface_pair', label: 'Interface Pair' },
  { value: 'contract', label: 'Contract' },
  { value: 'system_pair', label: 'System Pair' },
  { value: 'interface', label: 'Interface' },
] as const;
const POLICY_STAGE_OPTIONS = [
  { value: 'pre_execution', label: 'Pre-Execution' },
  { value: 'in_flight', label: 'In-Flight' },
  { value: 'post_execution', label: 'Post-Execution' },
] as const;
const ENFORCEMENT_OPTIONS = ['mandatory', 'advisory'] as const;
const SEVERITY_OPTIONS = ['critical', 'high', 'medium', 'low', 'error', 'warning', 'info'] as const;

interface PolicyFormState {
  policyId: string;
  policyName: string;
  description: string;
  targetType: string;
  stages: string[];
  enforcementLevel: string;
  severity: string;
  sparqlQuery: string;
  contextText: string;
  tagsText: string;
  ontologyRefsText: string;
  shapeRefsText: string;
  active: boolean;
  providerInterfaceId: string;
  consumerInterfaceId: string;
  contractId: string;
  sourceSystemId: string;
  targetSystemId: string;
  interfaceId: string;
}

interface EvaluationFormState {
  stage: string;
  contextText: string;
}

interface ReportsTarget {
  reportId?: string | null;
  subjectType?: string;
  subjectKey?: string;
}

interface SosPoliciesPanelProps {
  currentPair?: {
    providerInterfaceId: string;
    consumerInterfaceId: string;
  } | null;
  onOpenReports?: (target?: ReportsTarget) => void;
}

export function SosPoliciesPanel({ currentPair, onOpenReports }: SosPoliciesPanelProps) {
  const [selectedPolicyId, setSelectedPolicyId] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [evaluationError, setEvaluationError] = useState<string | null>(null);
  const [formState, setFormState] = useState<PolicyFormState>(emptyPolicyFormState(currentPair));
  const [evaluationState, setEvaluationState] = useState<EvaluationFormState>({
    stage: 'default',
    contextText: '{}',
  });
  const [evaluationResult, setEvaluationResult] = useState<SosValidationResponse | null>(null);

  const {
    data: policiesResponse,
    isLoading: isLoadingPolicies,
    error: policiesError,
  } = useSosPolicies();
  const { data: interfacesData, error: interfacesError } = useSosInterfaces();
  const { data: systemsResponse, error: systemsError } = useSosSystems();
  const { data: contractsData, error: contractsError } = useSosContracts();

  const createPolicy = useCreateSosPolicy();
  const updatePolicy = useUpdateSosPolicy();
  const deletePolicy = useDeleteSosPolicy();
  const validatePolicy = useValidateSosPolicy();
  const validatePolicyDryRun = useValidateSosPolicyDryRun();

  const sortedPolicies = useMemo(
    () =>
      [...(policiesResponse?.policies ?? [])].sort((left, right) =>
        right.updated_at.localeCompare(left.updated_at)
      ),
    [policiesResponse]
  );
  const sortedInterfaces = useMemo(
    () =>
      [...(interfacesData ?? [])].sort((left, right) =>
        left.interface_id.localeCompare(right.interface_id)
      ),
    [interfacesData]
  );
  const sortedSystems = useMemo(
    () =>
      [...(systemsResponse?.systems ?? [])].sort((left, right) =>
        left.system_id.localeCompare(right.system_id)
      ),
    [systemsResponse]
  );
  const sortedContracts = useMemo(
    () =>
      [...(contractsData ?? [])].sort((left, right) =>
        left.contract_id.localeCompare(right.contract_id)
      ),
    [contractsData]
  );

  const selectedPolicy = useMemo(
    () =>
      (policiesResponse?.policies ?? []).find((policy) => policy.policy_id === selectedPolicyId) ??
      null,
    [policiesResponse, selectedPolicyId]
  );
  const policyCount = policiesResponse?.total ?? sortedPolicies.length;

  useEffect(() => {
    if (selectedPolicy) {
      setFormState(policyToFormState(selectedPolicy));
      setEvaluationState({
        stage: 'default',
        contextText: prettyJson(selectedPolicy.context),
      });
      setEvaluationResult(null);
      setFormError(null);
      setEvaluationError(null);
      return;
    }

    setFormState(emptyPolicyFormState(currentPair));
  }, [currentPair, selectedPolicy]);

  const pending =
    createPolicy.isPending ||
    updatePolicy.isPending ||
    deletePolicy.isPending ||
    validatePolicy.isPending ||
    validatePolicyDryRun.isPending;

  const handleCreateNew = () => {
    setSelectedPolicyId(null);
    setFormState(emptyPolicyFormState(currentPair));
    setEvaluationState({ stage: 'default', contextText: '{}' });
    setEvaluationResult(null);
    setFormError(null);
    setEvaluationError(null);
  };

  const handleToggleStage = (stage: string, checked: boolean) => {
    setFormState((current) => ({
      ...current,
      stages: checked
        ? dedupeStrings([...current.stages, stage])
        : current.stages.filter((value) => value !== stage),
    }));
  };

  const handleUseCurrentPair = () => {
    if (!currentPair) {
      return;
    }

    setFormState((current) => ({
      ...current,
      targetType: 'interface_pair',
      providerInterfaceId: currentPair.providerInterfaceId,
      consumerInterfaceId: currentPair.consumerInterfaceId,
    }));
  };

  const handleSave = async () => {
    setFormError(null);

    if (!formState.policyId.trim()) {
      setFormError('Policy id is required.');
      return;
    }

    if (!formState.policyName.trim()) {
      setFormError('Policy name is required.');
      return;
    }

    if (!formState.sparqlQuery.trim()) {
      setFormError('SPARQL query is required.');
      return;
    }

    if (formState.stages.length === 0) {
      setFormError('Choose at least one policy stage.');
      return;
    }

    if (selectedPolicy && formState.targetType !== selectedPolicy.target_type) {
      setFormError(
        'Changing policy target type in place is disabled to keep governance records canonical. Create a new policy instead.'
      );
      return;
    }

    const context = parseObjectJson(formState.contextText, 'policy context');
    if (!context.ok) {
      setFormError(context.error);
      return;
    }

    const targetFields = buildTargetFields(formState);
    if (!targetFields.ok) {
      setFormError(targetFields.error);
      return;
    }

    const sharedRequest = {
      policy_name: formState.policyName.trim(),
      target_type: formState.targetType,
      stages: dedupeStrings(formState.stages),
      enforcement_level: formState.enforcementLevel,
      severity: formState.severity,
      sparql_query: formState.sparqlQuery.trim(),
      context: context.value,
      description: emptyToNull(formState.description),
      tags: parseCsvList(formState.tagsText),
      ontology_refs: parseCsvList(formState.ontologyRefsText),
      shape_refs: parseCsvList(formState.shapeRefsText),
      active: formState.active,
      ...targetFields.value,
    };

    try {
      const saved = selectedPolicy
        ? await updatePolicy.mutateAsync({
            id: selectedPolicy.policy_id,
            request: sharedRequest,
          })
        : await createPolicy.mutateAsync({
            policy_id: formState.policyId.trim(),
            ...sharedRequest,
          });

      setSelectedPolicyId(saved.policy_id);
      setEvaluationResult(null);
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const handleDelete = async () => {
    if (!selectedPolicy) {
      return;
    }

    setFormError(null);

    try {
      await deletePolicy.mutateAsync(selectedPolicy.policy_id);
      handleCreateNew();
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const runEvaluation = async (persistReport: boolean) => {
    if (!selectedPolicy) {
      setEvaluationError('Save or select a policy before running validation.');
      return;
    }

    setEvaluationError(null);

    const context = parseObjectJson(evaluationState.contextText, 'evaluation context');
    if (!context.ok) {
      setEvaluationError(context.error);
      return;
    }

    try {
      const result = persistReport
        ? await validatePolicy.mutateAsync({
            id: selectedPolicy.policy_id,
            request: {
              stage: evaluationState.stage === 'default' ? undefined : evaluationState.stage,
              context: context.value,
            },
          })
        : await validatePolicyDryRun.mutateAsync({
            id: selectedPolicy.policy_id,
            request: {
              stage: evaluationState.stage === 'default' ? undefined : evaluationState.stage,
              context: context.value,
            },
          });

      setEvaluationResult(result);
    } catch (error) {
      setEvaluationError(getErrorMessage(error));
    }
  };

  return (
    <div className="space-y-4">
      {(policiesError || interfacesError || systemsError || contractsError) && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>Policy governance data could not be loaded cleanly</AlertTitle>
          <AlertDescription>
            {policiesError && <p>Policies: {getErrorMessage(policiesError)}</p>}
            {interfacesError && <p>Interfaces: {getErrorMessage(interfacesError)}</p>}
            {systemsError && <p>Systems: {getErrorMessage(systemsError)}</p>}
            {contractsError && <p>Contracts: {getErrorMessage(contractsError)}</p>}
          </AlertDescription>
        </Alert>
      )}

      <div className="grid gap-4 xl:grid-cols-[320px,minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <div className="flex items-start justify-between gap-3">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <ScrollText className="h-4 w-4" />
                  Persisted Policies
                </CardTitle>
                <CardDescription>
                  Store SoS policy definitions separately from pair-by-pair validation runs.
                </CardDescription>
              </div>
              <Button variant="outline" size="sm" onClick={handleCreateNew} disabled={pending}>
                <Plus className="mr-2 h-4 w-4" />
                New
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
              <Badge variant="secondary">{policyCount} policies</Badge>
              <Badge variant="outline">governance</Badge>
            </div>

            {isLoadingPolicies ? (
              <div className="flex items-center gap-2 rounded-sm border border-border p-3 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Loading persisted policies...
              </div>
            ) : sortedPolicies.length === 0 ? (
              <div className="rounded-sm border border-dashed border-border p-4 text-sm text-muted-foreground">
                No SoS policies exist yet. Start with a global baseline or an interface-pair rule.
              </div>
            ) : (
              <div className="space-y-2">
                {sortedPolicies.map((policy) => {
                  const isSelected = policy.policy_id === selectedPolicyId;
                  return (
                    <button
                      type="button"
                      key={policy.policy_id}
                      onClick={() => setSelectedPolicyId(policy.policy_id)}
                      className={cn(
                        'w-full rounded-sm border p-3 text-left transition-colors',
                        isSelected
                          ? 'border-accent bg-accent/10'
                          : 'border-border hover:border-accent/40 hover:bg-background-secondary'
                      )}
                    >
                      <div className="flex items-start justify-between gap-3">
                        <div>
                          <div className="font-medium text-foreground">{policy.policy_name}</div>
                          <div className="font-mono text-xs text-muted-foreground">
                            {policy.policy_id}
                          </div>
                        </div>
                        <Badge variant={policy.active ? 'default' : 'secondary'}>
                          {policy.active ? 'Active' : 'Inactive'}
                        </Badge>
                      </div>
                      <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                        <Badge variant="outline">{policy.target_type}</Badge>
                        <Badge variant="outline">{policy.enforcement_level}</Badge>
                        <Badge variant="outline">{policy.severity}</Badge>
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>

        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <ShieldCheck className="h-4 w-4" />
                Policy Editor
              </CardTitle>
              <CardDescription>
                Keep governance definitions canonical here instead of spreading SPARQL and target metadata through unrelated workflow screens.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {formError && (
                <Alert variant="destructive">
                  <AlertTriangle className="h-4 w-4" />
                  <AlertTitle>Policy changes could not be saved</AlertTitle>
                  <AlertDescription>{formError}</AlertDescription>
                </Alert>
              )}

              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="sos-policy-id">Policy Id</Label>
                  <Input
                    id="sos-policy-id"
                    value={formState.policyId}
                    onChange={(event) =>
                      setFormState((current) => ({ ...current, policyId: event.target.value }))
                    }
                    disabled={Boolean(selectedPolicy)}
                    placeholder="policy.interface-pair.json-contract"
                    autoComplete="off"
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="sos-policy-name">Policy Name</Label>
                  <Input
                    id="sos-policy-name"
                    value={formState.policyName}
                    onChange={(event) =>
                      setFormState((current) => ({ ...current, policyName: event.target.value }))
                    }
                    placeholder="JSON payload compatibility"
                    autoComplete="off"
                  />
                </div>
              </div>

              <div className="grid gap-4 md:grid-cols-[minmax(0,1fr),auto]">
                <div className="space-y-2">
                  <Label htmlFor="sos-policy-target-type">Target Type</Label>
                  <Select
                    value={formState.targetType}
                    onValueChange={(value) =>
                      setFormState((current) => ({ ...current, targetType: value }))
                    }
                    disabled={Boolean(selectedPolicy)}
                  >
                    <SelectTrigger id="sos-policy-target-type">
                      <SelectValue placeholder="Choose a target type" />
                    </SelectTrigger>
                    <SelectContent>
                      {POLICY_TARGET_OPTIONS.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {option.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  {selectedPolicy && (
                    <p className="text-xs text-muted-foreground">
                      Target type is locked for existing policies so we do not accumulate stale target references.
                    </p>
                  )}
                </div>
                <div className="flex items-end gap-3 rounded-sm border border-border px-3 py-2">
                  <div>
                    <Label htmlFor="sos-policy-active">Active</Label>
                    <p className="text-xs text-muted-foreground">Automatic enforcement toggle</p>
                  </div>
                  <Switch
                    id="sos-policy-active"
                    checked={formState.active}
                    onCheckedChange={(checked) =>
                      setFormState((current) => ({ ...current, active: checked }))
                    }
                  />
                </div>
              </div>

              <div className="grid gap-4 md:grid-cols-2">
                <div className="space-y-2">
                  <Label>Stages</Label>
                  <div className="grid gap-2 rounded-sm border border-border p-3 sm:grid-cols-3">
                    {POLICY_STAGE_OPTIONS.map((stage) => (
                      <label
                        key={stage.value}
                        className="flex items-center gap-2 text-sm text-foreground"
                      >
                        <Checkbox
                          checked={formState.stages.includes(stage.value)}
                          onCheckedChange={(checked) =>
                            handleToggleStage(stage.value, checked === true)
                          }
                        />
                        <span>{stage.label}</span>
                      </label>
                    ))}
                  </div>
                </div>
                <div className="grid gap-4 sm:grid-cols-2">
                  <div className="space-y-2">
                    <Label htmlFor="sos-policy-enforcement">Enforcement</Label>
                    <Select
                      value={formState.enforcementLevel}
                      onValueChange={(value) =>
                        setFormState((current) => ({ ...current, enforcementLevel: value }))
                      }
                    >
                      <SelectTrigger id="sos-policy-enforcement">
                        <SelectValue placeholder="Choose enforcement" />
                      </SelectTrigger>
                      <SelectContent>
                        {ENFORCEMENT_OPTIONS.map((option) => (
                          <SelectItem key={option} value={option}>
                            {option}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="sos-policy-severity">Severity</Label>
                    <Select
                      value={formState.severity}
                      onValueChange={(value) =>
                        setFormState((current) => ({ ...current, severity: value }))
                      }
                    >
                      <SelectTrigger id="sos-policy-severity">
                        <SelectValue placeholder="Choose severity" />
                      </SelectTrigger>
                      <SelectContent>
                        {SEVERITY_OPTIONS.map((option) => (
                          <SelectItem key={option} value={option}>
                            {option}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                </div>
              </div>

              <TargetFieldsEditor
                formState={formState}
                interfaces={sortedInterfaces}
                systems={sortedSystems}
                contracts={sortedContracts}
                onChange={setFormState}
                onUseCurrentPair={currentPair ? handleUseCurrentPair : undefined}
              />

              <div className="space-y-2">
                <Label htmlFor="sos-policy-description">Description</Label>
                <Textarea
                  id="sos-policy-description"
                  value={formState.description}
                  onChange={(event) =>
                    setFormState((current) => ({ ...current, description: event.target.value }))
                  }
                  rows={2}
                  placeholder="What operational rule does this policy enforce?"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="sos-policy-query">SPARQL Query Template</Label>
                <Textarea
                  id="sos-policy-query"
                  value={formState.sparqlQuery}
                  onChange={(event) =>
                    setFormState((current) => ({ ...current, sparqlQuery: event.target.value }))
                  }
                  rows={8}
                  spellCheck={false}
                  placeholder="ASK { ?s ?p ?o }"
                />
              </div>

              <div className="grid gap-4 lg:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="sos-policy-context">Context JSON</Label>
                  <Textarea
                    id="sos-policy-context"
                    value={formState.contextText}
                    onChange={(event) =>
                      setFormState((current) => ({ ...current, contextText: event.target.value }))
                    }
                    rows={6}
                    spellCheck={false}
                    placeholder='{"classification": "UNCLASSIFIED"}'
                  />
                </div>
                <div className="grid gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="sos-policy-tags">Tags</Label>
                    <Input
                      id="sos-policy-tags"
                      value={formState.tagsText}
                      onChange={(event) =>
                        setFormState((current) => ({ ...current, tagsText: event.target.value }))
                      }
                      placeholder="governance, json, integration"
                      autoComplete="off"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="sos-policy-ontology-refs">Ontology Refs</Label>
                    <Input
                      id="sos-policy-ontology-refs"
                      value={formState.ontologyRefsText}
                      onChange={(event) =>
                        setFormState((current) => ({ ...current, ontologyRefsText: event.target.value }))
                      }
                      placeholder="urn:graphica:ontology:mission"
                      autoComplete="off"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="sos-policy-shape-refs">Shape Refs</Label>
                    <Input
                      id="sos-policy-shape-refs"
                      value={formState.shapeRefsText}
                      onChange={(event) =>
                        setFormState((current) => ({ ...current, shapeRefsText: event.target.value }))
                      }
                      placeholder="http://graphica.io/sos/interface/foo/shape/bar"
                      autoComplete="off"
                    />
                  </div>
                </div>
              </div>

              <div className="flex flex-wrap gap-2">
                <Button onClick={handleSave} disabled={pending}>
                  {createPolicy.isPending || updatePolicy.isPending ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Save className="mr-2 h-4 w-4" />
                  )}
                  {selectedPolicy ? 'Update Policy' : 'Create Policy'}
                </Button>
                <Button variant="outline" onClick={handleCreateNew} disabled={pending}>
                  Reset Form
                </Button>
                {selectedPolicy && (
                  <Button variant="destructive" onClick={handleDelete} disabled={pending}>
                    {deletePolicy.isPending ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : (
                      <Trash2 className="mr-2 h-4 w-4" />
                    )}
                    Delete Policy
                  </Button>
                )}
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Policy Evaluation</CardTitle>
              <CardDescription>
                Run the stored policy against the coordinator exactly as the production governance path would.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              {!selectedPolicy ? (
                <div className="rounded-sm border border-dashed border-border p-4 text-sm text-muted-foreground">
                  Select or save a policy to evaluate it.
                </div>
              ) : (
                <>
                  {evaluationError && (
                    <Alert variant="destructive">
                      <AlertTriangle className="h-4 w-4" />
                      <AlertTitle>Policy evaluation failed</AlertTitle>
                      <AlertDescription>{evaluationError}</AlertDescription>
                    </Alert>
                  )}

                  <div className="grid gap-4 md:grid-cols-[220px,minmax(0,1fr)]">
                    <div className="space-y-2">
                      <Label htmlFor="sos-policy-eval-stage">Stage Override</Label>
                      <Select
                        value={evaluationState.stage}
                        onValueChange={(value) =>
                          setEvaluationState((current) => ({ ...current, stage: value }))
                        }
                      >
                        <SelectTrigger id="sos-policy-eval-stage">
                          <SelectValue placeholder="Use stored default" />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="default">Use stored default</SelectItem>
                          {selectedPolicy.stages.map((stage) => (
                            <SelectItem key={stage} value={stage}>
                              {stage}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="sos-policy-eval-context">Runtime Context JSON</Label>
                      <Textarea
                        id="sos-policy-eval-context"
                        value={evaluationState.contextText}
                        onChange={(event) =>
                          setEvaluationState((current) => ({
                            ...current,
                            contextText: event.target.value,
                          }))
                        }
                        rows={4}
                        spellCheck={false}
                        placeholder='{"environment": "exercise"}'
                      />
                    </div>
                  </div>

                  <div className="flex flex-wrap gap-2">
                    <Button onClick={() => runEvaluation(true)} disabled={pending}>
                      {validatePolicy.isPending ? (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      ) : (
                        <PlayCircle className="mr-2 h-4 w-4" />
                      )}
                      Run And Persist Report
                    </Button>
                    <Button variant="outline" onClick={() => runEvaluation(false)} disabled={pending}>
                      {validatePolicyDryRun.isPending ? (
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                      ) : (
                        <PlayCircle className="mr-2 h-4 w-4" />
                      )}
                      Dry Run
                    </Button>
                    {onOpenReports && (
                      <Button
                        variant="outline"
                        onClick={() =>
                          onOpenReports({
                            subjectType: 'policy',
                            subjectKey: `policy:${selectedPolicy.policy_id}`,
                          })
                        }
                        disabled={pending}
                      >
                        Open Policy History
                      </Button>
                    )}
                  </div>

                  {evaluationResult && (
                    <ValidationResultCard
                      title={selectedPolicy.policy_name}
                      policyId={selectedPolicy.policy_id}
                      result={evaluationResult}
                      onOpenReports={onOpenReports}
                    />
                  )}
                </>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}

function TargetFieldsEditor({
  formState,
  interfaces,
  systems,
  contracts,
  onChange,
  onUseCurrentPair,
}: {
  formState: PolicyFormState;
  interfaces: SosInterfaceRecord[];
  systems: SosSystemRecord[];
  contracts: SosDataContract[];
  onChange: React.Dispatch<React.SetStateAction<PolicyFormState>>;
  onUseCurrentPair?: () => void;
}) {
  if (formState.targetType === 'global') {
    return (
      <div className="rounded-sm border border-border bg-background-secondary p-3 text-sm text-muted-foreground">
        Global policies apply across the SoS catalog and do not require a target reference.
      </div>
    );
  }

  if (formState.targetType === 'interface_pair') {
    return (
      <div className="space-y-3 rounded-sm border border-border p-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <div className="text-sm font-medium text-foreground">Interface Pair Target</div>
            <p className="text-xs text-muted-foreground">
              Policy subject key is normalized as provider -&gt; consumer.
            </p>
          </div>
          {onUseCurrentPair && (
            <Button type="button" variant="outline" size="sm" onClick={onUseCurrentPair}>
              Use Current Pair
            </Button>
          )}
        </div>
        <div className="grid gap-4 md:grid-cols-2">
          <ReferenceField
            id="sos-policy-provider-interface"
            label="Provider Interface Id"
            value={formState.providerInterfaceId}
            placeholder="iface.provider"
            listId="sos-policy-interface-options"
            onChange={(value) =>
              onChange((current) => ({ ...current, providerInterfaceId: value }))
            }
          />
          <ReferenceField
            id="sos-policy-consumer-interface"
            label="Consumer Interface Id"
            value={formState.consumerInterfaceId}
            placeholder="iface.consumer"
            listId="sos-policy-interface-options"
            onChange={(value) =>
              onChange((current) => ({ ...current, consumerInterfaceId: value }))
            }
          />
        </div>
        <datalist id="sos-policy-interface-options">
          {interfaces.map((item) => (
            <option key={item.interface_id} value={item.interface_id}>
              {item.interface_name}
            </option>
          ))}
        </datalist>
      </div>
    );
  }

  if (formState.targetType === 'contract') {
    return (
      <div className="space-y-3 rounded-sm border border-border p-3">
        <div>
          <div className="text-sm font-medium text-foreground">Contract Target</div>
          <p className="text-xs text-muted-foreground">
            Bind the policy to an existing provider/consumer contract.
          </p>
        </div>
        <ReferenceField
          id="sos-policy-contract-id"
          label="Contract Id"
          value={formState.contractId}
          placeholder="contract.provider.to.consumer"
          listId="sos-policy-contract-options"
          onChange={(value) => onChange((current) => ({ ...current, contractId: value }))}
        />
        <datalist id="sos-policy-contract-options">
          {contracts.map((item) => (
            <option key={item.contract_id} value={item.contract_id}>
              {item.contract_name}
            </option>
          ))}
        </datalist>
      </div>
    );
  }

  if (formState.targetType === 'system_pair') {
    return (
      <div className="space-y-3 rounded-sm border border-border p-3">
        <div>
          <div className="text-sm font-medium text-foreground">System Pair Target</div>
          <p className="text-xs text-muted-foreground">
            Use source and target systems for integration-level governance.
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-2">
          <ReferenceField
            id="sos-policy-source-system"
            label="Source System Id"
            value={formState.sourceSystemId}
            placeholder="sys.source"
            listId="sos-policy-system-options"
            onChange={(value) => onChange((current) => ({ ...current, sourceSystemId: value }))}
          />
          <ReferenceField
            id="sos-policy-target-system"
            label="Target System Id"
            value={formState.targetSystemId}
            placeholder="sys.target"
            listId="sos-policy-system-options"
            onChange={(value) => onChange((current) => ({ ...current, targetSystemId: value }))}
          />
        </div>
        <datalist id="sos-policy-system-options">
          {systems.map((item) => (
            <option key={item.system_id} value={item.system_id}>
              {item.system_name}
            </option>
          ))}
        </datalist>
      </div>
    );
  }

  return (
    <div className="space-y-3 rounded-sm border border-border p-3">
      <div>
        <div className="text-sm font-medium text-foreground">Interface Target</div>
        <p className="text-xs text-muted-foreground">
          Scope the policy to one interface definition.
        </p>
      </div>
      <ReferenceField
        id="sos-policy-interface-id"
        label="Interface Id"
        value={formState.interfaceId}
        placeholder="iface.catalogue.entry"
        listId="sos-policy-interface-options-single"
        onChange={(value) => onChange((current) => ({ ...current, interfaceId: value }))}
      />
      <datalist id="sos-policy-interface-options-single">
        {interfaces.map((item) => (
          <option key={item.interface_id} value={item.interface_id}>
            {item.interface_name}
          </option>
        ))}
      </datalist>
    </div>
  );
}

function ReferenceField({
  id,
  label,
  value,
  placeholder,
  listId,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  placeholder: string;
  listId?: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        list={listId}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        autoComplete="off"
      />
    </div>
  );
}

function ValidationResultCard({
  title,
  policyId,
  result,
  onOpenReports,
}: {
  title: string;
  policyId: string;
  result: SosValidationResponse;
  onOpenReports?: (target?: ReportsTarget) => void;
}) {
  return (
    <div className="space-y-3 rounded-sm border border-border p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-sm font-medium text-foreground">{title}</div>
          <p className="text-xs text-muted-foreground">Validation id {result.validation_id}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Badge variant={result.passed ? 'default' : 'destructive'}>
            {result.passed ? 'Passed' : 'Failed'}
          </Badge>
          <Badge variant="outline">Confidence {formatPercent(result.confidence)}</Badge>
        </div>
      </div>

      <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
        {result.checks.map((check) => (
          <div key={check.check_name} className="rounded-sm border border-border bg-background-secondary p-3">
            <div className="flex items-center justify-between gap-2">
              <div className="text-sm font-medium text-foreground">{check.check_name}</div>
              <Badge variant={check.passed ? 'outline' : 'secondary'}>{check.severity}</Badge>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">{check.description}</p>
          </div>
        ))}
      </div>

      {result.report_id && onOpenReports && (
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            onOpenReports({
              reportId: result.report_id,
              subjectType: 'policy',
              subjectKey: `policy:${policyId}`,
            })
          }
        >
          Open Persisted Report
        </Button>
      )}
    </div>
  );
}

function buildTargetFields(
  formState: PolicyFormState
): { ok: true; value: Record<string, string> } | { ok: false; error: string } {
  switch (formState.targetType) {
    case 'global':
      return { ok: true, value: {} };
    case 'interface_pair': {
      const providerInterfaceId = formState.providerInterfaceId.trim();
      const consumerInterfaceId = formState.consumerInterfaceId.trim();
      if (!providerInterfaceId || !consumerInterfaceId) {
        return {
          ok: false,
          error: 'Interface-pair policies require both provider and consumer interface ids.',
        };
      }
      return {
        ok: true,
        value: {
          provider_interface_id: providerInterfaceId,
          consumer_interface_id: consumerInterfaceId,
        },
      };
    }
    case 'contract': {
      const contractId = formState.contractId.trim();
      if (!contractId) {
        return { ok: false, error: 'Contract policies require a contract id.' };
      }
      return { ok: true, value: { contract_id: contractId } };
    }
    case 'system_pair': {
      const sourceSystemId = formState.sourceSystemId.trim();
      const targetSystemId = formState.targetSystemId.trim();
      if (!sourceSystemId || !targetSystemId) {
        return {
          ok: false,
          error: 'System-pair policies require both source and target system ids.',
        };
      }
      return {
        ok: true,
        value: {
          source_system_id: sourceSystemId,
          target_system_id: targetSystemId,
        },
      };
    }
    case 'interface': {
      const interfaceId = formState.interfaceId.trim();
      if (!interfaceId) {
        return { ok: false, error: 'Interface policies require an interface id.' };
      }
      return { ok: true, value: { interface_id: interfaceId } };
    }
    default:
      return { ok: false, error: `Unsupported policy target type '${formState.targetType}'.` };
  }
}

function emptyPolicyFormState(
  currentPair?: {
    providerInterfaceId: string;
    consumerInterfaceId: string;
  } | null
): PolicyFormState {
  return {
    policyId: '',
    policyName: '',
    description: '',
    targetType: currentPair ? 'interface_pair' : 'global',
    stages: ['pre_execution'],
    enforcementLevel: 'mandatory',
    severity: 'medium',
    sparqlQuery: 'ASK { ?s ?p ?o }',
    contextText: '{}',
    tagsText: '',
    ontologyRefsText: '',
    shapeRefsText: '',
    active: true,
    providerInterfaceId: currentPair?.providerInterfaceId ?? '',
    consumerInterfaceId: currentPair?.consumerInterfaceId ?? '',
    contractId: '',
    sourceSystemId: '',
    targetSystemId: '',
    interfaceId: '',
  };
}

function policyToFormState(policy: SosPolicyRecord): PolicyFormState {
  return {
    policyId: policy.policy_id,
    policyName: policy.policy_name,
    description: policy.description ?? '',
    targetType: policy.target_type,
    stages: policy.stages,
    enforcementLevel: policy.enforcement_level,
    severity: policy.severity,
    sparqlQuery: policy.sparql_query,
    contextText: prettyJson(policy.context),
    tagsText: policy.tags.join(', '),
    ontologyRefsText: policy.ontology_refs.join(', '),
    shapeRefsText: policy.shape_refs.join(', '),
    active: policy.active,
    providerInterfaceId: policy.provider_interface_id ?? '',
    consumerInterfaceId: policy.consumer_interface_id ?? '',
    contractId: policy.contract_id ?? '',
    sourceSystemId: policy.source_system_id ?? '',
    targetSystemId: policy.target_system_id ?? '',
    interfaceId: policy.interface_id ?? '',
  };
}

function parseObjectJson(
  value: string,
  label: string
): { ok: true; value: Record<string, unknown> } | { ok: false; error: string } {
  const trimmed = value.trim();
  if (!trimmed) {
    return { ok: true, value: {} };
  }

  try {
    const parsed = JSON.parse(trimmed) as unknown;
    if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
      return { ok: false, error: `${label} must be a JSON object.` };
    }
    return { ok: true, value: parsed as Record<string, unknown> };
  } catch {
    return { ok: false, error: `${label} must be valid JSON.` };
  }
}

function parseCsvList(value: string): string[] {
  return dedupeStrings(
    value
      .split(',')
      .map((entry) => entry.trim())
      .filter(Boolean)
  );
}

function dedupeStrings(values: string[]): string[] {
  return Array.from(new Set(values));
}

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function prettyJson(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
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

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}
