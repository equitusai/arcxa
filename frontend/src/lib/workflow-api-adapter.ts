/**
 * Workflow API Adapter
 * Converts frontend workflow state into the coordinator's current API contract.
 *
 * The frontend normalizes workflow details into a friendlier editing shape, but
 * outbound requests still need to preserve the backend's current step config
 * conventions.
 */

import type {
  FieldTransformerConfig,
  FieldTransformation,
  DataValidatorConfig,
  ValidationRule,
} from './workflow-etl-config';

// ============================================================================
// Field Transformer Adapters
// ============================================================================

function convertOperationType(
  frontendType: 'TRIM' | 'LOWER' | 'UPPER' | 'ROUND' | 'REGEX' | 'CONCAT' | 'SPLIT' | 'CUSTOM'
): string {
  return frontendType;
}

/**
 * Convert frontend operation to backend format
 */
function convertOperation(operation: FieldTransformation['operations'][0]): any {
  const backendOp: any = {
    type: convertOperationType(operation.type),
  };

  // Add params if they exist
  if (operation.params) {
    Object.assign(backendOp, operation.params);
  }

  return backendOp;
}

/**
 * Convert frontend FieldTransformerConfig to backend format
 *
 * Frontend:
 * {
 *   transformations: [
 *     {
 *       field: "email",
 *       operations: [
 *         { type: "TRIM" },
 *         { type: "LOWER" }
 *       ]
 *     }
 *   ]
 * }
 *
 * Backend:
 * {
 *   transformations: [
 *     {
 *       field: "email",
 *       operations: [
 *         { type: "Trim" },
 *         { type: "Lower" }
 *       ]
 *     }
 *   ]
 * }
 */
export function adaptFieldTransformerConfig(
  config: FieldTransformerConfig
): FieldTransformerConfig {
  return {
    transformations: config.transformations.map((transformation) => ({
      field: transformation.field,
      operations: transformation.operations.map(convertOperation),
    })),
  };
}

// ============================================================================
// Data Validator Adapters
// ============================================================================

function convertRuleType(
  frontendRuleType: ValidationRule['rule_type'],
  params?: Record<string, any>
): any {
  const hasParams = Boolean(params && Object.keys(params).length > 0);

  switch (frontendRuleType) {
    case 'REGEX':
      return hasParams
        ? {
            REGEX: {
              ...(params?.pattern ? { pattern: params.pattern } : {}),
            },
          }
        : 'REGEX';

    case 'RANGE':
      if (!hasParams) {
        return 'RANGE';
      }

      return {
        RANGE: {
          ...(params?.min !== undefined ? { min: params.min } : {}),
          ...(params?.max !== undefined ? { max: params.max } : {}),
          ...(params?.inclusive !== undefined ? { inclusive: params.inclusive } : {}),
        },
      };

    case 'IN_SET':
      if (!hasParams) {
        return 'IN_SET';
      }

      return {
        IN_SET: {
          ...(Array.isArray(params?.values)
            ? { values: params.values }
            : Array.isArray(params?.allowed_values)
              ? { values: params.allowed_values }
              : {}),
          ...(params?.case_sensitive !== undefined
            ? { case_sensitive: params.case_sensitive }
            : {}),
        },
      };

    case 'NOT_NULL':
    case 'UNIQUE':
    case 'CUSTOM':
    default:
      return frontendRuleType;
  }
}

function convertSeverity(frontendSeverity: 'error' | 'warning'): string {
  return frontendSeverity;
}

/**
 * Convert frontend validation rule to backend format
 *
 * Frontend:
 * {
 *   field: "email",
 *   rule_type: "REGEX",
 *   params: { pattern: "^[a-z]+@.*" },
 *   severity: "error"
 * }
 *
 * Backend:
 * {
 *   field: "email",
 *   rule_type: {
 *     REGEX: {
 *       pattern: "^[a-z]+@.*"
 *     }
 *   },
 *   severity: "error"
 * }
 */
function convertValidationRule(rule: ValidationRule): any {
  return {
    field: rule.field,
    rule_type: convertRuleType(rule.rule_type, rule.params),
    severity: convertSeverity(rule.severity),
  };
}

/**
 * Convert frontend DataValidatorConfig to backend format
 */
export function adaptDataValidatorConfig(config: DataValidatorConfig): any {
  return {
    rules: config.rules.map(convertValidationRule),
    fail_on_error: config.fail_on_error,
  };
}

function adaptWorkflowStepConfigForBackend(stepType: string | undefined, config: any): any {
  if (!config || typeof config !== 'object') {
    return config;
  }

  if (stepType === 'field_transformer') {
    return adaptFieldTransformerConfig(config);
  }

  if (stepType === 'data_validator') {
    return adaptDataValidatorConfig(config);
  }

  return config;
}

export function adaptWorkflowStepForBackend(step: any): any {
  if (!step || typeof step !== 'object') {
    return step;
  }

  return {
    ...step,
    config: adaptWorkflowStepConfigForBackend(step.step_type, step.config),
  };
}

