import React, { useState, useCallback, useRef, useMemo, useEffect } from 'react';
import ReactFlow, {
  Node,
  Edge,
  Controls,
  Background,
  BackgroundVariant,
  useNodesState,
  useEdgesState,
  addEdge,
  Connection,
  MiniMap,
  NodeTypes,
  Panel,
  MarkerType,
  ReactFlowProvider,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { motion } from 'framer-motion';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import {
  Save,
  FolderOpen,
  Plus,
  Loader2,
  CheckCircle,
  History,
  FlaskConical,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';
import {
  useWorkflows,
  useRegisterWorkflow,
  useUpdateWorkflow,
  useDeleteWorkflow,
  useDryRunWorkflow,
  useScheduleWorkflow,
  useWorkflowSchedule,
  useWorkflowExecutions,
} from '@/hooks/useWorkflows';
import * as workflowApi from '@/api/workflows';
import { useCancelETLJob } from '@/hooks/useETL';
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet';
import { WorkflowNodeV2 } from '@/components/workflow/premium';
import { ConditionalRouterNode } from '@/components/workflow/ConditionalRouterNode';
import { NodePalette } from '@/components/workflow/NodePalette';
import { KeyboardShortcutsDialog } from '@/components/workflow/KeyboardShortcutsDialog';
import { AutoSaveIndicator } from '@/components/workflow/AutoSaveIndicator';
import { ExecutionProgress } from '@/components/workflow/ExecutionProgress';
import { StepConfigDialog } from '@/components/workflow/StepConfigDialog';
import { ScheduleWorkflowDialog } from '@/components/workflow/ScheduleWorkflowDialog';
import { ExecutionCommandBar } from '@/components/workflow/ExecutionCommandBar';
import { ScheduleStatusBadge } from '@/components/workflow/ScheduleStatusBadge';
import { ExecutionHistoryViewer } from '@/components/workflow/ExecutionHistoryViewer';
import { ExecutionDetailsDialog } from '@/components/workflow/ExecutionDetailsDialog';
import { ExecuteWorkflowDialog } from '@/components/workflow/ExecuteWorkflowDialog';
import { SavedWorkflowsPanel } from '@/components/workflow/SavedWorkflowsPanel';
import { useKeyboardShortcuts } from '@/hooks/useKeyboardShortcuts';
import { useWorkflowAutoSave } from '@/hooks/useWorkflowAutoSave';
import { useWorkflowExecution } from '@/hooks/useWorkflowExecution';
import { validateConnection, showValidationError, validateWorkflow } from '@/lib/workflow-validation';
import { getStepTypeConfig } from '@/lib/workflow-step-config';
import { getETLStepTypeConfig, isETLStepType } from '@/lib/workflow-etl-config';
import type {
  StepType,
  WorkflowDefinition,
  Workflow,
  WorkflowExecutionRequest,
  WorkflowValidationIssue,
} from '@/api/types';

// Node types configuration
const nodeTypes: NodeTypes = {
  custom: WorkflowNodeV2,
  conditional: ConditionalRouterNode,
};

const initialNodes: Node[] = [];
const initialEdges: Edge[] = [];
const DEFAULT_DRY_RUN_INPUT = '{\n  "data": "sample input"\n}';
const SELF_SOURCING_STEP_TYPES = new Set(['csv_source', 'db_extract', 'multi_source_input']);

function WorkflowDesignerInner() {
  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);
  const [workflowName, setWorkflowName] = useState('New Workflow');
  const [workflowId, setWorkflowId] = useState<string>('');
  const [selectedWorkflowId, setSelectedWorkflowId] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);
  const [isConfigDialogOpen, setIsConfigDialogOpen] = useState(false);
  const [isWorkflowListOpen, setIsWorkflowListOpen] = useState(false);
  const [isExecutionHistoryOpen, setIsExecutionHistoryOpen] = useState(false);
  const [showDryRunDialog, setShowDryRunDialog] = useState(false);
  const [showExecuteDialog, setShowExecuteDialog] = useState(false);
  const [isToolPaletteCollapsed, setIsToolPaletteCollapsed] = useState(false);
  const [showExecutionDetailsDialog, setShowExecutionDetailsDialog] = useState(false);
  const [selectedExecutionId, setSelectedExecutionId] = useState<string | null>(null);
  const [dryRunInput, setDryRunInput] = useState(DEFAULT_DRY_RUN_INPUT);
  const [dryRunResult, setDryRunResult] = useState<any>(null);
  const [showScheduleDialog, setShowScheduleDialog] = useState(false);
  const [isWorkflowsPanelCollapsed, setIsWorkflowsPanelCollapsed] = useState(false);
  const [backendValidationIssues, setBackendValidationIssues] = useState<WorkflowValidationIssue[]>([]);
  const [isBackendValidating, setIsBackendValidating] = useState(false);
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const [reactFlowInstance, setReactFlowInstance] = useState<any>(null);

  // Phase 2.3: Dark mode detection
  const isDark = useMemo(
    () => document.documentElement.classList.contains('dark'),
    []
  );

  // API hooks
  const { data: workflows, isLoading: loadingWorkflows } = useWorkflows();
  const registerWorkflow = useRegisterWorkflow();
  const updateWorkflow = useUpdateWorkflow();
  const deleteWorkflow = useDeleteWorkflow();
  const dryRun = useDryRunWorkflow();
  const scheduleWorkflow = useScheduleWorkflow();
  const { data: workflowSchedules } = useWorkflowSchedule(selectedWorkflowId || undefined);
  const { data: workflowExecutions, isLoading: loadingExecutions, refetch: refetchExecutions } = useWorkflowExecutions(selectedWorkflowId || undefined);
  const cancelJob = useCancelETLJob();

  // Auto-collapse workflows panel when empty
  useEffect(() => {
    if (!loadingWorkflows && workflows && workflows.length === 0 && !isWorkflowsPanelCollapsed) {
      setIsWorkflowsPanelCollapsed(true);
    }
  }, [workflows, loadingWorkflows, isWorkflowsPanelCollapsed]);

  // Get primary schedule (first enabled schedule, or first schedule if none enabled)
  const rawPrimarySchedule = workflowSchedules?.find(s => s.enabled) || workflowSchedules?.[0];

  // Map primary schedule to ScheduleStatusBadge format
  const primarySchedule = rawPrimarySchedule && rawPrimarySchedule.cron_expression ? {
    enabled: rawPrimarySchedule.enabled,
    cron_expression: rawPrimarySchedule.cron_expression,
    timezone: rawPrimarySchedule.timezone || undefined,
    next_run_time: rawPrimarySchedule.next_execution || undefined,
  } : undefined;

  // Auto-save hook
  const autoSave = useWorkflowAutoSave(nodes, edges, {
    workflowId: selectedWorkflowId || '',
    workflowName,
    enabled: !!selectedWorkflowId,
    debounceMs: 3000,
  });

  // Execution hook
  const execution = useWorkflowExecution(selectedWorkflowId || '', setNodes);

  // Keyboard shortcuts
  useKeyboardShortcuts(nodes, edges, setNodes, setEdges, {
    onSave: () => autoSave.save(),
    onAutoLayout: () => {
      toast.info('Auto-layout coming soon!');
    },
    onExecute: () => {
      openExecuteDialog();
    },
  });

  const onConnect = useCallback(
    (params: Connection) => {
      const validation = validateConnection(params, nodes, edges);

      if (!validation.valid) {
        showValidationError(validation.error || 'Invalid connection');
        return;
      }

      // Determine edge styling based on source node type and handle
      const sourceNode = nodes.find((n) => n.id === params.source);
      const isConditional = sourceNode?.type === 'conditional';
      const handleId = params.sourceHandle;

      let edgeStyle: any = {
        stroke: 'hsl(var(--primary))',
        strokeWidth: 2,
      };
      let edgeColor = 'hsl(var(--primary))';
      let edgeLabel = '';

      // Conditional branch styling
      if (isConditional) {
        if (handleId === 'true') {
          edgeStyle = {
            stroke: '#10B981', // green for TRUE
            strokeWidth: 2,
          };
          edgeColor = '#10B981';
          edgeLabel = 'TRUE';
        } else if (handleId === 'false') {
          edgeStyle = {
            stroke: '#EF4444', // red for FALSE
            strokeWidth: 2,
            strokeDasharray: '5,5', // dashed for FALSE
          };
          edgeColor = '#EF4444';
          edgeLabel = 'FALSE';
        }
      }

      setEdges((eds) =>
        addEdge(
          {
            ...params,
            type: 'smoothstep',
            animated: true,
            style: edgeStyle,
            label: edgeLabel || undefined,
            labelStyle: { fill: edgeColor, fontWeight: 600, fontSize: 10 },
            labelBgStyle: { fill: isDark ? 'rgba(15,23,42,0.9)' : 'white', fillOpacity: 0.8 },
            markerEnd: {
              type: MarkerType.ArrowClosed,
              color: edgeColor,
            },
          },
          eds
        )
      );
    },
    [nodes, edges, setEdges, isDark]
  );

  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  // Node management functions (must be before onDrop to avoid hoisting errors)
  const deleteNode = useCallback((nodeId: string) => {
    setNodes((nds) => nds.filter((node) => node.id !== nodeId));
    setEdges((eds) =>
      eds.filter((edge) => edge.source !== nodeId && edge.target !== nodeId)
    );
  }, [setNodes, setEdges]);

  const configureNode = useCallback((nodeId: string) => {
    const node = nodes.find((n) => n.id === nodeId);
    if (node) {
      setSelectedNode(node);
      setIsConfigDialogOpen(true);
    }
  }, [nodes]);

  const duplicateNode = useCallback((nodeId: string) => {
    setNodes((nds) => {
      const nodeToDuplicate = nds.find((n) => n.id === nodeId);
      if (!nodeToDuplicate) return nds;

      const newNode: Node = {
        ...nodeToDuplicate,
        id: `step_${Date.now()}`,
        position: {
          x: nodeToDuplicate.position.x + 60,
          y: nodeToDuplicate.position.y + 40,
        },
        data: {
          ...nodeToDuplicate.data,
          label: `${nodeToDuplicate.data.label} (Copy)`,
          onDelete: deleteNode,
          onDuplicate: duplicateNode,
          onConfigure: configureNode,
        },
      };

      toast.success(`Duplicated: ${nodeToDuplicate.data.label}`);
      return nds.concat(newNode);
    });
  }, [setNodes, deleteNode, configureNode]);

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();

      if (!reactFlowWrapper.current || !reactFlowInstance) return;

      const stepType = event.dataTransfer.getData('application/reactflow') as StepType;
      const toolData = JSON.parse(event.dataTransfer.getData('application/tool'));

      const reactFlowBounds = reactFlowWrapper.current.getBoundingClientRect();
      const position = reactFlowInstance.project({
        x: event.clientX - reactFlowBounds.left,
        y: event.clientY - reactFlowBounds.top,
      });

      // Use 'conditional' node type for conditional_router, otherwise 'custom'
      const nodeType = stepType === 'conditional_router' ? 'conditional' : 'custom';

      const newNode: Node = {
        id: `step_${Date.now()}`,
        type: nodeType,
        position,
        data: {
          label: toolData.label,
          step_type: stepType,
          config: {},
          executionStatus: 'idle',
          onDelete: deleteNode,
          onDuplicate: duplicateNode,
          onConfigure: configureNode,
        },
      };

      setNodes((nds) => nds.concat(newNode));
    },
    [reactFlowInstance, setNodes, deleteNode, duplicateNode, configureNode]
  );

  const handleDragStart = (event: React.DragEvent, stepType: string, label: string) => {
    event.dataTransfer.setData('application/reactflow', stepType);
    event.dataTransfer.setData(
      'application/tool',
      JSON.stringify({ type: stepType, label })
    );
    event.dataTransfer.effectAllowed = 'move';
  };

  // Convert React Flow nodes/edges to backend workflow definition
  const convertToWorkflowDefinition = useCallback((): WorkflowDefinition => {
    // Map nodes to workflow steps
    const steps = nodes.map((node) => {
      // Get dependencies from edges
      const depends_on = edges
        .filter((edge) => edge.target === node.id)
        .map((edge) => edge.source);

      // Only include config if it has actual properties
      // Empty {} causes backend deserialization errors
      const hasConfig = node.data.config && Object.keys(node.data.config).length > 0;

      const step: any = {
        id: node.id,
        step_type: node.data.step_type,
        depends_on: depends_on.length > 0 ? depends_on : undefined,
      };

      // Only add config if it's not empty
      if (hasConfig) {
        step.config = node.data.config;
      }

      return step;
    });

    return {
      steps,
      fusion_threshold: 0.85, // Default threshold
      fallback: 'manual_review' as const,
    };
  }, [edges, nodes]);

  // Convert backend workflow definition to React Flow format
  const loadWorkflowFromDefinition = (definition: WorkflowDefinition) => {
    // Reconstruct nodes from steps
    const newNodes = definition.steps?.map((step, idx) => {
      // Use 'conditional' node type for conditional_router, otherwise 'custom'
      return {
        id: step.id,
        type: (step.step_type === 'conditional_router' ? 'conditional' : 'custom') as 'conditional' | 'custom',
        position: { x: 250 + (idx % 3) * 300, y: 150 + Math.floor(idx / 3) * 150 },
        data: {
          label: step.id,
          step_type: step.step_type,
          config: step.config || {},
          executionStatus: 'idle' as const,
          onDelete: deleteNode,
          onDuplicate: duplicateNode,
          onConfigure: configureNode, // ✅ Fixed: Added onConfigure handler
        },
      };
    }) || [];

    // Reconstruct edges from depends_on
    const newEdges: Edge[] = [];
    definition.steps?.forEach((step) => {
      step.depends_on?.forEach((sourceId) => {
        newEdges.push({
          id: `e${sourceId}-${step.id}`,
          source: sourceId,
          target: step.id,
          type: 'smoothstep',
          animated: true,
          style: { stroke: 'hsl(var(--primary))', strokeWidth: 2 },
          markerEnd: {
            type: MarkerType.ArrowClosed,
            color: 'hsl(var(--primary))',
          },
        });
      });
    });

    setNodes(newNodes);
    setEdges(newEdges);
  };

  const applyNodeValidationIssues = useCallback((issues: WorkflowValidationIssue[]) => {
    const issueByStep = new Map<string, string>();

    issues
      .filter((issue) => issue.level === 'error' && issue.step_id && issue.step_id !== '$workflow')
      .forEach((issue) => {
        if (!issueByStep.has(issue.step_id)) {
          issueByStep.set(issue.step_id, issue.message);
        }
      });

    setNodes((currentNodes) => {
      let changed = false;
      const nextNodes = currentNodes.map((node) => {
        const nextError = issueByStep.get(node.id);
        const currentError = node.data?.validationError;

        if ((currentError || undefined) === nextError) {
          return node;
        }

        changed = true;
        return {
          ...node,
          data: {
            ...node.data,
            validationError: nextError,
          },
        };
      });

      return changed ? nextNodes : currentNodes;
    });

    setSelectedNode((currentSelectedNode) => {
      if (!currentSelectedNode) {
        return currentSelectedNode;
      }

      const nextError = issueByStep.get(currentSelectedNode.id);
      const currentError = currentSelectedNode.data?.validationError;

      if ((currentError || undefined) === nextError) {
        return currentSelectedNode;
      }

      return {
        ...currentSelectedNode,
        data: {
          ...currentSelectedNode.data,
          validationError: nextError,
        },
      };
    });
  }, [setNodes]);

  const localValidation = useMemo(
    () => validateWorkflow(nodes, edges, { includeNodeValidationErrors: false }),
    [edges, nodes]
  );

  const supportsInputlessExecution = useMemo(() => {
    if (nodes.length === 0) {
      return false;
    }

    const nodeIdsWithIncomingEdges = new Set(edges.map((edge) => edge.target));
    return nodes.some(
      (node) =>
        !nodeIdsWithIncomingEdges.has(node.id) &&
        SELF_SOURCING_STEP_TYPES.has(String(node.data?.step_type || ''))
    );
  }, [edges, nodes]);

  useEffect(() => {
    if (supportsInputlessExecution && dryRunInput === DEFAULT_DRY_RUN_INPUT) {
      setDryRunInput('null');
      return;
    }

    if (!supportsInputlessExecution && dryRunInput.trim() === 'null') {
      setDryRunInput(DEFAULT_DRY_RUN_INPUT);
    }
  }, [dryRunInput, supportsInputlessExecution]);

  const blockingBackendIssues = useMemo(
    () => backendValidationIssues.filter((issue) => issue.level === 'error'),
    [backendValidationIssues]
  );

  const validationErrors = useMemo(() => {
    const errors = [...localValidation.errors];

    if (nodes.length === 0) {
      errors.push('Workflow is empty. Add at least one node to execute.');
    }

    if (!selectedWorkflowId) {
      errors.push('Please save the workflow first.');
    }

    blockingBackendIssues.forEach((issue) => {
      errors.push(
        issue.step_id === '$workflow'
          ? issue.message
          : `${issue.step_id}: ${issue.message}`
      );
    });

    if (isBackendValidating) {
      errors.push('Validating datasource-backed workflow steps...');
    }

    return Array.from(new Set(errors));
  }, [blockingBackendIssues, isBackendValidating, localValidation.errors, nodes.length, selectedWorkflowId]);

  const canRunWorkflow = Boolean(
    selectedWorkflowId &&
      !execution.isExecuting &&
      nodes.length > 0 &&
      localValidation.valid &&
      !isBackendValidating &&
      blockingBackendIssues.length === 0
  );

  const runBackendValidation = useCallback(
    async (options?: { showToast?: boolean }) => {
      const definition = convertToWorkflowDefinition();

      setIsBackendValidating(true);

      try {
        const response = await workflowApi.validateWorkflowDefinition(definition);
        const issues = response.issues || [];

        setBackendValidationIssues(issues);
        applyNodeValidationIssues(issues);

        if (options?.showToast) {
          if (response.valid) {
            toast.success('Workflow validation passed', {
              description: response.warnings?.length
                ? `${response.warnings.length} warning${response.warnings.length === 1 ? '' : 's'}`
                : `${response.step_count || definition.steps.length} steps validated`,
            });
          } else {
            toast.error('Workflow validation failed', {
              description:
                issues[0]?.message ||
                response.message ||
                'Datasource-backed validation reported blocking issues.',
            });
          }
        }

        return response;
      } catch (error: any) {
        const message = error?.message || 'Validation request failed';
        const fallbackIssues: WorkflowValidationIssue[] = [
          {
            level: 'error',
            step_id: '$workflow',
            code: 'validation_request_failed',
            message,
          },
        ];

        setBackendValidationIssues(fallbackIssues);
        applyNodeValidationIssues([]);

        if (options?.showToast) {
          toast.error('Validation request failed', {
            description: message,
          });
        }

        return {
          valid: false,
          message,
          warnings: [],
          step_count: definition.steps.length,
          has_conditional_logic: false,
          has_error_handling: false,
          issues: fallbackIssues,
        };
      } finally {
        setIsBackendValidating(false);
      }
    },
    [applyNodeValidationIssues, convertToWorkflowDefinition]
  );

  useEffect(() => {
    if (nodes.length === 0 || !localValidation.valid) {
      setBackendValidationIssues([]);
      applyNodeValidationIssues([]);
      setIsBackendValidating(false);
      return;
    }

    let cancelled = false;
    const timeoutId = window.setTimeout(async () => {
      setIsBackendValidating(true);

      try {
        const response = await workflowApi.validateWorkflowDefinition(convertToWorkflowDefinition());
        if (cancelled) {
          return;
        }

        const issues = response.issues || [];
        setBackendValidationIssues(issues);
        applyNodeValidationIssues(issues);
      } catch (error: any) {
        if (cancelled) {
          return;
        }

        setBackendValidationIssues([
          {
            level: 'error',
            step_id: '$workflow',
            code: 'validation_request_failed',
            message: error?.message || 'Validation request failed',
          },
        ]);
        applyNodeValidationIssues([]);
      } finally {
        if (!cancelled) {
          setIsBackendValidating(false);
        }
      }
    }, 500);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [applyNodeValidationIssues, convertToWorkflowDefinition, localValidation.valid, nodes.length]);

  const saveWorkflow = () => {
    if (!workflowName.trim()) {
      toast.error('Please enter a workflow name');
      return;
    }

    if (nodes.length === 0) {
      toast.error('Cannot save empty workflow. Add at least one step.');
      return;
    }

    const definition = convertToWorkflowDefinition();

    if (selectedWorkflowId) {
      // Update existing workflow
      updateWorkflow.mutate({
        workflowId: selectedWorkflowId,
        request: {
          name: workflowName,
          definition,
        },
      });
    } else {
      // Create new workflow
      const id = workflowId.trim() || workflowName.toLowerCase().replace(/\s+/g, '_');
      registerWorkflow.mutate({
        id,
        name: workflowName,
        definition,
      }, {
        onSuccess: (data) => {
          setWorkflowId(data.id);
          setSelectedWorkflowId(data.id);
        },
      });
    }
  };

  const loadWorkflow = async (workflow: Workflow) => {
    try {
      // Fetch full workflow with definition from backend
      const fullWorkflow = await workflowApi.getWorkflow(workflow.id);

      setWorkflowName(fullWorkflow.name);
      setWorkflowId(fullWorkflow.id);
      setSelectedWorkflowId(fullWorkflow.id);
      setBackendValidationIssues([]);
      applyNodeValidationIssues([]);
      loadWorkflowFromDefinition(fullWorkflow.definition);
      setIsWorkflowListOpen(false);

      toast.success('Workflow loaded', {
        description: `Loaded "${fullWorkflow.name}" with ${fullWorkflow.definition.steps?.length || 0} steps`,
      });
    } catch (error: any) {
      toast.error('Failed to load workflow', {
        description: error.message || 'Could not fetch workflow definition',
      });
    }
  };

  const validateCurrentWorkflow = async () => {
    if (!localValidation.valid) {
      toast.error('Workflow validation failed', {
        description: localValidation.errors.join('\n'),
      });
      return;
    }

    const response = await runBackendValidation({ showToast: true });

    if (response.valid && response.warnings && response.warnings.length > 0) {
      toast.warning('Workflow validation warnings', {
        description: response.warnings.join('\n'),
      });
    }
  };

  async function openExecuteDialog() {
    if (!selectedWorkflowId) {
      toast.error('Please save the workflow first');
      return;
    }

    if (!localValidation.valid) {
      toast.error(`Cannot execute: ${localValidation.errors.join(', ')}`);
      return;
    }

    const validation = await runBackendValidation();
    if (!validation.valid) {
      toast.error('Cannot execute workflow', {
        description:
          validation.issues?.find((issue) => issue.level === 'error')?.message ||
          validation.message,
      });
      return;
    }

    setShowExecuteDialog(true);
  }

  const executeCurrentWorkflow = async (request: WorkflowExecutionRequest) => {
    const result = await execution.execute(request);

    if (!result) {
      return;
    }

    setSelectedExecutionId(result.execution_id);

    if (result.materialized_dataset) {
      toast.success('Workflow output materialized', {
        description: `${result.materialized_dataset.name} is now available in the catalogue`,
      });
    }
  };

  const stopCurrentWorkflow = () => {
    if (!execution.executionId) {
      toast.error('No execution to cancel');
      return;
    }

    // Cancel the ETL job using the execution ID
    cancelJob.mutate(execution.executionId, {
      onSuccess: () => {
        // Reset execution state after successful cancellation
        execution.reset();
        toast.success('Workflow execution cancelled');
      },
    });
  };

  const pauseCurrentWorkflow = () => {
    // TODO: Implement pause functionality once backend API is available
    toast.info('Pause functionality coming soon - waiting for backend API');
  };

  const resumeCurrentWorkflow = () => {
    // TODO: Implement resume functionality once backend API is available
    toast.info('Resume functionality coming soon - waiting for backend API');
  };

  const handleDryRun = async () => {
    if (!selectedWorkflowId) {
      toast.error('Please save the workflow first');
      return;
    }

    if (!localValidation.valid) {
      toast.error(`Cannot dry-run: ${localValidation.errors.join(', ')}`);
      return;
    }

    const validation = await runBackendValidation();
    if (!validation.valid) {
      toast.error('Dry-run blocked by validation issues', {
        description:
          validation.issues?.find((issue) => issue.level === 'error')?.message ||
          validation.message,
      });
      return;
    }

    try {
      const input =
        supportsInputlessExecution && !dryRunInput.trim()
          ? null
          : JSON.parse(dryRunInput);

      const result = await dryRun.mutateAsync({
        workflowId: selectedWorkflowId,
        request: { input },
      });

      setDryRunResult(result);
    } catch (error: any) {
      if (error.message?.includes('JSON')) {
        toast.error('Invalid JSON in input');
      }
    }
  };

  const handleSchedule = async (request: import('@/api/types').ScheduleWorkflowRequest) => {
    if (!selectedWorkflowId) {
      toast.error('Please save the workflow first');
      return;
    }

    if (!localValidation.valid) {
      toast.error(`Cannot schedule: ${localValidation.errors.join(', ')}`);
      return;
    }

    const validation = await runBackendValidation();
    if (!validation.valid) {
      toast.error('Scheduling blocked by validation issues', {
        description:
          validation.issues?.find((issue) => issue.level === 'error')?.message ||
          validation.message,
      });
      return;
    }

    await scheduleWorkflow.mutateAsync({
      workflowId: selectedWorkflowId,
      request,
    });
  };

  const newWorkflow = () => {
    setWorkflowName('New Workflow');
    setWorkflowId('');
    setSelectedWorkflowId(null);
    setBackendValidationIssues([]);
    applyNodeValidationIssues([]);
    setNodes(initialNodes);
    setEdges(initialEdges);
    setSelectedNode(null);
  };

  const clearWorkflow = () => {
    setBackendValidationIssues([]);
    applyNodeValidationIssues([]);
    setNodes([]);
    setEdges([]);
    setSelectedNode(null);
  };

  const handleDeleteWorkflow = (workflowId: string) => {
    if (confirm('Are you sure you want to delete this workflow?')) {
      deleteWorkflow.mutate(workflowId, {
        onSuccess: () => {
          // If we're currently viewing this workflow, clear the designer
          if (selectedWorkflowId === workflowId) {
            newWorkflow();
          }
        },
      });
    }
  };

  const handleDuplicateWorkflow = async (workflow: Workflow) => {
    try {
      const newName = `${workflow.name} (Copy)`;
      const newId = `${workflow.id}_copy_${Date.now()}`;

      await registerWorkflow.mutateAsync({
        id: newId,
        name: newName,
        definition: workflow.definition,
      });

      toast.success(`Workflow duplicated as "${newName}"`);
    } catch (error: any) {
      toast.error('Failed to duplicate workflow');
    }
  };

  const onNodeClick = useCallback((event: React.MouseEvent, node: Node) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedNode(node);
  }, []);

  const onNodeDoubleClick = useCallback((event: React.MouseEvent, node: Node) => {
    event.preventDefault();
    event.stopPropagation();
    setSelectedNode(node);
    setIsConfigDialogOpen(true);
  }, []);

  const onPaneClick = useCallback(() => {
    setSelectedNode(null);
  }, []);

  const updateNodeData = (nodeId: string, data: any) => {
    let updatedData: any = null;

    setNodes((nds) =>
      nds.map((node) => {
        if (node.id === nodeId) {
          // Merge data deeply to preserve all fields
          updatedData = {
            ...node.data,
            ...data,
            config: {
              ...node.data.config,
              ...data.config,
            },
          };
          return { ...node, data: updatedData };
        }
        return node;
      })
    );

    if (selectedNode?.id === nodeId && updatedData) {
      setSelectedNode((prev) =>
        prev ? { ...prev, data: updatedData } : null
      );
    }
  };

  return (
    <div className="flex flex-col h-[calc(100vh-8rem)]">
      {/* Page Header */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.15 }}
        className="flex items-start justify-between pb-4 mb-4 border-b-2 border-border flex-shrink-0"
      >
        <div className="min-w-0 flex-1">
          <h1 className="text-2xl font-semibold text-foreground mb-1">
            Workflow Designer
          </h1>
          <p className="text-sm text-muted-foreground">
            Design data deduplication and merging workflows
          </p>
        </div>

        <div className="flex gap-2 ml-4 items-center">
          <Button variant="outline" size="sm" className="gap-2" onClick={newWorkflow}>
            <Plus className="h-4 w-4" />
            New
          </Button>

          <Button
            variant="outline"
            size="sm"
            className="gap-2"
            onClick={() => setIsWorkflowsPanelCollapsed(!isWorkflowsPanelCollapsed)}
          >
            <FolderOpen className="h-4 w-4" />
            {isWorkflowsPanelCollapsed ? 'Show' : 'Hide'} Workflows
          </Button>

          <Button
            size="sm"
            className="gap-2"
            onClick={saveWorkflow}
            disabled={registerWorkflow.isPending || updateWorkflow.isPending}
          >
            {(registerWorkflow.isPending || updateWorkflow.isPending) ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Saving...
              </>
            ) : (
              <>
                <Save className="h-4 w-4" />
                Save
              </>
            )}
          </Button>

          <Button
            variant="outline"
            size="sm"
            className="gap-2"
            onClick={validateCurrentWorkflow}
            disabled={nodes.length === 0 || isBackendValidating}
          >
            <CheckCircle className="h-4 w-4" />
            {isBackendValidating ? 'Validating...' : 'Validate'}
          </Button>

          <Button
            variant="outline"
            size="sm"
            className="gap-2"
            onClick={() => {
              setDryRunResult(null);
              setShowDryRunDialog(true);
            }}
            disabled={!canRunWorkflow}
          >
            <FlaskConical className="h-4 w-4" />
            Dry-Run
          </Button>

          <Button
            variant="ghost"
            size="sm"
            className="gap-2"
            onClick={() => setIsExecutionHistoryOpen(true)}
            disabled={!selectedWorkflowId}
          >
            <History className="h-4 w-4" />
            History
          </Button>

          <ScheduleStatusBadge
            schedule={primarySchedule}
            scheduleCount={workflowSchedules?.length}
            isLoading={false}
            onClick={() => setShowScheduleDialog(true)}
            disabled={!canRunWorkflow}
            compact={true}
          />

          <KeyboardShortcutsDialog />

          <AutoSaveIndicator
            isSaving={autoSave.isSaving}
            lastSaved={autoSave.lastSaved}
            error={autoSave.error}
            className="ml-4"
          />
        </div>
      </motion.div>

      {/* Workflow Name */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.05 }}
        className="flex items-center gap-3 mb-4 flex-shrink-0"
      >
        <label className="text-sm font-medium text-foreground">Workflow Name:</label>
        <Input
          value={workflowName}
          onChange={(e) => setWorkflowName(e.target.value)}
          className="max-w-sm"
        />
      </motion.div>

      {/* Execution Command Bar */}
      <ExecutionCommandBar
        workflowId={selectedWorkflowId || undefined}
        workflowName={workflowName}
        executionStatus={
          execution.isExecuting
            ? 'running'
            : execution.result?.success
            ? 'success'
            : execution.result && !execution.result.success
            ? 'error'
            : 'idle'
        }
        progress={
          execution.isExecuting && execution.currentStepId
            ? {
                currentStep: execution.completedSteps.size + 1,
                totalSteps: nodes.length,
                stepName: execution.currentStepId,
                percentage: Math.round(((execution.completedSteps.size + 1) / nodes.length) * 100),
              }
            : undefined
        }
        canExecute={canRunWorkflow}
        canStop={execution.isExecuting}
        canPause={false} // TODO: Enable when backend API is available
        canResume={false} // TODO: Enable when backend API is available
        onExecute={openExecuteDialog}
        onStop={stopCurrentWorkflow}
        onPause={pauseCurrentWorkflow}
        onResume={resumeCurrentWorkflow}
        validationErrors={validationErrors}
      />

      {/* Main Workflow Area */}
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15, delay: 0.1 }}
        className="flex gap-0 flex-1 min-h-0"
      >
        {/* Saved Workflows Panel */}
        <SavedWorkflowsPanel
          workflows={workflows}
          isLoading={loadingWorkflows}
          onLoadWorkflow={loadWorkflow}
          onDeleteWorkflow={handleDeleteWorkflow}
          onDuplicateWorkflow={handleDuplicateWorkflow}
          selectedWorkflowId={selectedWorkflowId}
          isCollapsed={isWorkflowsPanelCollapsed}
          onToggleCollapse={() => setIsWorkflowsPanelCollapsed(!isWorkflowsPanelCollapsed)}
        />

        {/* Tool Palette Sidebar */}
        <motion.div
          initial={false}
          animate={{
            width: isToolPaletteCollapsed ? '48px' : '256px',
          }}
          transition={{ duration: 0.2, ease: 'easeInOut' }}
          className="border-r border-border bg-background h-full relative flex-shrink-0"
        >
          {/* Collapse/Expand Button */}
          <Button
            variant="ghost"
            size="sm"
            className="absolute top-2 right-2 z-10 h-7 w-7 p-0"
            onClick={() => setIsToolPaletteCollapsed(!isToolPaletteCollapsed)}
            title={isToolPaletteCollapsed ? 'Expand Tool Palette' : 'Collapse Tool Palette'}
          >
            {isToolPaletteCollapsed ? (
              <ChevronRight className="h-4 w-4" />
            ) : (
              <ChevronLeft className="h-4 w-4" />
            )}
          </Button>

          {/* Node Palette - hide when collapsed */}
          {!isToolPaletteCollapsed && (
            <NodePalette onDragStart={handleDragStart} />
          )}
        </motion.div>

        {/* React Flow Canvas */}
        <div
          ref={reactFlowWrapper}
          className="flex-1 bg-background border border-border rounded-sm relative overflow-hidden h-full"
        >
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onInit={setReactFlowInstance}
            onDrop={onDrop}
            onDragOver={onDragOver}
            onNodeClick={onNodeClick}
            onNodeDoubleClick={onNodeDoubleClick}
            onPaneClick={onPaneClick}
            nodeTypes={nodeTypes}
            defaultViewport={{ x: 0, y: 0, zoom: 0.75 }}
            minZoom={0.25}
            maxZoom={1.5}
            fitView
            snapToGrid
            snapGrid={[16, 16]}
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={16}
              size={1.5}
              color="hsl(var(--border))"
              className="opacity-30"
            />
            <Controls
              className="bg-background border border-border rounded-sm"
              showInteractive={false}
            />
            <MiniMap
              className="bg-background-secondary border border-border rounded-sm"
              nodeColor={(node) => {
                if (node.data.executionStatus === 'executing') return 'hsl(var(--primary))';
                if (node.data.executionStatus === 'success') return 'hsl(var(--success))';
                if (node.data.executionStatus === 'error') return 'hsl(var(--error))';

                const isETLNode = isETLStepType(node.data.step_type);
                const stepConfig = isETLNode
                  ? getETLStepTypeConfig(node.data.step_type as any)
                  : getStepTypeConfig(node.data.step_type);
                return stepConfig.color.base;
              }}
              maskColor="rgba(0,0,0,0.05)"
              style={{ width: 180, height: 120 }}
              pannable
              zoomable
            />
            <Panel position="top-right" className="bg-background-secondary border border-border rounded-sm p-2 m-2">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>{nodes.length} nodes</span>
                <span>•</span>
                <span>{edges.length} connections</span>
              </div>
            </Panel>
          </ReactFlow>
        </div>

        {/* Step Configuration Dialog */}
        <StepConfigDialog
          open={isConfigDialogOpen}
          onOpenChange={setIsConfigDialogOpen}
          selectedNode={selectedNode}
          nodes={nodes}
          edges={edges}
          onUpdateNode={updateNodeData}
          onDeleteNode={deleteNode}
        />
      </motion.div>

      {/* Execution History Sheet */}
      <Sheet open={isExecutionHistoryOpen} onOpenChange={setIsExecutionHistoryOpen}>
        <SheetContent className="w-[600px] sm:max-w-[600px]">
          <SheetHeader>
            <SheetTitle>Execution History</SheetTitle>
            <SheetDescription>
              View past workflow executions and their results
            </SheetDescription>
          </SheetHeader>
          <div className="mt-6">
            <ExecutionHistoryViewer
              workflowId={selectedWorkflowId || undefined}
              executions={workflowExecutions}
              isLoading={loadingExecutions}
              onExecutionClick={(execution) => {
                setSelectedExecutionId(execution.execution_id);
                setShowExecutionDetailsDialog(true);
              }}
              onRefresh={() => refetchExecutions()}
            />
          </div>
        </SheetContent>
      </Sheet>

      <ExecuteWorkflowDialog
        open={showExecuteDialog}
        onOpenChange={setShowExecuteDialog}
        workflowName={workflowName}
        supportsInputlessExecution={supportsInputlessExecution}
        isExecuting={execution.isExecuting}
        onExecute={executeCurrentWorkflow}
      />

      {/* Dry-Run Dialog */}
      <Dialog open={showDryRunDialog} onOpenChange={setShowDryRunDialog}>
        <DialogContent className="max-w-3xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Dry-Run Workflow: {workflowName}</DialogTitle>
            <DialogDescription>
              Test the entire workflow without persisting results
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 mt-4">
            <div>
              <Label>
                {supportsInputlessExecution ? 'Optional Input Override (JSON)' : 'Input Data (JSON)'}
              </Label>
              <Textarea
                value={dryRunInput}
                onChange={(e) => setDryRunInput(e.target.value)}
                className="font-mono text-sm h-32 mt-2"
                placeholder={supportsInputlessExecution ? 'null' : DEFAULT_DRY_RUN_INPUT}
              />
              <p className="text-xs text-muted-foreground mt-1">
                {supportsInputlessExecution
                  ? 'This workflow can execute from configured source steps. Leave this as null unless you need to inject additional JSON input during dry-run.'
                  : 'Input data that will be passed to the workflow'}
              </p>
            </div>

            {dryRunResult && (
              <div>
                <Label>Dry-Run Results</Label>
                <div className={`p-4 rounded-md mt-2 border-2 ${
                  dryRunResult.success
                    ? 'bg-green-50 border-green-200'
                    : 'bg-red-50 border-red-200'
                }`}>
                  <div className="flex items-center gap-2 mb-3">
                    <span className="font-semibold text-base">
                      {dryRunResult.success ? '✅ All Steps Passed' : '❌ Workflow Failed'}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {dryRunResult.total_execution_time_ms}ms total
                    </span>
                  </div>

                  {dryRunResult.failed_step && (
                    <div className="mb-3 p-2 bg-red-100 border border-red-300 rounded text-sm text-red-800">
                      <strong>Failed at step:</strong> {dryRunResult.failed_step}
                    </div>
                  )}

                  <div className="space-y-3">
                    <div className="text-sm font-semibold">Step-by-Step Results:</div>
                    {dryRunResult.steps_executed?.map((step: any, idx: number) => (
                      <div
                        key={idx}
                        className={`p-3 rounded border ${
                          step.success
                            ? 'bg-white border-green-300'
                            : 'bg-red-50 border-red-300'
                        }`}
                      >
                        <div className="flex items-center justify-between mb-2">
                          <div className="flex items-center gap-2">
                            <span className="font-medium text-sm">
                              {step.success ? '✓' : '✗'} {step.step_id}
                            </span>
                            <span className="text-xs px-2 py-0.5 bg-muted rounded">
                              {step.step_type}
                            </span>
                          </div>
                          <span className="text-xs text-muted-foreground">
                            {step.execution_time_ms}ms
                          </span>
                        </div>

                        {step.error && (
                          <div className="mt-2 text-sm text-red-700">
                            <strong>Error:</strong> {step.error}
                          </div>
                        )}

                        {step.output && (
                          <div className="mt-2">
                            <div className="text-xs font-semibold mb-1">Output:</div>
                            <pre className="text-xs bg-muted p-2 rounded border overflow-auto max-h-32">
                              {JSON.stringify(step.output, null, 2)}
                            </pre>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>

                  {dryRunResult.final_output && (
                    <div className="mt-3 pt-3 border-t border-gray-300">
                      <div className="text-sm font-semibold mb-2">Final Output:</div>
                      <pre className="text-xs bg-white p-3 rounded border overflow-auto max-h-40">
                        {JSON.stringify(dryRunResult.final_output, null, 2)}
                      </pre>
                    </div>
                  )}
                </div>
              </div>
            )}

            <div className="flex gap-2">
              <Button
                onClick={handleDryRun}
                disabled={dryRun.isPending}
                className="flex-1"
              >
                {dryRun.isPending ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Running Dry-Run...
                  </>
                ) : (
                  <>
                    <FlaskConical className="h-4 w-4 mr-2" />
                    Run Dry-Run
                  </>
                )}
              </Button>
              <Button
                variant="outline"
                onClick={() => setShowDryRunDialog(false)}
              >
                Close
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Schedule Workflow Dialog */}
      {selectedWorkflowId && (
        <ScheduleWorkflowDialog
          open={showScheduleDialog}
          onOpenChange={setShowScheduleDialog}
          workflowId={selectedWorkflowId}
          workflowName={workflowName}
          supportsInputlessExecution={supportsInputlessExecution}
          onSchedule={handleSchedule}
          isScheduling={scheduleWorkflow.isPending}
        />
      )}

      {/* Execution Details Dialog */}
      <ExecutionDetailsDialog
        open={showExecutionDetailsDialog}
        onOpenChange={setShowExecutionDetailsDialog}
        executionId={selectedExecutionId}
        onRerun={(executionId) => {
          // Re-run workflow with same input
          toast.info('Re-run functionality coming soon');
        }}
      />

      {/* Execution Progress Overlay */}
      <ExecutionProgress
        isExecuting={execution.isExecuting}
        currentStepId={execution.currentStepId}
        result={execution.result}
        totalSteps={nodes.length}
        completedSteps={execution.completedSteps.size}
      />
    </div>
  );
}

export function WorkflowDesigner() {
  return (
    <ReactFlowProvider>
      <WorkflowDesignerInner />
    </ReactFlowProvider>
  );
}
