import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SosValidation } from './SosValidation';

const mockLookupContract = vi.fn();
const mockValidatePair = vi.fn();

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
  useSosContracts: () => ({
    data: [
      {
        contract_id: 'contract-1',
        contract_name: 'Contract 1',
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
  useSosCompatibilityMatrix: () => ({
    data: {
      matrix: [],
      generated_at: '2026-04-22T00:00:00Z',
    },
    isLoading: false,
    error: null,
  }),
  useLookupSosContract: () => ({
    mutateAsync: mockLookupContract,
    isPending: false,
  }),
  useValidateInterfacePair: () => ({
    mutateAsync: mockValidatePair,
    isPending: false,
  }),
}));

vi.mock('@/components/sos/SosCatalogPanel', () => ({
  SosCatalogPanel: ({
    seedTab,
    seedSystemId,
    seedInterfaceId,
    seedContractId,
    seedToken,
    onSelectionChange,
  }: {
    seedTab?: string;
    seedSystemId?: string | null;
    seedInterfaceId?: string | null;
    seedContractId?: string | null;
    seedToken?: number;
    onSelectionChange?: (state: {
      tab: string;
      systemId?: string | null;
      interfaceId?: string | null;
      contractId?: string | null;
    }) => void;
  }) => (
    <div>
      <div data-testid="catalog-panel">
        {JSON.stringify({ seedTab, seedSystemId, seedInterfaceId, seedContractId, seedToken })}
      </div>
      <button
        type="button"
        onClick={() =>
          onSelectionChange?.({
            tab: 'interfaces',
            systemId: 'sys.provider',
            interfaceId: 'iface.provider',
            contractId: null,
          })
        }
      >
        Select Interface In Catalog
      </button>
      <button
        type="button"
        onClick={() =>
          onSelectionChange?.({
            tab: 'contracts',
            systemId: 'sys.provider',
            interfaceId: 'iface.provider',
            contractId: 'contract-1',
          })
        }
      >
        Select Contract In Catalog
      </button>
    </div>
  ),
}));

vi.mock('@/components/sos/SosReportsPanel', () => ({
  SosReportsPanel: () => <div data-testid="reports-panel">reports</div>,
}));

vi.mock('@/components/sos/SosPoliciesPanel', () => ({
  SosPoliciesPanel: () => <div data-testid="policies-panel">policies</div>,
}));

vi.mock('@/components/sos/SosAnalyticsPanel', () => ({
  SosAnalyticsPanel: ({
    currentPair,
    investigationState,
    onOpenCatalog,
    onInvestigationStateChange,
    onUsePair,
  }: {
    currentPair?: { providerInterfaceId: string; consumerInterfaceId: string } | null;
    investigationState?: {
      graphLoaded: boolean;
      selectedNodeId: string | null;
      selectedEdgeKey: string | null;
      visibleKinds: string[];
    };
    onOpenCatalog?: (target: { tab: string; contractId?: string | null }) => void;
    onInvestigationStateChange?: (state: {
      graphLoaded: boolean;
      selectedNodeId: string | null;
      selectedEdgeKey: string | null;
      visibleKinds: string[];
    }) => void;
    onUsePair?: (providerInterfaceId: string, consumerInterfaceId: string) => void;
  }) => (
    <div>
      <div data-testid="analytics-current-pair">{JSON.stringify(currentPair)}</div>
      <div data-testid="analytics-investigation">{JSON.stringify(investigationState)}</div>
      <button
        type="button"
        onClick={() =>
          onInvestigationStateChange?.({
            graphLoaded: true,
            selectedNodeId: null,
            selectedEdgeKey: 'iface.provider::iface.consumer::integrates_with::contract-1',
            visibleKinds: ['contract'],
          })
        }
      >
        Update Investigation
      </button>
      <button
        type="button"
        onClick={() => onOpenCatalog?.({ tab: 'contracts', contractId: 'contract-1' })}
      >
        Open Contract Catalog
      </button>
      <button
        type="button"
        onClick={() => onUsePair?.('iface.alt.provider', 'iface.alt.consumer')}
      >
        Use Graph Pair
      </button>
    </div>
  ),
}));

vi.mock('@/components/sos/SosOperationsPanel', () => ({
  SosOperationsPanel: () => <div data-testid="operations-panel">operations</div>,
}));

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location-search">{location.search}</div>;
}

