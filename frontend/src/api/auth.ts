/**
 * Authentication API functions
 *
 * Handles user authentication, token management, and user creation
 *
 * Note: Auth endpoints are at root level (not under /api/v1)
 */

import { apiClient } from './client';
import {
  LoginRequest,
  LoginResponse,
  SetupAdminRequest,
  CreateUserRequest,
  CreateUserResponse,
} from './types';

// Auth endpoints are at http://localhost:8080/auth/*, not /api/v1/auth/*
const AUTH_BASE_URL = import.meta.env.VITE_API_BASE_URL?.replace('/api/v1', '') || 'http://localhost:8080';

/**
 * Login user and get JWT token
 *
 * @param credentials - Username and password
 * @returns Access token and user information
 */
export async function login(credentials: LoginRequest): Promise<LoginResponse> {
  const response = await apiClient.post<LoginResponse>(
    `${AUTH_BASE_URL}/auth/login`,
    credentials
  );
  return response.data;
}

/**
 * Initial admin setup (one-time operation)
 *
 * @param request - Admin setup credentials with setup token
 * @returns Access token and admin user information
 */
export async function setupAdmin(request: SetupAdminRequest): Promise<LoginResponse> {
  const response = await apiClient.post<LoginResponse>(
    `${AUTH_BASE_URL}/auth/setup`,
    request
  );
  return response.data;
}

/**
 * Generate API token (admin only)
 *
 * @param userId - User ID to generate token for
 * @param expiresIn - Token expiration in seconds (optional)
 * @returns Generated API token
 */
export async function generateApiToken(
  userId: string,
  expiresIn?: number
): Promise<{ token: string; expires_at: string }> {
  const response = await apiClient.post<{ token: string; expires_at: string }>(
    `${AUTH_BASE_URL}/auth/token`,
    { user_id: userId, expires_in: expiresIn }
  );
  return response.data;
}

/**
 * Create new user (admin only)
 *
 * @param request - User creation request with credentials and role
 * @returns Created user information
 */
export async function createUser(request: CreateUserRequest): Promise<CreateUserResponse> {
  const response = await apiClient.post<CreateUserResponse>(
    `${AUTH_BASE_URL}/auth/users`,
    request
  );
  return response.data;
}

/**
 * Logout (client-side only - clear auth state)
 *
 * Note: Backend uses stateless JWT, so logout is handled client-side
 * by clearing the token from memory
 */
export function logout(): void {
  // Token is cleared by calling useAuthStore.clearAuth()
  // This function is provided for symmetry with login
}
