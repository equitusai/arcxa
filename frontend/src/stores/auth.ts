/**
 * Authentication store for Graphica frontend
 *
 * Manages authentication state including JWT tokens and user information.
 * IMPORTANT: Tokens are stored in memory only (not localStorage) for security.
 */

import { create } from 'zustand';

export type UserRole = 'Viewer' | 'Operator' | 'Admin' | 'Service';

export interface User {
  id: string;
  username: string;
  role: UserRole;
  created_at: string;
  email?: string;
  full_name?: string;
}

export interface AuthState {
  // Authentication state
  token: string | null;
  user: User | null;
  isAuthenticated: boolean;

  // Auth actions
  setAuth: (token: string, user: User) => void;
  clearAuth: () => void;
  updateUser: (user: Partial<User>) => void;

  // Token management
  getAuthHeader: () => string | null;
}

/**
 * Auth store with in-memory token storage
 *
 * Security considerations:
 * - Tokens are NOT persisted to localStorage (XSS protection)
 * - Tokens will be lost on page refresh (requires re-login)
 * - For production, consider implementing refresh tokens with HTTP-only cookies
 */
export const useAuthStore = create<AuthState>((set, get) => ({
  // Initial state
  token: null,
  user: null,
  isAuthenticated: false,

  // Set authentication (after login)
  setAuth: (token, user) => {
    set({
      token,
      user,
      isAuthenticated: true,
    });
  },

  // Clear authentication (logout)
  clearAuth: () => {
    set({
      token: null,
      user: null,
      isAuthenticated: false,
    });
  },

  // Update user information
  updateUser: (updates) => {
    const currentUser = get().user;
    if (currentUser) {
      set({
        user: {
          ...currentUser,
          ...updates,
        },
      });
    }
  },

  // Get Authorization header for API requests
  getAuthHeader: () => {
    const token = get().token;
    return token ? `Bearer ${token}` : null;
  },
}));

/**
 * Helper functions for role-based access control
 */
export const hasPermission = (user: User | null, requiredRole: UserRole): boolean => {
  if (!user) return false;

  const roleHierarchy: Record<UserRole, number> = {
    Viewer: 1,
    Operator: 2,
    Admin: 3,
    Service: 3, // Service accounts have same level as Admin
  };

  return roleHierarchy[user.role] >= roleHierarchy[requiredRole];
};

export const canView = (user: User | null): boolean => hasPermission(user, 'Viewer');
export const canWrite = (user: User | null): boolean => hasPermission(user, 'Operator');
export const isAdmin = (user: User | null): boolean => user?.role === 'Admin';
export const isService = (user: User | null): boolean => user?.role === 'Service';
