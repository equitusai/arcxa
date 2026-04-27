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

vi.mock('@/pages/SosValidation', () => ({
  SosValidation: () => (
    <div data-testid="sos-page">{`${window.location.pathname}${window.location.search}`}</div>
  ),
}));

describe('App SoS route', () => {
  beforeEach(() => {
    useAuthStore.getState().clearAuth();
    window.history.pushState({}, '', '/');
  });

  it('routes authenticated users directly to the SoS validation workspace and preserves URL state', async () => {
    useAuthStore.getState().setAuth('token', {
      id: 'user-1',
      username: 'tester',
      role: 'Admin',
      created_at: '2026-04-22T00:00:00Z',
    });
    window.history.pushState(
      {},
      '',
      '/sos-validation?tab=reports&reportSubjectType=interface_pair&reportSubjectKey=interface_pair%3Aiface.provider%3Aiface.consumer'
    );

    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId('sos-page').textContent).toContain(
        '/sos-validation?tab=reports&reportSubjectType=interface_pair&reportSubjectKey=interface_pair%3Aiface.provider%3Aiface.consumer'
      );
    });
    expect(screen.getByTestId('layout-shell')).toBeTruthy();
    expect(screen.queryByTestId('login-page')).toBeNull();
  });

  it('redirects unauthenticated users hitting the SoS route back to login', async () => {
    window.history.pushState({}, '', '/sos-validation?tab=analytics');

    render(<App />);

    await waitFor(() => {
      expect(screen.getByTestId('login-page').textContent).toContain('/login');
    });
    expect(screen.queryByTestId('sos-page')).toBeNull();
  });
});
