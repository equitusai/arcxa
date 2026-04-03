import { describe, expect, it } from 'vitest';

import {
  buildRowJourneyGraph,
  formatOutcomeLabel,
  formatRowIdentity,
} from './workflow-lineage-adapter';

describe('workflow lineage adapter', () => {
  it('formats Oracle row identities into stable row keys', () => {
    expect(
      formatRowIdentity({
        source_type: {
          Database: 'Oracle',
        },
        source_id: 'CUSTOMER_FEED',
        position: {
          PrimaryKey: {
            STAGE_ROW_ID: 'FEED001',
          },
        },
      })
    ).toBe('oracle:CUSTOMER_FEED:STAGE_ROW_ID=FEED001');
  });

  it('builds a lineage graph from row journey responses', () => {
    const graph = buildRowJourneyGraph({
      source: {
        source_type: { Database: 'Oracle' },
        source_id: 'CUSTOMER_FEED',
        position: {
          PrimaryKey: {
            STAGE_ROW_ID: 'FEED001',
          },
        },
      },
      steps: [
        {
          activity: 'deduplication in batch_123',
          timestamp: '2026-04-02T19:20:55.054541303Z',
          duration_ms: 0,
          outcome: {
            Processed: {
              output_location: 'deduplication_kept',
            },
          },
        },
        {
          activity: 'db_load in batch_123',
          timestamp: '2026-04-02T19:21:20.054541303Z',
          duration_ms: 25000,
          outcome: {
            Processed: {
              output_location: 'DB2INST1.CUSTOMER_FEED_CURATED',
            },
          },
        },
      ],
      destination: {
        source_type: { Database: 'DB2' },
        source_id: 'CUSTOMER_FEED_CURATED',
        position: {
          PrimaryKey: {
            STAGE_ROW_ID: 'FEED001',
          },
        },
      },
      total_duration_ms: 25000,
    });

    expect(graph.nodes).toHaveLength(4);
    expect(graph.edges).toHaveLength(3);
    expect(graph.nodes[0].label).toContain('Source');
    expect(graph.nodes[3].label).toContain('Destination');
    expect(graph.metadata.totalEvents).toBe(2);
  });

  it('formats workflow outcomes for display', () => {
    expect(
      formatOutcomeLabel({
        Processed: {
          output_location: 'deduplication_kept',
        },
      })
    ).toBe('Processed to deduplication_kept');
  });
});
