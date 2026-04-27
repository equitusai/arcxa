import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SosCatalogPanel } from './SosCatalogPanel';

vi.mock('@/hooks/useSosValidation', () => {
  const idleMutation = () => ({
    isPending: false,
    mutateAsync: vi.fn(),
  });

  return {
    useSosSystems: () => ({
      data: {
        systems: [
          {
            system_id: 'sys.provider',
            system_name: 'Provider System',
            system_type: 'mission.broker',
            vendor: 'Graphica',
            version: '1.0.0',
            classification: 'UNCLASSIFIED',
            description: null,
            deployment: {},
            capabilities: {},
            tags: [],
            active: true,
            created_at: '2026-04-22T00:00:00Z',
            updated_at: '2026-04-22T00:00:00Z',
          },
        ],
      },
      isLoading: false,
      error: null,
    }),
    useSosInterfaces: () => ({
      data: [
        {
          system_id: 'sys.provider',
          interface_id: 'iface.provider',
          interface_name: 'Provider Interface',
          direction: 'Provider',
          protocol: 'REST',
          data_format: 'JSON',
          schema: { type: 'object' },
          metadata: {},
          created_at: '2026-04-22T00:00:00Z',
          updated_at: '2026-04-22T00:00:00Z',
        },
        {
          system_id: 'sys.provider',
          interface_id: 'iface.consumer',
          interface_name: 'Consumer Interface',
          direction: 'Consumer',
          protocol: 'REST',
          data_format: 'JSON',
          schema: { type: 'object' },
          metadata: {},
          created_at: '2026-04-22T00:00:00Z',
          updated_at: '2026-04-22T00:00:00Z',
        },
      ],
      isLoading: false,
      error: null,
    }),
    useSosContracts: () => ({
      data: [
        {
          contract_id: 'contract-1',
          contract_name: 'Provider To Consumer Contract',
          provider_interface_id: 'iface.provider',
          consumer_interface_id: 'iface.consumer',
          sla_metrics: [],
          transformation_rules: {},
          tags: [],
          approved: true,
          signed: false,
          created_at: '2026-04-22T00:00:00Z',
          updated_at: '2026-04-22T00:00:00Z',
        },
      ],
      isLoading: false,
      error: null,
    }),
    useCreateSosSystem: idleMutation,
    useUpdateSosSystem: idleMutation,
    useDeleteSosSystem: idleMutation,
    useCreateSosInterface: idleMutation,
    useUpdateSosInterface: idleMutation,
    useDeleteSosInterface: idleMutation,
    useCreateSosContract: idleMutation,
    useUpdateSosContract: idleMutation,
    useDeleteSosContract: idleMutation,
    useApproveSosContract: idleMutation,
    useSignSosContract: idleMutation,
  };
});

describe('SosCatalogPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('emits exact selection changes and restores the active interface when tabs change', async () => {
    const onSelectionChange = vi.fn();
    const { rerender } = render(
      <SosCatalogPanel seedTab="interfaces" onSelectionChange={onSelectionChange} />
    );

    onSelectionChange.mockClear();

    fireEvent.click(screen.getByRole('button', { name: /Provider Interface/i }));

    await waitFor(() => {
      expect(onSelectionChange).toHaveBeenLastCalledWith({
        tab: 'interfaces',
        systemId: null,
        interfaceId: 'iface.provider',
        contractId: null,
      });
    });

    rerender(<SosCatalogPanel seedTab="systems" onSelectionChange={onSelectionChange} />);
    rerender(<SosCatalogPanel seedTab="interfaces" onSelectionChange={onSelectionChange} />);

    await waitFor(() => {
      expect(screen.getByDisplayValue('Provider Interface')).toBeTruthy();
    });
    expect(screen.getByDisplayValue('iface.provider')).toBeTruthy();
  });
});
