import type { SosDependencyGraphEdge } from '@/api/sosValidation';

export type SosGraphKind = 'system' | 'interface' | 'contract';

export const SOS_GRAPH_KINDS: SosGraphKind[] = ['system', 'interface', 'contract'];

export const DEFAULT_VISIBLE_SOS_GRAPH_KINDS: SosGraphKind[] = [...SOS_GRAPH_KINDS];

export function getDependencyGraphEdgeKey(edge: SosDependencyGraphEdge): string {
  return [edge.from, edge.to, edge.kind, edge.contract_id ?? ''].join('::');
}

export function buildVisibleKindState(
  kinds: readonly SosGraphKind[] = DEFAULT_VISIBLE_SOS_GRAPH_KINDS
): Record<SosGraphKind, boolean> {
  const visibleKinds = new Set(kinds);

  return {
    system: visibleKinds.has('system'),
    interface: visibleKinds.has('interface'),
    contract: visibleKinds.has('contract'),
  };
}

export function extractVisibleKinds(
  visibleKinds: Record<SosGraphKind, boolean>
): SosGraphKind[] {
  const activeKinds = SOS_GRAPH_KINDS.filter((kind) => visibleKinds[kind]);
  return activeKinds.length > 0 ? activeKinds : [...DEFAULT_VISIBLE_SOS_GRAPH_KINDS];
}

export function parseVisibleKinds(rawValue: string | null | undefined): SosGraphKind[] {
  if (!rawValue) {
    return [...DEFAULT_VISIBLE_SOS_GRAPH_KINDS];
  }

  const parsedKinds = rawValue
    .split(',')
    .map((value) => value.trim())
    .filter((value): value is SosGraphKind =>
      (SOS_GRAPH_KINDS as string[]).includes(value)
    );

  return parsedKinds.length > 0 ? parsedKinds : [...DEFAULT_VISIBLE_SOS_GRAPH_KINDS];
}
