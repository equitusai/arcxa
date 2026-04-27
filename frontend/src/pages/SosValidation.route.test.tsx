import React from 'react';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SosValidation } from './SosValidation';

const mockLookupContract = vi.fn();
const mockValidatePair = vi.fn();
const mockLoadDependencyGraph = vi.fn();
const mockRunWhatIfAnalysis = vi.fn();
const mockValidateSchema = vi.fn();
const mockLookupReport = vi.fn();
const mockLookupHistory = vi.fn();
const mockLookupLineage = vi.fn();

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
        contract_id: 'contract.provider.consumer',
        contract_name: 'Provider To Consumer Contract',
        provider_interface_id: 'iface.provider',
        consumer_interface_id: 'iface.consumer',
        sla_metrics: [],
        transformation_rules: {},
        tags: [],
        approved: true,
        signed: true,
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
  useLookupValidationReport: () => ({
    mutateAsync: mockLookupReport,
    isPending: false,
  }),
  useLookupValidationHistory: () => ({
    mutateAsync: mockLookupHistory,
    isPending: false,
  }),
  useLookupValidationLineage: () => ({
    mutateAsync: mockLookupLineage,
    isPending: false,
  }),
}));

vi.mock('@/components/sos/SosCatalogPanel', () => ({
  SosCatalogPanel: () => <div data-testid="catalog-panel">catalog</div>,
}));

vi.mock('@/components/sos/SosPoliciesPanel', () => ({
  SosPoliciesPanel: () => <div data-testid="policies-panel">policies</div>,
}));

function LocationProbe() {
  const location = useLocation();
  return <div data-testid="location-search">{location.search}</div>;
}

function makeReport(subjectKey: string) {
  return {
    report_id: 'report-1',
    validation_id: 'validation-1',
    subject_type: 'interface_pair',
    subject_key: subjectKey,
    validation_type: 'interface_compatibility',
    passed: true,
    confidence: 0.92,
    checks: [
      {
        check_name: 'protocol',
        passed: true,
        severity: 'info',
        description: 'Protocols align',
      },
    ],
    validated_at: '2026-04-22T00:00:00Z',
    previous_report_id: null,
    change_summary: {
      resolved_checks: [],
      new_failures: [],
      confidence_delta: 0,
      schema_or_policy_version_changed: false,
    },
    workflow_execution_id: null,
    workflow_step_id: null,
    ontology_refs: [],
    shape_refs: [],
    policy_refs: [],
    schema_hashes: {},
  };
}

describe('SosValidation route flow', () => {
  beforeEach(() => {
    mockLookupContract.mockReset();
    mockValidatePair.mockReset();
    mockLoadDependencyGraph.mockReset();
    mockRunWhatIfAnalysis.mockReset();
    mockValidateSchema.mockReset();
    mockLookupReport.mockReset();
    mockLookupHistory.mockReset();
    mockLookupLineage.mockReset();

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

    mockLookupHistory.mockResolvedValue({
      subject_type: 'interface_pair',
      subject_key: 'interface_pair:iface.provider:iface.consumer',
      reports: [makeReport('interface_pair:iface.provider:iface.consumer')],
    });
    mockLookupLineage.mockResolvedValue({
      subject_type: 'interface_pair',
      subject_key: 'interface_pair:iface.provider:iface.consumer',
      reports: [makeReport('interface_pair:iface.provider:iface.consumer')],
      edges: [],
    });
  });

  it('hands analytics edge actions into the reports workspace and persists the report subject in the URL', async () => {
    render(
      <MemoryRouter initialEntries={['/sos-validation?tab=analytics&provider=iface.provider&consumer=iface.consumer']}>
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

    fireEvent.click(screen.getByRole('button', { name: 'Load Graph' }));

    await waitFor(() => {
      expect(mockLoadDependencyGraph).toHaveBeenCalledTimes(1);
    });

    const integratesRow = screen
      .getAllByRole('row')
      .find((row) => within(row).queryByText('integrates_with'));

    expect(integratesRow).toBeTruthy();
    fireEvent.click(within(integratesRow as HTMLElement).getByRole('button', { name: 'Inspect Edge' }));
    fireEvent.click(screen.getByRole('button', { name: 'Open Pair History' }));

    await waitFor(() => {
      expect(mockLookupHistory).toHaveBeenCalledWith({
        subjectKey: 'interface_pair:iface.provider:iface.consumer',
        subjectType: 'interface_pair',
        limit: 25,
      });
    });
    expect(mockLookupLineage).toHaveBeenCalledWith({
      subjectKey: 'interface_pair:iface.provider:iface.consumer',
      subjectType: 'interface_pair',
      limit: 25,
    });

    expect(screen.getByDisplayValue('interface_pair:iface.provider:iface.consumer')).toBeTruthy();
    expect(screen.getByTestId('location-search').textContent).toContain('tab=reports');
    expect(screen.getByTestId('location-search').textContent).toContain('reportSubjectType=interface_pair');
    expect(screen.getByTestId('location-search').textContent).toContain(
      'reportSubjectKey=interface_pair%3Aiface.provider%3Aiface.consumer'
    );
  });
});