// ============================================================================
// Workflow Node Adapter
// ============================================================================

/**
 * Convert frontend workflow node to backend format
 *
 * This is the main adapter function that handles all node types
 */
export function adaptWorkflowNode(node: any): any {
  const adaptedNode = { ...node };

  // Adapt config based on node type
  if (node.data?.step_type === 'field_transformer' && node.data?.config) {
    adaptedNode.data = {
      ...node.data,
      config: adaptFieldTransformerConfig(node.data.config),
    };
  } else if (node.data?.step_type === 'data_validator' && node.data?.config) {
    adaptedNode.data = {
      ...node.data,
      config: adaptDataValidatorConfig(node.data.config),
    };
  }

  return adaptedNode;
}

/**
 * Convert entire workflow definition to backend format
 */
export function adaptWorkflowDefinition(workflow: any): any {
  if (!workflow || typeof workflow !== 'object') {
    return workflow;
  }

  if (Array.isArray(workflow.steps)) {
    return {
      ...workflow,
      steps: workflow.steps.map(adaptWorkflowStepForBackend),
    };
  }

  if (workflow.definition && Array.isArray(workflow.definition.steps)) {
    return {
      ...workflow,
      definition: adaptWorkflowDefinition(workflow.definition),
    };
  }

  return {
    ...workflow,
    nodes: workflow.nodes?.map(adaptWorkflowNode) || [],
  };
}

// ============================================================================
// Response Adapters (Backend → Frontend)
// ============================================================================

/**
 * Convert backend operation type to frontend format
 */
function convertBackendOperationType(backendType: string): string {
  const mapping: Record<string, string> = {
    ROUND: 'ROUND',
    Trim: 'TRIM',
    Lower: 'LOWER',
    Upper: 'UPPER',
    Regex: 'REGEX',
    Concat: 'CONCAT',
    Split: 'SPLIT',
    Custom: 'CUSTOM',
  };

  return mapping[backendType] || backendType;
}

/**
 * Convert backend rule type to frontend format
 */
function convertBackendRuleType(backendRuleType: any): {
  rule_type: ValidationRule['rule_type'];
  params?: Record<string, any>;
} {
  const typeMapping: Record<string, ValidationRule['rule_type']> = {
    NOT_NULL: 'NOT_NULL',
    REGEX: 'REGEX',
    RANGE: 'RANGE',
    IN_SET: 'IN_SET',
    UNIQUE: 'UNIQUE',
    CUSTOM: 'CUSTOM',
    NotNull: 'NOT_NULL',
    Regex: 'REGEX',
    Range: 'RANGE',
    InSet: 'IN_SET',
    Unique: 'UNIQUE',
    Custom: 'CUSTOM',
  };

  if (typeof backendRuleType === 'string') {
    return {
      rule_type: typeMapping[backendRuleType] || 'CUSTOM',
    };
  }

  if (!backendRuleType || typeof backendRuleType !== 'object') {
    return {
      rule_type: 'CUSTOM',
    };
  }

  let backendType: string | undefined;
  let backendParams: Record<string, any> = {};

  if (typeof backendRuleType.type === 'string') {
    backendType = backendRuleType.type;
    backendParams = backendRuleType;
  } else {
    const entries = Object.entries(backendRuleType);

    if (entries.length === 1) {
      const [entryType, entryParams] = entries[0];
      backendType = entryType;
      backendParams =
        entryParams && typeof entryParams === 'object'
          ? (entryParams as Record<string, any>)
          : {};
    }
  }

  const frontendType = backendType ? typeMapping[backendType] || ('CUSTOM' as const) : 'CUSTOM';

  // Extract params based on type
  const params: Record<string, any> = {};

  switch (frontendType) {
    case 'REGEX':
      if (backendParams.pattern) params.pattern = backendParams.pattern;
      break;

    case 'RANGE':
      if (backendParams.min !== undefined) params.min = backendParams.min;
      if (backendParams.max !== undefined) params.max = backendParams.max;
      break;

    case 'IN_SET':
      if (backendParams.values) params.values = backendParams.values;
      if (backendParams.allowed_values) params.values = backendParams.allowed_values;
      break;
  }

  return {
    rule_type: frontendType,
    params: Object.keys(params).length > 0 ? params : undefined,
  };
}

/**
 * Convert backend severity to frontend format
 */
function convertBackendSeverity(backendSeverity: string): 'error' | 'warning' {
  const mapping: Record<string, 'error' | 'warning'> = {
    error: 'error',
    warning: 'warning',
    info: 'warning',
    Error: 'error',
    Warning: 'warning',
    Info: 'warning', // Map Info to warning for frontend
  };

  return mapping[backendSeverity] || 'error';
}

function adaptBackendOperation(operation: any): FieldTransformation['operations'][0] {
  if (!operation || typeof operation !== 'object') {
    return {
      type: 'CUSTOM',
    } as FieldTransformation['operations'][0];
  }

  const { type, ...params } = operation;

  return {
    type: convertBackendOperationType(type) as FieldTransformation['operations'][0]['type'],
    params: Object.keys(params).length > 0 ? params : undefined,
  };
}

