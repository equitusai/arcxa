import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import App from './App';
import { useAuthStore } from '@/stores/auth';

vi.mock('@/components/Layout', async () => {
  const router = await vi.importActual<typeof import('react-router-dom')>('react-router-dom');

  return {
    Layout: () => (
      <div data-testid="layout-shell">
        <router.Outlet />
      </div>
    ),
  };
});

vi.mock('@/pages/Login', () => ({
  Login: () => <div data-testid="login-page">{`${window.location.pathname}${window.location.search}`}</div>,
}));

vi.mock('@/pages/MigrationEvidence', () => ({
  MigrationEvidence: () => (
    <div data-testid="migration-evidence-page">{`${window.location.pathname}${window.location.search}`}</div>
  ),
}));

describe('App migration-evidence route', () => {
  beforeEach(() => {
    useAuthStore.getState().clearAuth();
    window.history.pushState({}, '', '/');
  });

  it('routes authenticated users directly to the migration-evidence workspace and preserves URL state', async () => {
    useAuthStore.getState().setAuth('token', {
      id: 'user-1',
      username: 'tester',
      role: 'Admin',
      created_at: '2026-04-30T00:00:00Z',
    });
    window.history.pushState(
      {},
      '',
      '/migration-evidence?tab=audit&program=program-rise-1&object=object-sales-order&field=%24.amount'
    );

    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId('migration-evidence-page').textContent).toContain(
        '/migration-evidence?tab=audit&program=program-rise-1&object=object-sales-order&field=%24.amount'
      );
    });
    expect(screen.getByTestId('layout-shell')).toBeTruthy();
    expect(screen.queryByTestId('login-page')).toBeNull();
  });

  it('redirects unauthenticated users hitting the migration-evidence route back to login', async () => {
    window.history.pushState({}, '', '/migration-evidence?tab=connectors');

    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId('login-page').textContent).toContain('/login');
    });
    expect(screen.queryByTestId('migration-evidence-page')).toBeNull();
  });
});
