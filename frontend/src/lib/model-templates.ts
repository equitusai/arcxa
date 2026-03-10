/**
 * Model Registration Templates
 * Pre-configured templates for common ML serving frameworks
 */

import type { WizardFormData } from '@/components/models/RegisterModelWizard';

export interface ModelTemplate {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: 'aws' | 'kubernetes' | 'serverless' | 'custom';
  defaults: Partial<WizardFormData>;
  requiredFields: string[];
  documentation?: string;
}

export const MODEL_TEMPLATES: ModelTemplate[] = [
  {
    id: 'sagemaker-rest',
    name: 'AWS SageMaker Endpoint',
    description: 'AWS-hosted model with IAM authentication',
    icon: '📊',
    category: 'aws',
    defaults: {
      framework: 'sagemaker',
      endpoint: {
        protocol: 'https',
        url: 'https://runtime.sagemaker.{region}.amazonaws.com/endpoints/{endpoint-name}/invocations',
        timeout_ms: 60000,
        headers: {
          'Content-Type': 'application/json',
        },
      },
      circuitBreaker: {
        enabled: true,
        failureThreshold: 5,
        successThreshold: 2,
        timeoutMs: 30000,
      },
      retry: {
        enabled: true,
        maxAttempts: 3,
      },
      cache: {
        enabled: true,
        ttlSeconds: 300,
      },
    },
    requiredFields: ['name', 'endpoint.url'],
    documentation: 'https://docs.aws.amazon.com/sagemaker/latest/dg/realtime-endpoints.html',
  },
  {
    id: 'torchserve-http',
    name: 'TorchServe HTTP',
    description: 'PyTorch model on Kubernetes/Docker',
    icon: '🔥',
    category: 'kubernetes',
    defaults: {
      framework: 'torch',
      endpoint: {
        protocol: 'http',
        url: 'http://torchserve:8080/predictions/{model-name}',
        timeout_ms: 30000,
        headers: {
          'Content-Type': 'application/json',
        },
      },
      circuitBreaker: {
        enabled: true,
        failureThreshold: 5,
        successThreshold: 2,
        timeoutMs: 30000,
      },
      retry: {
        enabled: true,
        maxAttempts: 3,
      },
      cache: {
        enabled: true,
        ttlSeconds: 180,
      },
    },
    requiredFields: ['name', 'endpoint.url'],
    documentation: 'https://pytorch.org/serve/',
  },
  {
    id: 'tensorflow-serving-grpc',
    name: 'TensorFlow Serving gRPC',
    description: 'High-performance TF model with gRPC',
    icon: '🧠',
    category: 'kubernetes',
    defaults: {
      framework: 'tensorflow',
      endpoint: {
        protocol: 'grpc',
        url: 'grpc://tensorflow-serving:8500',
        timeout_ms: 10000,
        headers: {},
      },
      circuitBreaker: {
        enabled: true,
        failureThreshold: 10,
        successThreshold: 3,
        timeoutMs: 20000,
      },
      retry: {
        enabled: true,
        maxAttempts: 2,
      },
      cache: {
        enabled: true,
        ttlSeconds: 300,
      },
    },
    requiredFields: ['name', 'endpoint.url'],
    documentation: 'https://www.tensorflow.org/tfx/guide/serving',
  },
  {
    id: 'lambda-inference',
    name: 'AWS Lambda Function',
    description: 'Serverless model endpoint',
    icon: '⚡',
    category: 'serverless',
    defaults: {
      framework: 'custom',
      endpoint: {
        protocol: 'lambda',
        url: 'arn:aws:lambda:{region}:{account}:function:{function-name}',
        timeout_ms: 120000,
        headers: {},
      },
      circuitBreaker: {
        enabled: true,
        failureThreshold: 3,
        successThreshold: 2,
        timeoutMs: 60000,
      },
      retry: {
        enabled: true,
        maxAttempts: 2,
      },
      cache: {
        enabled: true,
        ttlSeconds: 600,
      },
    },
    requiredFields: ['name', 'endpoint.url'],
    documentation: 'https://docs.aws.amazon.com/lambda/',
  },
  {
    id: 'custom-rest',
    name: 'Custom REST API',
    description: 'Generic HTTP/JSON endpoint',
    icon: '🔗',
    category: 'custom',
    defaults: {
      framework: 'custom',
      endpoint: {
        protocol: 'http',
        url: '',
        timeout_ms: 30000,
        headers: {
          'Content-Type': 'application/json',
        },
      },
      circuitBreaker: {
        enabled: true,
        failureThreshold: 5,
        successThreshold: 2,
        timeoutMs: 30000,
      },
      retry: {
        enabled: true,
        maxAttempts: 3,
      },
      cache: {
        enabled: true,
        ttlSeconds: 300,
      },
    },
    requiredFields: ['name', 'endpoint.url', 'framework'],
  },
];

export function getTemplateById(id: string): ModelTemplate | undefined {
  return MODEL_TEMPLATES.find(t => t.id === id);
}

export function getTemplatesByCategory(category: string): ModelTemplate[] {
  return MODEL_TEMPLATES.filter(t => t.category === category);
}
