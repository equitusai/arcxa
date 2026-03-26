/**
 * Login Page
 *
 * Architectural pattern: Container Component
 * Responsibilities:
 * - Form state management (controlled components)
 * - Form validation
 * - Authentication via useLogin hook
 * - Error display
 * - Loading states
 *
 * Separation of concerns:
 * - Business logic: useLogin hook
 * - State management: React useState
 * - API calls: Abstracted in hooks
 * - UI: Presentation components (Button, Input, Card, Alert)
 */

import React, { useState } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { toast } from 'sonner';
import { useLogin } from '@/hooks/useAuth';
import { useAuthStore } from '@/stores/auth';
import { AppLegalFooter } from '@/components/AppLegalFooter';
import { BrandMark } from '@/components/BrandMark';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Loader2, AlertCircle } from 'lucide-react';
import { ApiClientError } from '@/utils/errors';
import { APP_NAME, APP_TAGLINE } from '@/lib/branding';

export function Login() {
  // Form state
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [formErrors, setFormErrors] = useState<{ username?: string; password?: string }>({});

  // Hooks
  const navigate = useNavigate();
  const location = useLocation();
  const loginMutation = useLogin();
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);

  // Get the redirect path (where user was trying to go before being redirected to login)
  const from = (location.state as any)?.from?.pathname || '/';

  // Auto-redirect if already authenticated
  React.useEffect(() => {
    if (isAuthenticated) {
      navigate(from, { replace: true });
    }
  }, [isAuthenticated, navigate, from]);

  /**
   * Validate form inputs
   */
  const validateForm = (): boolean => {
    const errors: { username?: string; password?: string } = {};

    if (!username.trim()) {
      errors.username = 'Username is required';
    }

    if (!password) {
      errors.password = 'Password is required';
    } else if (password.length < 6) {
      errors.password = 'Password must be at least 6 characters';
    }

    setFormErrors(errors);
    return Object.keys(errors).length === 0;
  };

  /**
   * Handle form submission
   */
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    // Clear previous errors
    setFormErrors({});

    // Validate
    if (!validateForm()) {
      return;
    }

    // Execute login mutation
    loginMutation.mutate(
      { username, password },
      {
        onSuccess: (data) => {
          toast.success('Login successful');
          // Auth state is set by useLogin hook
          // Navigation will happen via useEffect when isAuthenticated changes
        },
        onError: (error) => {
          if (error instanceof ApiClientError) {
            if (error.isUnauthorized() || error.status === 401) {
              toast.error('Invalid username or password');
            } else {
              toast.error(error.getUserMessage());
            }
          } else {
            toast.error('Login failed. Please try again.');
          }
        },
      }
    );
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background-secondary px-4">
      <div className="w-full max-w-md space-y-6">
        <Card className="w-full glass-morphism border-border">
          <CardHeader className="space-y-1 text-center border-b-2 border-border pb-6">
            <div className="mb-4">
              <BrandMark centered subtitle="Secure Access" />
            </div>

            <CardTitle className="text-2xl font-semibold text-foreground">
              Sign in to {APP_NAME}
            </CardTitle>
            <CardDescription className="text-foreground-muted">
              {APP_TAGLINE}
            </CardDescription>
          </CardHeader>

          <CardContent className="pt-6">
            <form onSubmit={handleSubmit} className="space-y-4">
              {/* Global error message */}
              {loginMutation.isError && (
                <Alert variant="destructive" className="border-error bg-error/10">
                  <AlertCircle className="h-4 w-4" />
                  <AlertDescription className="text-sm">
                    {loginMutation.error instanceof ApiClientError
                      ? loginMutation.error.getUserMessage()
                      : 'An error occurred during login'}
                  </AlertDescription>
                </Alert>
              )}

              {/* Username field */}
              <div className="space-y-2">
                <label htmlFor="username" className="text-sm font-semibold text-foreground">
                  Username
                </label>
                <Input
                  id="username"
                  type="text"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="Enter your username"
                  className={formErrors.username ? 'border-error' : ''}
                  disabled={loginMutation.isPending}
                  autoComplete="username"
                  autoFocus
                />
                {formErrors.username && (
                  <p className="text-xs text-error">{formErrors.username}</p>
                )}
              </div>

              {/* Password field */}
              <div className="space-y-2">
                <label htmlFor="password" className="text-sm font-semibold text-foreground">
                  Password
                </label>
                <Input
                  id="password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="Enter your password"
                  className={formErrors.password ? 'border-error' : ''}
                  disabled={loginMutation.isPending}
                  autoComplete="current-password"
                />
                {formErrors.password && (
                  <p className="text-xs text-error">{formErrors.password}</p>
                )}
              </div>

              <Button
                type="submit"
                className="w-full"
                disabled={loginMutation.isPending}
              >
                {loginMutation.isPending ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Signing in...
                  </>
                ) : (
                  'Sign in'
                )}
              </Button>
            </form>

            <div className="mt-6 text-center text-sm text-muted-foreground space-y-1">
              <p>No self-service sign-up is available in this build.</p>
              <p>
                Ask an ARCXA administrator to create your account, or complete the
                one-time admin setup on a brand-new instance.
              </p>
            </div>
          </CardContent>
        </Card>

        <AppLegalFooter centered />
      </div>
    </div>
  );
}
