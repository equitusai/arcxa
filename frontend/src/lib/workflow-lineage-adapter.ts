import type {
  RowJourneyResponse,
  RowLineageEvent,
  RunLineageResponse,
} from '@/api/types';
import type { LineageGraph } from '@/hooks/useLineageGraph';

type RowIdentity = NonNullable<RowJourneyResponse['source']>;
type JourneyStep = NonNullable<RowJourneyResponse['steps']>[number];

const NODE_COLORS = {
  source: '#3b82f6',
  step: '#8b5cf6',
  destination: '#06b6d4',
};

function getSourceTypeLabel(sourceType?: Record<string, string>): string {
  if (!sourceType) return 'row';
  const [kind, value] = Object.entries(sourceType)[0] || [];
  if (typeof value === 'string' && value.length > 0) {
    return value.toLowerCase();
  }
  return kind ? kind.toLowerCase() : 'row';
}

export function formatRowIdentity(row: RowIdentity | null | undefined): string {
  if (!row) return 'unknown:row';

  const sourceType = getSourceTypeLabel(row.source_type);
  const sourceId = row.source_id || 'unknown_source';
  const position = row.position || {};
  const [positionKind, positionValue] = Object.entries(position)[0] || [];

  if (positionKind === 'PrimaryKey' && positionValue && typeof positionValue === 'object') {
    const primaryKey = Object.entries(positionValue as Record<string, unknown>)
      .map(([key, value]) => `${key}=${String(value)}`)
      .join(',');
    return `${sourceType}:${sourceId}:${primaryKey}`;
  }

  if (positionKind && positionValue !== undefined) {
    return `${sourceType}:${sourceId}:${positionKind}=${String(positionValue)}`;
  }

  return `${sourceType}:${sourceId}`;
}

export function formatOutcomeLabel(outcome: Record<string, unknown> | undefined): string {
  if (!outcome) return 'Unknown';

  if ('Processed' in outcome) {
    const processed = outcome.Processed;
    if (processed && typeof processed === 'object' && 'output_location' in processed) {
      return `Processed to ${String((processed as Record<string, unknown>).output_location)}`;
    }
    return 'Processed';
  }

  if ('Filtered' in outcome) {
    const filtered = outcome.Filtered;
    if (filtered && typeof filtered === 'object' && 'reason' in filtered) {
      return `Filtered: ${String((filtered as Record<string, unknown>).reason)}`;
    }
    return 'Filtered';
  }

  if ('Failed' in outcome) {
    const failed = outcome.Failed;
    if (failed && typeof failed === 'object' && 'error_message' in failed) {
      return `Failed: ${String((failed as Record<string, unknown>).error_message)}`;
    }
    return 'Failed';
  }

  return Object.keys(outcome)[0] || 'Unknown';
}

function getJourneyTimestamps(journey: RowJourneyResponse): { start: string; end: string } {
  const steps = journey.steps || [];
  const firstTimestamp = steps[0]?.timestamp || new Date().toISOString();
  const lastTimestamp = steps[steps.length - 1]?.timestamp || firstTimestamp;

  return {
    start: firstTimestamp,
    end: lastTimestamp,
  };
}

function createStepNode(step: JourneyStep, index: number) {
  const nodeId = `step:${index}:${step.timestamp}`;
  return {
    id: nodeId,
    label: step.activity,
    type: 'record' as const,
    recordId: nodeId,
    dataset: 'workflow_step',
    timestamp: step.timestamp,
    metadata: {
      activity: step.activity,
      duration_ms: step.duration_ms,
      outcome: step.outcome,
      outcome_label: formatOutcomeLabel(step.outcome),
    },
    color: NODE_COLORS.step,
    size: 2,
  };
}

export function buildRowJourneyGraph(journey: RowJourneyResponse): LineageGraph {
  const steps = journey.steps || [];
  const timestamps = getJourneyTimestamps(journey);
  const datasets = new Set<string>();

  const sourceId = `source:${formatRowIdentity(journey.source)}`;
  const sourceDataset = journey.source?.source_id || 'source';
  datasets.add(sourceDataset);

  const nodes: LineageGraph['nodes'] = [
    {
      id: sourceId,
      label: `Source • ${sourceDataset}`,
      type: 'dataset',
      recordId: formatRowIdentity(journey.source),
      dataset: sourceDataset,
      timestamp: timestamps.start,
      metadata: {
        source: journey.source,
        row_key: formatRowIdentity(journey.source),
      },
      color: NODE_COLORS.source,
      size: 3,
    },
  ];

  const edges: LineageGraph['edges'] = [];
  let previousNodeId = sourceId;

  steps.forEach((step, index) => {
    const stepNode = createStepNode(step, index);
    nodes.push(stepNode);
    edges.push({
      id: `edge:${previousNodeId}->${stepNode.id}`,
      source: previousNodeId,
      target: stepNode.id,
      label: formatOutcomeLabel(step.outcome),
      operation: step.activity,
      timestamp: step.timestamp,
      metadata: {
        duration_ms: step.duration_ms,
        outcome: step.outcome,
      },
      value: 1,
      color: NODE_COLORS.step,
    });
    previousNodeId = stepNode.id;
  });

  if (journey.destination) {
    const destinationDataset = journey.destination.source_id || 'destination';
    datasets.add(destinationDataset);
    const destinationId = `destination:${formatRowIdentity(journey.destination)}`;

    nodes.push({
      id: destinationId,
      label: `Destination • ${destinationDataset}`,
      type: 'dataset',
      recordId: formatRowIdentity(journey.destination),
      dataset: destinationDataset,
      timestamp: timestamps.end,
      metadata: {
        destination: journey.destination,
        row_key: formatRowIdentity(journey.destination),
      },
      color: NODE_COLORS.destination,
      size: 3,
    });

    edges.push({
      id: `edge:${previousNodeId}->${destinationId}`,
      source: previousNodeId,
      target: destinationId,
      label: 'Loaded',
      operation: 'destination',
      timestamp: timestamps.end,
      value: 1,
      color: NODE_COLORS.destination,
    });
  }

  return {
    nodes,
    edges,
    metadata: {
      totalEvents: steps.length,
      dateRange: timestamps,
      datasets,
      models: new Set<string>(),
    },
  };
}

export function getLatestRunLineageEvents(runLineage: RunLineageResponse | undefined) {
  return [...(runLineage?.events || [])].sort((left, right) =>
    right.timestamp.localeCompare(left.timestamp)
  );
}

export function formatRowEventSummary(event: RowLineageEvent): string {
  const stepId = event.step_id || event.job_id;
  return `${stepId} • ${formatOutcomeLabel(event.outcome)}`;
}
