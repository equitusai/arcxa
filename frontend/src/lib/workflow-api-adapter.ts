/**
 * Workflow API Adapter
 * Converts frontend workflow node configurations to backend API format
 *
 * The frontend uses developer-friendly formats (UPPER_CASE enums, flat structures)
 * The backend expects specific formats (PascalCase, nested structures)
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

/**
 * Convert frontend operation type to backend format
 *
 * Frontend: 'TRIM', 'UPPER', 'LOWER', 'REGEX', 'CONCAT', 'SPLIT', 'CUSTOM'
 * Backend: 'Trim', 'Upper', 'Lower', 'Regex', 'Concat', 'Split', 'Custom'
 */
function convertOperationType(
  frontendType: 'TRIM' | 'LOWER' | 'UPPER' | 'REGEX' | 'CONCAT' | 'SPLIT' | 'CUSTOM'
): string {
  const mapping: Record<string, string> = {
    TRIM: 'Trim',
    LOWER: 'Lower',
    UPPER: 'Upper',
    REGEX: 'Regex',
    CONCAT: 'Concat',
    SPLIT: 'Split',
    CUSTOM: 'Custom',
  };

  return mapping[frontendType] || frontendType;
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

/**
 * Convert frontend rule type to backend format
 *
 * Frontend: 'NOT_NULL' | 'REGEX' | 'RANGE' | 'IN_SET' | 'UNIQUE' | 'CUSTOM'
 * Backend: { type: 'NotNull' | 'Regex' | 'Range' | 'InSet' | 'Unique' }
 */
function convertRuleType(
  frontendRuleType: ValidationRule['rule_type'],
  params?: Record<string, any>
): any {
  const typeMapping: Record<string, string> = {
    NOT_NULL: 'NotNull',
    REGEX: 'Regex',
    RANGE: 'Range',
    IN_SET: 'InSet',
    UNIQUE: 'Unique',
    CUSTOM: 'Custom', // Note: Backend doesn't support CUSTOM, will fail validation
  };

  const backendType = typeMapping[frontendRuleType] || frontendRuleType;

  // Build the rule_type object based on type
  const ruleType: any = {
    type: backendType,
  };

  // Add type-specific params
  if (params) {
    switch (frontendRuleType) {
      case 'REGEX':
        if (params.pattern) {
          ruleType.pattern = params.pattern;
        }
        break;

      case 'RANGE':
        if (params.min !== undefined) ruleType.min = params.min;
        if (params.max !== undefined) ruleType.max = params.max;
        break;

      case 'IN_SET':
        if (params.values) {
          ruleType.values = params.values;
        }
        break;

      default:
        // For other types, merge params directly
        Object.assign(ruleType, params);
    }
  }

  return ruleType;
}

/**
 * Convert frontend severity to backend format
 *
 * Frontend: 'error' | 'warning'
 * Backend: 'Error' | 'Warning' | 'Info'
 */
function convertSeverity(frontendSeverity: 'error' | 'warning'): string {
  const mapping: Record<string, string> = {
    error: 'Error',
    warning: 'Warning',
    info: 'Info',
  };

  return mapping[frontendSeverity] || 'Error';
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
 *     type: "Regex",
 *     pattern: "^[a-z]+@.*"
 *   },
 *   severity: "Error"
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
    NotNull: 'NOT_NULL',
    Regex: 'REGEX',
    Range: 'RANGE',
    InSet: 'IN_SET',
    Unique: 'UNIQUE',
    Custom: 'CUSTOM',
  };

  const frontendType = typeMapping[backendRuleType.type] || ('CUSTOM' as const);

  // Extract params based on type
  const params: Record<string, any> = {};

  switch (backendRuleType.type) {
    case 'Regex':
      if (backendRuleType.pattern) params.pattern = backendRuleType.pattern;
      break;

    case 'Range':
      if (backendRuleType.min !== undefined) params.min = backendRuleType.min;
      if (backendRuleType.max !== undefined) params.max = backendRuleType.max;
      break;

    case 'InSet':
      if (backendRuleType.values) params.values = backendRuleType.values;
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
    Error: 'error',
    Warning: 'warning',
    Info: 'warning', // Map Info to warning for frontend
  };

  return mapping[backendSeverity] || 'error';
}

/**
 * Adapt backend workflow response to frontend format
 */
export function adaptWorkflowResponse(backendWorkflow: any): any {
  // For now, mostly pass through
  // We may need to convert operation types and rule types back
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