function renderSosValidation(initialEntry: string) {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <Routes>
        <Route
          path="/sos-validation"
          element={
            <>
              <SosValidation />
              <LocationProbe />
            </>
          }
        />
      </Routes>
    </MemoryRouter>
  );
}

describe('SosValidation', () => {
  beforeEach(() => {
    mockLookupContract.mockReset();
    mockValidatePair.mockReset();
  });

  it('restores analytics investigation state from the URL and persists catalog handoff updates', async () => {
    renderSosValidation(
      '/sos-validation?tab=analytics&provider=iface.provider&consumer=iface.consumer&analyticsGraph=1&analyticsNode=iface.provider&analyticsLanes=system,contract'
    );

    expect(screen.getByTestId('analytics-current-pair').textContent).toContain('iface.provider');
    expect(screen.getByTestId('analytics-current-pair').textContent).toContain('iface.consumer');
    expect(screen.getByTestId('analytics-investigation').textContent).toContain('"graphLoaded":true');
    expect(screen.getByTestId('analytics-investigation').textContent).toContain('"selectedNodeId":"iface.provider"');
    expect(screen.getByTestId('analytics-investigation').textContent).toContain('"visibleKinds":["system","contract"]');

    fireEvent.click(screen.getByRole('button', { name: 'Update Investigation' }));

    await waitFor(() => {
      expect(screen.getByTestId('location-search').textContent).toContain('analyticsEdge=iface.provider%3A%3Aiface.consumer%3A%3Aintegrates_with%3A%3Acontract-1');
    });
    expect(screen.getByTestId('location-search').textContent).toContain('analyticsLanes=contract');

    fireEvent.click(screen.getByRole('button', { name: 'Open Contract Catalog' }));

    await waitFor(() => {
      expect(screen.getByTestId('catalog-panel').textContent).toContain('contract-1');
    });
    expect(screen.getByTestId('catalog-panel').textContent).toContain('contracts');
    expect(screen.getByTestId('location-search').textContent).toContain('tab=catalog');
    expect(screen.getByTestId('location-search').textContent).toContain('catalogTab=contracts');
    expect(screen.getByTestId('location-search').textContent).toContain('catalogContract=contract-1');
  });

  it('pushes graph-selected interface pairs back into the workbench and URL state', async () => {
    renderSosValidation('/sos-validation?tab=analytics&provider=iface.provider&consumer=iface.consumer');

    fireEvent.click(screen.getByRole('button', { name: 'Use Graph Pair' }));

    await waitFor(() => {
      expect(screen.getByDisplayValue('iface.alt.provider')).toBeTruthy();
    });
    expect(screen.getByDisplayValue('iface.alt.consumer')).toBeTruthy();
    expect(screen.getByTestId('location-search').textContent).toContain('provider=iface.alt.provider');
    expect(screen.getByTestId('location-search').textContent).toContain('consumer=iface.alt.consumer');
    expect(screen.getByTestId('location-search').textContent).not.toContain('tab=analytics');
  });

  it('persists manual catalog selection changes back into URL state', async () => {
    renderSosValidation('/sos-validation?tab=catalog');

    fireEvent.click(screen.getByRole('button', { name: 'Select Interface In Catalog' }));

    await waitFor(() => {
      expect(screen.getByTestId('catalog-panel').textContent).toContain('"seedTab":"interfaces"');
    });
    expect(screen.getByTestId('catalog-panel').textContent).toContain('"seedSystemId":"sys.provider"');
    expect(screen.getByTestId('catalog-panel').textContent).toContain(
      '"seedInterfaceId":"iface.provider"'
    );
    expect(screen.getByTestId('location-search').textContent).toContain('tab=catalog');
    expect(screen.getByTestId('location-search').textContent).toContain('catalogTab=interfaces');
    expect(screen.getByTestId('location-search').textContent).toContain('catalogSystem=sys.provider');
    expect(screen.getByTestId('location-search').textContent).toContain(
      'catalogInterface=iface.provider'
    );

    fireEvent.click(screen.getByRole('button', { name: 'Select Contract In Catalog' }));

    await waitFor(() => {
      expect(screen.getByTestId('catalog-panel').textContent).toContain('"seedTab":"contracts"');
    });
    expect(screen.getByTestId('catalog-panel').textContent).toContain('"seedContractId":"contract-1"');
    expect(screen.getByTestId('location-search').textContent).toContain('catalogTab=contracts');
    expect(screen.getByTestId('location-search').textContent).toContain('catalogContract=contract-1');
  });
});
