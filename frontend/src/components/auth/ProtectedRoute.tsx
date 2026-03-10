/**
 * Protected Route Component
 *
 * Architectural pattern: Higher-Order Component for route protection
 * Responsibilities:
 * - Check authentication state
 * - Redirect unauthenticated users to login
 * - Optionally check user permissions/roles
 *
 * Separation of concerns:
 * - Auth state management: Zustand store
 * - Routing: React Router
 * - UI: This component only handles logic, no UI
 */

import { Navigate, useLocation } from 'react-router-dom';
import { useAuthStore, type UserRole } from '@/stores/auth';

interface ProtectedRouteProps {
  children: React.ReactNode;
  requiredRole?: UserRole;
}

/**
 * ProtectedRoute wrapper component
 *
 * @param children - Child components to render if authenticated
 * @param requiredRole - Optional required role for access (default: any authenticated user)
 *
 * @example
 * <Route path="/admin" element={<ProtectedRoute requiredRole="Admin"><AdminPage /></ProtectedRoute>} />
 */
export function ProtectedRoute({ children, requiredRole }: ProtectedRouteProps) {
  const { isAuthenticated, user } = useAuthStore();
  const location = useLocation();

  // Not authenticated - redirect to login
  if (!isAuthenticated) {
    // Save the attempted location for redirect after login
    return <Navigate to="/login" state={{ from: location }} replace />;
  }

  // Check role-based access if required
  if (requiredRole && user) {
    const roleHierarchy: Record<UserRole, number> = {
      Viewer: 1,
      Operator: 2,
      Admin: 3,
      Service: 3,
    };

    const hasPermission = roleHierarchy[user.role] >= roleHierarchy[requiredRole];

    if (!hasPermission) {
      // User is authenticated but doesn't have required role
      return <Navigate to="/unauthorized" replace />;
    }
  }

  // Authenticated and authorized - render children
  return <>{children}</>;
}
