import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SosReportsPanel } from "./SosReportsPanel";

const mockLookupReport = vi.fn();
const mockLookupHistory = vi.fn();
const mockLookupLineage = vi.fn();

vi.mock("@/hooks/useSosValidation", () => ({
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

const historyReports = [
  {
    report_id: "report-3",
    validation_id: "validation-3",
    subject_type: "interface_pair",
    subject_key: "interface_pair:iface.provider:iface.consumer",
    validation_type: "interface_compatibility",
    passed: true,
    confidence: 0.9,
    checks: [],
    validated_at: "2026-04-25T12:00:00Z",
    previous_report_id: "report-2",
    change_summary: {
      resolved_checks: ["schema.mismatch"],
      new_failures: [],
      confidence_delta: 0.4,
      schema_or_policy_version_changed: false,
    },
    ontology_refs: ["ontology:sos_core"],
    shape_refs: ["shape:provider"],
    policy_refs: ["policy:rollout-gate@2"],
    schema_hashes: {
      provider: "sha256:provider-v3",
    },
  },
  {
    report_id: "report-1",
    validation_id: "validation-1",
    subject_type: "interface_pair",
    subject_key: "interface_pair:iface.provider:iface.consumer",
    validation_type: "interface_compatibility",
    passed: true,
    confidence: 0.7,
    checks: [],
    validated_at: "2026-04-23T12:00:00Z",
    previous_report_id: null,
    change_summary: {
      resolved_checks: [],
      new_failures: [],
      confidence_delta: 0,
      schema_or_policy_version_changed: false,
    },
    ontology_refs: ["ontology:sos_core"],
    shape_refs: ["shape:provider"],
    policy_refs: ["policy:rollout-gate@1"],
    schema_hashes: {
      provider: "sha256:provider-v1",
    },
  },
  {
    report_id: "report-2",
    validation_id: "validation-2",
    subject_type: "interface_pair",
    subject_key: "interface_pair:iface.provider:iface.consumer",
    validation_type: "interface_compatibility",
    passed: false,
    confidence: 0.5,
    checks: [],
    validated_at: "2026-04-24T12:00:00Z",
    previous_report_id: "report-1",
    change_summary: {
      resolved_checks: [],
      new_failures: ["schema.mismatch"],
      confidence_delta: -0.2,
      schema_or_policy_version_changed: true,
    },
    ontology_refs: ["ontology:sos_core"],
    shape_refs: ["shape:provider"],
    policy_refs: ["policy:rollout-gate@2"],
    schema_hashes: {
      provider: "sha256:provider-v2",
    },
  },
];

beforeEach(() => {
  mockLookupReport.mockReset();
  mockLookupHistory.mockReset();
  mockLookupLineage.mockReset();

  mockLookupReport.mockResolvedValue(historyReports[0]);
  mockLookupHistory.mockResolvedValue({
    subject_type: "interface_pair",
    subject_key: "interface_pair:iface.provider:iface.consumer",
    reports: historyReports,
  });
  mockLookupLineage.mockResolvedValue({
    subject_type: "interface_pair",
    subject_key: "interface_pair:iface.provider:iface.consumer",
    reports: historyReports,
    edges: [
      {
        from_report_id: "report-1",
        to_report_id: "report-2",
        relationship: "supersedes",
      },
      {
        from_report_id: "report-2",
        to_report_id: "report-3",
        relationship: "supersedes",
      },
    ],
  });
});

describe("SosReportsPanel", () => {
  it("renders history and lineage trend summaries inside the existing reports workspace", async () => {
    render(
      <SosReportsPanel
        latestReportId="report-3"
        seedSubjectType="interface_pair"
        seedSubjectKey="interface_pair:iface.provider:iface.consumer"
      />,
    );

    await waitFor(() => {
      expect(mockLookupReport).toHaveBeenCalledWith("report-3");
      expect(mockLookupHistory).toHaveBeenCalledWith({
        subjectKey: "interface_pair:iface.provider:iface.consumer",
        subjectType: "interface_pair",
        limit: 25,
      });
      expect(mockLookupLineage).toHaveBeenCalledWith({
        subjectKey: "interface_pair:iface.provider:iface.consumer",
        subjectType: "interface_pair",
        limit: 25,
      });
    });

    expect(await screen.findByText("Change Timeline")).toBeTruthy();
    expect(screen.getByText(/across 3 loaded reports\./)).toBeTruthy();
    expect(screen.getAllByText("+20 pts").length).toBeGreaterThan(0);
    expect(screen.getAllByText("schema.mismatch").length).toBeGreaterThan(0);

    expect(screen.getByText("Lineage Progression")).toBeTruthy();
    expect(screen.getByText("Current Head")).toBeTruthy();
    expect(
      screen.getByText(/Primary relationship: supersedes x2/),
    ).toBeTruthy();
    expect(
      screen.getByText(/Upstream link: supersedes from report-2/),
    ).toBeTruthy();
    expect(screen.getByText("Lineage Edges")).toBeTruthy();
    expect(screen.getAllByText("supersedes").length).toBeGreaterThan(0);
  });
});
