/**
 * Enterprise error handling utilities for Graphica API
 */

/**
 * Custom API error class with structured error information
 */
export class ApiClientError extends Error {
  constructor(
    message: string,
    public status: number,
    public code?: string,
    public details?: Record<string, any>
  ) {
    super(message);
    this.name = 'ApiClientError';
    Object.setPrototypeOf(this, ApiClientError.prototype);
  }

  /**
   * Check if error is a client error (4xx)
   */
  isClientError(): boolean {
    return this.status >= 400 && this.status < 500;
  }

  /**
   * Check if error is a server error (5xx)
   */
  isServerError(): boolean {
    return this.status >= 500 && this.status < 600;
  }

  /**
   * Check if error is a network error (no status)
   */
  isNetworkError(): boolean {
    return this.status === 0;
  }

  /**
   * Check if error is unauthorized (401)
   */
  isUnauthorized(): boolean {
    return this.status === 401;
  }

  /**
   * Check if error is forbidden (403)
   */
  isForbidden(): boolean {
    return this.status === 403;
  }

  /**
   * Check if error is not found (404)
   */
  isNotFound(): boolean {
    return this.status === 404;
  }

  /**
   * Check if error is rate limited (429)
   */
  isRateLimited(): boolean {
    return this.status === 429;
  }

  /**
   * Get user-friendly error message
   */
  getUserMessage(): string {
    if (this.isNetworkError()) {
      return 'Network error. Please check your connection and try again.';
    }

    if (this.isUnauthorized()) {
      return 'Your session has expired. Please log in again.';
    }

    if (this.isForbidden()) {
      return 'You do not have permission to perform this action.';
    }

    if (this.isNotFound()) {
      return 'The requested resource was not found.';
    }

    if (this.isRateLimited()) {
      return 'Too many requests. Please wait a moment and try again.';
    }

    if (this.isServerError()) {
      return 'Server error. Please try again later.';
    }

    return this.message || 'An unexpected error occurred.';
  }

  /**
   * Convert to JSON for logging
   */
  toJSON() {
    return {
      name: this.name,
      message: this.message,
      status: this.status,
      code: this.code,
      details: this.details,
    };
  }
}

/**
 * Error type definitions for common scenarios
 */
export const ErrorCodes = {
  // Authentication errors
  INVALID_CREDENTIALS: 'INVALID_CREDENTIALS',
  SESSION_EXPIRED: 'SESSION_EXPIRED',
  INSUFFICIENT_PERMISSIONS: 'INSUFFICIENT_PERMISSIONS',

  // Validation errors
  VALIDATION_ERROR: 'VALIDATION_ERROR',
  INVALID_INPUT: 'INVALID_INPUT',
  MISSING_REQUIRED_FIELD: 'MISSING_REQUIRED_FIELD',

  // Resource errors
  RESOURCE_NOT_FOUND: 'RESOURCE_NOT_FOUND',
  RESOURCE_ALREADY_EXISTS: 'RESOURCE_ALREADY_EXISTS',
  RESOURCE_CONFLICT: 'RESOURCE_CONFLICT',

  // Business logic errors
  WORKFLOW_EXECUTION_FAILED: 'WORKFLOW_EXECUTION_FAILED',
  MODEL_PREDICTION_FAILED: 'MODEL_PREDICTION_FAILED',
  RULE_EXECUTION_FAILED: 'RULE_EXECUTION_FAILED',

  // System errors
  INTERNAL_SERVER_ERROR: 'INTERNAL_SERVER_ERROR',
  SERVICE_UNAVAILABLE: 'SERVICE_UNAVAILABLE',
  RATE_LIMIT_EXCEEDED: 'RATE_LIMIT_EXCEEDED',
  TIMEOUT: 'TIMEOUT',

  // Network errors
  NETWORK_ERROR: 'NETWORK_ERROR',
  CONNECTION_ERROR: 'CONNECTION_ERROR',
} as const;

export type ErrorCode = typeof ErrorCodes[keyof typeof ErrorCodes];

/**
 * Check if error should be retried
 */
export function shouldRetryError(error: ApiClientError): boolean {
  // Retry network errors
  if (error.isNetworkError()) {
    return true;
  }

  // Retry server errors (5xx)
  if (error.isServerError()) {
    return true;
  }

  // Retry rate limit errors (with backoff)
  if (error.isRateLimited()) {
    return true;
  }

  // Don't retry client errors (4xx)
  return false;
}

/**
 * Get retry delay for error (exponential backoff)
 */
export function getRetryDelay(attemptNumber: number, baseDelay = 1000): number {
  // Exponential backoff: 1s, 2s, 4s, 8s, etc.
  return Math.min(baseDelay * Math.pow(2, attemptNumber - 1), 30000);
}

/**
 * Format error for logging
 */
export function formatErrorForLogging(error: unknown): Record<string, any> {
  if (error instanceof ApiClientError) {
    return error.toJSON();
  }

  if (error instanceof Error) {
    return {
      name: error.name,
      message: error.message,
      stack: error.stack,
    };
  }

  return {
    error: String(error),
  };
}