function adaptFieldTransformerResponseConfig(config: any): FieldTransformerConfig {
  return {
    transformations: (config?.transformations || []).map((transformation: any) => ({
      field: transformation.field,
      operations: (transformation.operations || []).map(adaptBackendOperation),
    })),
  };
}

function adaptValidationRule(rule: any): ValidationRule {
  const adaptedRuleType = convertBackendRuleType(rule?.rule_type);

  return {
    field: rule?.field || '',
    rule_type: adaptedRuleType.rule_type,
    params: adaptedRuleType.params ?? (rule?.params ?? undefined),
    severity: convertBackendSeverity(rule?.severity || 'Error'),
  };
}

function adaptDataValidatorResponseConfig(config: any): DataValidatorConfig {
  return {
    rules: (config?.rules || []).map(adaptValidationRule),
    fail_on_error: Boolean(config?.fail_on_error),
  };
}

function adaptWorkflowStepResponse(step: any): any {
  if (!step || typeof step !== 'object') {
    return step;
  }

  if (step.step_type === 'field_transformer') {
    return {
      ...step,
      config: adaptFieldTransformerResponseConfig(step.config),
    };
  }

  if (step.step_type === 'data_validator') {
    return {
      ...step,
      config: adaptDataValidatorResponseConfig(step.config),
    };
  }

  return step;
}

/**
 * Adapt backend workflow response to frontend format
 */
export function adaptWorkflowResponse(backendWorkflow: any): any {
  if (!backendWorkflow || typeof backendWorkflow !== 'object') {
    return backendWorkflow;
  }

  if (Array.isArray(backendWorkflow.steps)) {
    return {
      ...backendWorkflow,
      steps: backendWorkflow.steps.map(adaptWorkflowStepResponse),
    };
  }

  if (backendWorkflow.definition && Array.isArray(backendWorkflow.definition.steps)) {
    return {
      ...backendWorkflow,
      definition: adaptWorkflowResponse(backendWorkflow.definition),
    };
  }

  return backendWorkflow;
}

// ============================================================================
// Validation Helpers
// ============================================================================

/**
 * Validate that a field transformer config can be sent to backend
 */
export function validateFieldTransformerConfig(config: FieldTransformerConfig): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!config.transformations || config.transformations.length === 0) {
    errors.push('At least one transformation is required');
  }

  config.transformations?.forEach((transformation, idx) => {
    if (!transformation.field) {
      errors.push(`Transformation ${idx + 1}: field is required`);
    }

    if (!transformation.operations || transformation.operations.length === 0) {
      errors.push(`Transformation ${idx + 1}: at least one operation is required`);
    }

    transformation.operations?.forEach((operation, opIdx) => {
      // Validate operation-specific params
      switch (operation.type) {
        case 'REGEX':
          if (!operation.params?.pattern) {
            errors.push(
              `Transformation ${idx + 1}, operation ${opIdx + 1}: REGEX requires 'pattern' param`
            );
          }
          break;

        case 'CONCAT':
          if (!operation.params?.fields || !Array.isArray(operation.params.fields)) {
            errors.push(
              `Transformation ${idx + 1}, operation ${opIdx + 1}: CONCAT requires 'fields' array param`
            );
          }
          break;

        case 'SPLIT':
          if (!operation.params?.delimiter) {
            errors.push(
              `Transformation ${idx + 1}, operation ${opIdx + 1}: SPLIT requires 'delimiter' param`
            );
          }
          break;
      }
    });
  });

  return {
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Validate that a data validator config can be sent to backend
 */
export function validateDataValidatorConfig(config: DataValidatorConfig): {
  valid: boolean;
  errors: string[];
  warnings: string[];
} {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!config.rules || config.rules.length === 0) {
    errors.push('At least one validation rule is required');
  }

  config.rules?.forEach((rule, idx) => {
    if (!rule.field) {
      errors.push(`Rule ${idx + 1}: field is required`);
    }

    // Warn about CUSTOM rule type (not supported by backend)
    if (rule.rule_type === 'CUSTOM') {
      warnings.push(
        `Rule ${idx + 1}: CUSTOM rule type is not supported by backend. Consider using a different rule type.`
      );
    }

    // Validate rule-specific params
    switch (rule.rule_type) {
      case 'REGEX':
        if (!rule.params?.pattern) {
          errors.push(`Rule ${idx + 1}: REGEX requires 'pattern' param`);
        }
        break;

      case 'RANGE':
        if (rule.params?.min === undefined && rule.params?.max === undefined) {
          errors.push(`Rule ${idx + 1}: RANGE requires at least 'min' or 'max' param`);
        }
        break;

      case 'IN_SET':
        if (!rule.params?.values || !Array.isArray(rule.params.values)) {
          errors.push(`Rule ${idx + 1}: IN_SET requires 'values' array param`);
        }
        break;
    }
  });

  return {
    valid: errors.length === 0,
    errors,
    warnings,
  };
}
