/**
 * Authentication React Query hooks
 *
 * Provides hooks for login, logout, and user management
 */

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import { useAuthStore } from '@/stores/auth';
import * as authApi from '@/api/auth';
import { LoginRequest, SetupAdminRequest, CreateUserRequest } from '@/api/types';

/**
 * Login mutation hook
 *
 * Handles user login and sets authentication state
 *
 * @example
 * const login = useLogin();
 * login.mutate({ username: 'admin', password: 'password' });
 */
export function useLogin() {
  const setAuth = useAuthStore((state) => state.setAuth);

  return useMutation({
    mutationFn: (credentials: LoginRequest) => authApi.login(credentials),
    onSuccess: (data, variables) => {
      // Backend returns: { token, expires_at, role }
      // We need to create a User object
      const user = {
        id: variables.username, // Use username as ID since backend doesn't return user ID
        username: variables.username,
        role: (data.role.charAt(0).toUpperCase() + data.role.slice(1)) as any, // Capitalize role (admin -> Admin)
        created_at: new Date().toISOString(),
      };

      // Set authentication state
      setAuth(data.token, user);
      // Navigation is handled by the Login component
    },
    onError: (error: any) => {
      console.error('Login failed:', error);
      // Error will be handled by the component via mutation.error
    },
  });
}

/**
 * Logout mutation hook
 *
 * Clears authentication state and redirects to login
 *
 * @example
 * const logout = useLogout();
 * logout.mutate();
 */
export function useLogout() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const clearAuth = useAuthStore((state) => state.clearAuth);

  return useMutation({
    mutationFn: async () => {
      // Clear auth state
      clearAuth();

      // Clear all cached queries
      queryClient.clear();
    },
    onSuccess: () => {
      // Show success toast
      toast.success('Logged out successfully');

      // Navigate to login page
      navigate('/login');
    },
  });
}

/**
 * Setup admin mutation hook
 *
 * Handles initial admin setup (one-time operation)
 *
 * @example
 * const setupAdmin = useSetupAdmin();
 * setupAdmin.mutate({
 *   username: 'admin',
 *   password: 'securepassword',
 *   email: 'admin@example.com',
 *   setup_token: 'token123'
 * });
 */
export function useSetupAdmin() {
  const navigate = useNavigate();
  const setAuth = useAuthStore((state) => state.setAuth);

  return useMutation({
    mutationFn: (request: SetupAdminRequest) => authApi.setupAdmin(request),
    onSuccess: (data) => {
      // Set authentication state
      setAuth(data.token, {
        id: 'admin-' + Date.now(),
        username: 'admin',
        role: (data.role.charAt(0).toUpperCase() + data.role.slice(1)) as 'Admin' | 'Operator' | 'Viewer' | 'Service',
        created_at: new Date().toISOString()
      });

      // Show success toast
      toast.success('Admin setup complete');

      // Navigate to dashboard
      navigate('/');
    },
    onError: (error: any) => {
      console.error('Admin setup failed:', error);
      toast.error('Admin setup failed');
    },
  });
}

/**
 * Create user mutation hook (admin only)
 *
 * Creates a new user account
 *
 * @example
 * const createUser = useCreateUser();
 * createUser.mutate({
 *   username: 'john',
 *   password: 'password123',
 *   role: 'Operator',
 *   email: 'john@example.com'
 * });
 */
export function useCreateUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateUserRequest) => authApi.createUser(request),
    onSuccess: (data) => {
      // Invalidate user-related queries if we implement them
      queryClient.invalidateQueries({ queryKey: ['users'] });

      // Show success toast
      toast.success(`User "${data.username}" created successfully`);
    },
    onError: (error: any) => {
      console.error('User creation failed:', error);
      toast.error('Failed to create user');
    },
  });
}

/**
 * Generate API token mutation hook (admin only)
 *
 * Generates an API token for a user
 *
 * @example
 * const generateToken = useGenerateApiToken();
 * generateToken.mutate({ userId: 'user123', expiresIn: 86400 });
 */
export function useGenerateApiToken() {
  return useMutation({
    mutationFn: ({
      userId,
      expiresIn,
    }: {
      userId: string;
      expiresIn?: number;
    }) => authApi.generateApiToken(userId, expiresIn),
    onSuccess: () => {
      toast.success('API token generated successfully');
    },
    onError: (error: any) => {
      console.error('Token generation failed:', error);
      toast.error('Failed to generate API token');
    },
  });
}
