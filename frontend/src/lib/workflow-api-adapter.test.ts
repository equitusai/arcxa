import { describe, expect, it } from 'vitest';

import { adaptWorkflowDefinition, adaptWorkflowResponse } from './workflow-api-adapter';

describe('workflow api adapter', () => {
  it('normalizes backend validator rule objects into frontend-friendly rules', () => {
    const backendWorkflow = {
      workflow_id: 'oracle-demo-customer-feed-to-db2',
      definition: {
        steps: [
          {
            id: 'validate_customer_feed',
            step_type: 'data_validator',
            config: {
              rules: [
                {
                  field: 'CUSTOMER_CODE',
                  rule_type: 'NOT_NULL',
                  params: null,
                  severity: 'error',
                },
                {
                  field: 'EMAIL',
                  rule_type: {
                    REGEX: {
                      pattern: '^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$',
                    },
                  },
                  params: null,
                  severity: 'Error',
                },
              ],
              fail_on_error: true,
            },
          },
        ],
      },
    };

    const adaptedWorkflow = adaptWorkflowResponse(backendWorkflow);
    const rules = adaptedWorkflow.definition.steps[0].config.rules;

    expect(rules).toEqual([
      {
        field: 'CUSTOMER_CODE',
        rule_type: 'NOT_NULL',
        params: undefined,
        severity: 'error',
      },
      {
        field: 'EMAIL',
        rule_type: 'REGEX',
        params: {
          pattern: '^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$',
        },
        severity: 'error',
      },
    ]);
  });

  it('normalizes backend field transformer operations when they use backend casing', () => {
    const backendDefinition = {
      steps: [
        {
          id: 'normalize_customer_feed',
          step_type: 'field_transformer',
          config: {
            transformations: [
              {
                field: 'EMAIL',
                operations: [
                  { type: 'Trim' },
                  { type: 'Lower' },
                ],
              },
            ],
          },
        },
      ],
    };

    const adaptedDefinition = adaptWorkflowResponse(backendDefinition);
    const operations = adaptedDefinition.steps[0].config.transformations[0].operations;

    expect(operations).toEqual([
      { type: 'TRIM', params: undefined },
      { type: 'LOWER', params: undefined },
    ]);
  });

  it('converts frontend workflow definitions back into the backend validation contract', () => {
    const frontendDefinition = {
      steps: [
        {
          id: 'normalize_customer_feed',
          step_type: 'field_transformer',
          config: {
            transformations: [
              {
                field: 'EMAIL',
                operations: [
                  { type: 'TRIM' },
                  { type: 'LOWER' },
                ],
              },
            ],
          },
        },
        {
          id: 'validate_customer_feed',
          step_type: 'data_validator',
          config: {
            rules: [
              {
                field: 'CUSTOMER_CODE',
                rule_type: 'NOT_NULL',
                severity: 'error',
              },
              {
                field: 'EMAIL',
                rule_type: 'REGEX',
                params: {
                  pattern: '^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$',
                },
                severity: 'warning',
              },
            ],
            fail_on_error: true,
          },
        },
      ],
      fusion_threshold: 0,
      fallback: 'manual_review',
    };

    const adaptedDefinition = adaptWorkflowDefinition(frontendDefinition);

    expect(adaptedDefinition).toEqual({
      steps: [
        {
          id: 'normalize_customer_feed',
          step_type: 'field_transformer',
          config: {
            transformations: [
              {
                field: 'EMAIL',
                operations: [
                  { type: 'TRIM' },
                  { type: 'LOWER' },
                ],
              },
            ],
          },
        },
        {
          id: 'validate_customer_feed',
          step_type: 'data_validator',
          config: {
            rules: [
              {
                field: 'CUSTOMER_CODE',
                rule_type: 'NOT_NULL',
                severity: 'error',
              },
              {
                field: 'EMAIL',
                rule_type: {
                  REGEX: {
                    pattern: '^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$',
                  },
                },
                severity: 'warning',
              },
            ],
            fail_on_error: true,
          },
        },
      ],
      fusion_threshold: 0,
      fallback: 'manual_review',
    });
  });
});
