import { describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('./client', () => ({
  api: {
    put: vi.fn(),
  },
}));

import { api } from './client';
import {
  buildDatasourceSecretRef,
  putSecret,
  secretRefToApiPath,
  storeDatasourceCredentials,
} from './secrets';

describe('secrets api helpers', () => {
  beforeEach(() => {
    vi.mocked(api.put).mockReset();
    vi.mocked(api.put).mockResolvedValue({
      path: 'datasources/test-source/credentials',
      version: 'v1',
      store: 'default',
      created_at: '2026-03-31T00:00:00Z',
    });
  });

  it('builds datasource secret refs with the current coordinator shape', () => {
    expect(buildDatasourceSecretRef('oracle-d8yg')).toBe(
      'vault://datasources/oracle-d8yg/credentials'
    );
  });

  it('converts vault secret refs into REST secret paths', () => {
    expect(
      secretRefToApiPath('vault://datasources/oracle-d8yg/credentials')
    ).toBe('datasources/oracle-d8yg/credentials');
  });

  it('keeps legacy vault credential refs routable while we transition', () => {
    expect(secretRefToApiPath('vault://credentials/oracle-d8yg')).toBe(
      'credentials/oracle-d8yg'
    );
  });

  it('stores datasource credentials using the encoded secret path, not the raw vault URI', async () => {
    await storeDatasourceCredentials(
      'vault://datasources/oracle-d8yg/credentials',
      {
        username: 'demo_user',
        password: 'demo_user',
      },
      'Credentials for oracle-d8yg'
    );

    expect(api.put).toHaveBeenCalledWith(
      '/secrets/datasources%2Foracle-d8yg%2Fcredentials',
      {
        value: {
          username: 'demo_user',
          password: 'demo_user',
        },
        description: 'Credentials for oracle-d8yg',
        store: 'default',
      }
    );
  });

  it('accepts direct secret paths too', async () => {
    await putSecret('datasources/oracle-d8yg/credentials', {
      value: { username: 'demo_user' },
      store: 'default',
    });

    expect(api.put).toHaveBeenCalledWith(
      '/secrets/datasources%2Foracle-d8yg%2Fcredentials',
      {
        value: { username: 'demo_user' },
        store: 'default',
      }
    );
  });
});
