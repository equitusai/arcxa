/**
 * Enterprise-grade Axios HTTP client for Graphica API
 *
 * Features:
 * - Request/response interceptors
 * - Automatic retry with exponential backoff
 * - Request timeout configuration
 * - Auth token injection
 * - Error response normalization
 * - Request/response logging in development
 */

import axios, { AxiosError, AxiosRequestConfig, InternalAxiosRequestConfig } from 'axios';
import { useAuthStore } from '@/stores/auth';
import { ApiClientError } from '@/utils/errors';

// API base configuration
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:8080/api/v1';
const API_TIMEOUT = parseInt(import.meta.env.VITE_API_TIMEOUT || '30000');
const MAX_RETRIES = 3;
const INITIAL_RETRY_DELAY = 1000; // 1 second

/**
 * Create axios instance with base configuration
 */
export const apiClient = axios.create({
  baseURL: API_BASE_URL,
  timeout: API_TIMEOUT,
  headers: {
    'Content-Type': 'application/json',
  },
});

/**
 * Request interceptor: Inject auth token
 */
apiClient.interceptors.request.use(
  (config: InternalAxiosRequestConfig) => {
    // Get auth token from store
    const token = useAuthStore.getState().token;

    // Inject Bearer token if available
    if (token && config.headers) {
      config.headers.Authorization = `Bearer ${token}`;
    }

    // IMPORTANT: Remove Content-Type for FormData to allow browser to set boundary
    if (config.data instanceof FormData && config.headers) {
      delete config.headers['Content-Type'];
    }

    // Log request in development
    if (import.meta.env.DEV) {
      console.log(`[API Request] ${config.method?.toUpperCase()} ${config.url}`, {
        params: config.params,
        data: config.data instanceof FormData ? 'FormData' : config.data,
      });
    }

    return config;
  },
  (error) => {
    console.error('[API Request Error]', error);
    return Promise.reject(error);
  }
);

/**
 * Response interceptor: Handle errors and log responses
 */
apiClient.interceptors.response.use(
  (response) => {
    // Log successful responses in development
    if (import.meta.env.DEV) {
      console.log(`[API Response] ${response.config.method?.toUpperCase()} ${response.config.url}`, {
        status: response.status,
        data: response.data,
      });
    }

    return response;
  },
  async (error: AxiosError) => {
    const config = error.config as AxiosRequestConfig & { _retry?: number };

    // Log error in development
    if (import.meta.env.DEV) {
      console.error(`[API Error] ${config?.method?.toUpperCase()} ${config?.url}`, {
        status: error.response?.status,
        data: error.response?.data,
        message: error.message,
      });
    }

    // Handle 401 Unauthorized - clear auth and redirect to login
    if (error.response?.status === 401) {
      const currentPath = window.location.pathname;

      // Only clear auth and redirect if not already on login page
      if (currentPath !== '/login') {
        useAuthStore.getState().clearAuth();
        window.location.href = '/login';
      }

      return Promise.reject(error);
    }

    // Retry logic for 5xx errors and network errors
    const shouldRetry = (
      // Network errors (no response)
      !error.response ||
      // Server errors (5xx)
      (error.response.status >= 500 && error.response.status < 600)
    );

    if (shouldRetry && config && (!config._retry || config._retry < MAX_RETRIES)) {
      config._retry = (config._retry || 0) + 1;

      // Exponential backoff delay
      const delay = INITIAL_RETRY_DELAY * Math.pow(2, config._retry - 1);

      console.log(`[API Retry] Attempt ${config._retry}/${MAX_RETRIES} after ${delay}ms`);

      await new Promise(resolve => setTimeout(resolve, delay));

      return apiClient(config);
    }

    return Promise.reject(error);
  }
);

/**
 * Generic request wrapper with error handling
 */
export async function request<T = any>(config: AxiosRequestConfig): Promise<T> {
  try {
    const response = await apiClient.request<T>(config);
    return response.data;
  } catch (error) {
    throw normalizeError(error);
  }
}

/**
 * Normalize axios errors into ApiClientError
 */
function normalizeError(error: unknown): ApiClientError {
  if (axios.isAxiosError(error)) {
    const status = error.response?.status || 0;
    const message =
      error.response?.data?.message ||
      error.response?.data?.error ||
      error.message ||
      'An unknown error occurred';
    const code = error.response?.data?.code;
    const details = error.response?.data?.details;

    return new ApiClientError(message, status, code, details);
  }

  if (error instanceof Error) {
    return new ApiClientError(error.message, 0);
  }

  return new ApiClientError('An unknown error occurred', 0);
}

/**
 * Convenience methods for common HTTP operations
 */
export const api = {
  get: <T = any>(url: string, config?: AxiosRequestConfig) =>
    request<T>({ ...config, method: 'GET', url }),

  post: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) =>
    request<T>({ ...config, method: 'POST', url, data }),

  put: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) =>
    request<T>({ ...config, method: 'PUT', url, data }),

  delete: <T = any>(url: string, config?: AxiosRequestConfig) =>
    request<T>({ ...config, method: 'DELETE', url }),

  patch: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) =>
    request<T>({ ...config, method: 'PATCH', url, data }),
};

/**
 * Create API client for specific service with custom config
 */
export function createServiceClient(baseURL: string, config?: AxiosRequestConfig) {
  return axios.create({
    ...config,
    baseURL,
    timeout: config?.timeout || API_TIMEOUT,
  });
}

/**
 * Default export for convenience
 */
export default api;
