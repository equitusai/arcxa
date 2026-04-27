# Lineage And Governance

ARCXA treats lineage and governance as part of the platform's core contract, not as an afterthought.

## Use This Guide When

Use this guide when you want to:
- understand what lineage questions the platform can currently answer
- distinguish row, column, schema, and graph-oriented governance surfaces
- understand what is queryable in production versus what exists only for gated test paths
- connect lineage behavior to workflows, RDF projection, and SoS governance

## Table Of Contents

1. [Lineage Areas Exposed Publicly](#lineage-areas-exposed-publicly)
2. [Row-Level And Workflow Lineage](#row-level-and-workflow-lineage)
3. [Column-Level Lineage](#column-level-lineage)
4. [Schema Evolution And Drift](#schema-evolution-and-drift)
5. [Governance And RDF Projection](#governance-and-rdf-projection)
6. [Operational Boundaries](#operational-boundaries)
7. [Relationship To SoS](#relationship-to-sos)

## Lineage Areas Exposed Publicly

The current lineage and governance surface includes:
- record and workflow-run lineage
- row-level lineage and row journey views
- column-level lineage and impact analysis
- schema evolution, schema drift, and migration impact analysis
- governance and SPARQL-oriented RDF queries

Relevant Swagger surfaces:
- `/api/v1/lineage/swagger-ui`
- `/api/v1/field-lineage/swagger-ui`
- `/api/v1/governance/swagger-ui`

## Row-Level And Workflow Lineage

The lineage module exposes routes for:
- record lineage
- record graph views
- run lineage
- time-range lineage queries
- row-key search
- row lineage and row journey views
- batch lineage
- job stats and filtered row views

Representative routes include:
- `GET /api/v1/lineage/record/:record_id`
- `GET /api/v1/lineage/record/:record_id/graph`
- `GET /api/v1/lineage/run/:run_id`
- `POST /api/v1/lineage/time-range`
- `GET /api/v1/lineage/rows/search`
- `GET /api/v1/lineage/row/:row_key`
- `GET /api/v1/lineage/row/:row_key/journey`
- `GET /api/v1/lineage/batch/:batch_id`
- `GET /api/v1/lineage/job/:job_id/stats`
- `GET /api/v1/lineage/job/:job_id/filtered`

That gives operators and auditors more than just a static graph. It gives them a way to ask what happened to a record, a batch, or a workflow run over time.

## Column-Level Lineage

The field-lineage surface exists because row-level lineage alone is not enough when teams need transformation-level impact analysis.

Publicly visible use cases include:
- tracing how one column is derived from another
- viewing column graphs
- impact analysis for downstream columns
- identifying derived columns
- reviewing entity-field history
- conflict resolution and resolved-entity workflows

Representative routes include:
- `GET /api/v1/lineage/column/:table/:column`
- `GET /api/v1/lineage/column/:table/:column/graph`
- `GET /api/v1/lineage/column/:table/:column/derived`
- `POST /api/v1/lineage/column/impact-analysis`
- `GET /api/v1/entities/:entity_id/fields/:field_name/lineage`
- `GET /api/v1/entities/:entity_id/fields/:field_name/history`
- `POST /api/v1/entities/:entity_id/resolved-entity`
- `GET /api/v1/entities/:entity_id/resolved-entity`
- `GET /api/v1/conflicts/requiring-review`

## Schema Evolution And Drift

The lineage API also includes schema-oriented flows for:
- recording schema change events
- saving schema versions
- retrieving the latest schema version
- analyzing schema drift
- evaluating migration impact

Representative routes include:
- `POST /api/v1/lineage/schema/change`
- `GET /api/v1/lineage/schema/datasource/:datasource_id/changes`
- `GET /api/v1/lineage/schema/datasource/:datasource_id/version/latest`
- `GET /api/v1/lineage/schema/drift/:source_version/:target_version`
- `POST /api/v1/lineage/schema/impact`

This is important because lineage is not just about values moving through a pipeline. It is also about understanding how the structure of the data changed over time.

## Governance And RDF Projection

ARCXA's governance surfaces intersect with RDF and SPARQL-oriented storage and query flows.

Relevant governance routes include:
- `POST /api/v1/governance/sparql`
- `GET /api/v1/governance/stats`
- `GET /api/v1/governance/auto-save/stats`
- `POST /api/v1/governance/save`

In practice, that means:
- selected lineage and governance artifacts can be projected into RDF
- graph-native query patterns are available for audit-heavy and provenance-heavy scenarios
- the shard runtime matters here, because that is where RDF persistence and SPARQL execution live

## Operational Boundaries

A few current realities matter:
- the public lineage router includes query surfaces and schema-evolution write surfaces
- special lineage test-write endpoints exist in code, but they are compile-time gated and intentionally excluded from normal production builds
- the repository contains an OpenLineage implementation module, but it is not currently mounted on the main public router
- local runs via `./run-local.sh` disable auth, but secured environments should still be assumed to require auth for `/api/v1/...`

## Relationship To SoS

Systems-of-systems validation extends the same governance thinking into policy and contract-aware integration validation.

The SoS layer now persists:
- validation reports
- validation lineage
- contract and policy references
- approval, evidence, and attestation artifacts

## Related Guides

- [`workflows-and-execution.md`](workflows-and-execution.md)
- [`systems-of-systems.md`](systems-of-systems.md)
- [`architecture.md`](architecture.md)
- [`api-surface.md`](api-surface.md)

