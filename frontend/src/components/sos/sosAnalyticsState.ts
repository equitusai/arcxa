import {
  DEFAULT_VISIBLE_SOS_GRAPH_KINDS,
  type SosGraphKind,
} from '@/components/sos/sosDependencyGraphUtils';

export interface SosAnalyticsInvestigationState {
  graphLoaded: boolean;
  selectedNodeId: string | null;
  selectedEdgeKey: string | null;
  visibleKinds: SosGraphKind[];
}

export function createDefaultSosAnalyticsInvestigationState(): SosAnalyticsInvestigationState {
  return {
    graphLoaded: false,
    selectedNodeId: null,
    selectedEdgeKey: null,
    visibleKinds: [...DEFAULT_VISIBLE_SOS_GRAPH_KINDS],
  };
}

export function normalizeSosAnalyticsInvestigationState(
  state?: Partial<SosAnalyticsInvestigationState> | null
): SosAnalyticsInvestigationState {
  const defaults = createDefaultSosAnalyticsInvestigationState();

  return {
    graphLoaded: state?.graphLoaded ?? defaults.graphLoaded,
    selectedNodeId: state?.selectedNodeId ?? defaults.selectedNodeId,
    selectedEdgeKey: state?.selectedEdgeKey ?? defaults.selectedEdgeKey,
    visibleKinds:
      state?.visibleKinds && state.visibleKinds.length > 0
        ? [...state.visibleKinds]
        : defaults.visibleKinds,
  };
}
