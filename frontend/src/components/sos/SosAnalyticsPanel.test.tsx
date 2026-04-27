import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SosAnalyticsPanel } from './SosAnalyticsPanel';

const mockLoadDependencyGraph = vi.fn();
const mockRunWhatIfAnalysis = vi.fn();
const mockValidateSchema = vi.fn();

vi.mock('@/hooks/useSosValidation', () => ({
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
        system_id: 'sys.consumer',
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
  useLookupSosDependencyGraph: () => ({
    mutateAsync: mockLoadDependencyGraph,
    isPending: false,
  }),
  useRunSosWhatIfAnalysis: () => ({
    mutateAsync: mockRunWhatIfAnalysis,
    isPending: false,
  }),
  useValidateSosInterfaceSchema: () => ({
    mutateAsync: mockValidateSchema,
    isPending: false,
  }),
}));

describe('SosAnalyticsPanel', () => {
  beforeEach(() => {
    mockLoadDependencyGraph.mockReset();
    mockRunWhatIfAnalysis.mockReset();
    mockValidateSchema.mockReset();

    mockLoadDependencyGraph.mockResolvedValue({
      generated_at: '2026-04-22T00:00:00Z',
      nodes: [
        { id: 'sys.provider', kind: 'system', label: 'Provider System', system_id: 'sys.provider' },
        { id: 'iface.provider', kind: 'interface', label: 'Provider Interface', system_id: 'sys.provider' },
        { id: 'contract.provider.consumer', kind: 'contract', label: 'Provider To Consumer Contract' },
        { id: 'iface.consumer', kind: 'interface', label: 'Consumer Interface', system_id: 'sys.consumer' },
      ],
      edges: [
        { from: 'sys.provider', to: 'iface.provider', kind: 'exposes' },
        { from: 'iface.provider', to: 'contract.provider.consumer', kind: 'governs_provider' },
        { from: 'contract.provider.consumer', to: 'iface.consumer', kind: 'governs_consumer' },
        {
          from: 'iface.provider',
          to: 'iface.consumer',
          kind: 'integrates_with',
          contract_id: 'contract.provider.consumer',
        },
      ],
    });
  });

  it('routes contract-edge actions to catalog, workbench, and reports', async () => {
    const onOpenCatalog = vi.fn();
    const onUsePair = vi.fn();
    const onOpenReports = vi.fn();

    const { unmount } = render(
      <SosAnalyticsPanel
        currentPair={{
          providerInterfaceId: 'iface.provider',
          consumerInterfaceId: 'iface.consumer',
        }}
        onOpenCatalog={onOpenCatalog}
        onUsePair={onUsePair}
        onOpenReports={onOpenReports}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Load Graph' }));

    await waitFor(() => {
      expect(mockLoadDependencyGraph).toHaveBeenCalledTimes(1);
    });

    const integratesRow = screen
      .getAllByRole('row')
      .find((row) => within(row).queryByText('integrates_with'));

    expect(integratesRow).toBeTruthy();
    fireEvent.click(within(integratesRow as HTMLElement).getByRole('button', { name: 'Inspect Edge' }));

    fireEvent.click(screen.getByRole('button', { name: 'Open Contract In Catalog' }));
    expect(onOpenCatalog).toHaveBeenCalledWith({
      tab: 'contracts',
      contractId: 'contract.provider.consumer',
    });

    fireEvent.click(screen.getByRole('button', { name: 'Open Pair In Workbench' }));
    expect(onUsePair).toHaveBeenCalledWith('iface.provider', 'iface.consumer');

    fireEvent.click(screen.getByRole('button', { name: 'Open Pair History' }));
    expect(onOpenReports).toHaveBeenCalledWith({
      subjectType: 'interface_pair',
      subjectKey: 'interface_pair:iface.provider:iface.consumer',
    });

    unmount();
  });
});
