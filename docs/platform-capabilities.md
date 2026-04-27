# Platform Capabilities

This guide summarizes the major public-facing capability areas in ARCXA.

## Data Source Catalog

ARCXA can register and manage a broad set of source classes, including relational systems, warehouses, files, and RDF-style inputs.

Core capabilities include:
- connection registration and capability reporting
- connector-aware connectivity checks
- schema discovery and preview-oriented APIs
- normalized source metadata for downstream workflows and UI gating

## Governed Datasets And Catalog Views

ARCXA is not limited to one-off connector access. It can materialize datasets into a governed surface that becomes easier to reason about than raw upstream systems.

This is the bridge between:
- raw operational sources
- downstream workflows
- audited and explorable governed data

## Ontology And Semantic Mapping

A major differentiator in ARCXA is the semantic alignment layer.

Capabilities include:
- ontology management
- mapping sessions and manual review
- ontology-linked field alignment
- R2RML-related workflows
- ontology-driven DDL and related generation paths

The intent is simple: downstream use should not stay trapped in source-native column names forever.

## Workflow Orchestration

ARCXA supports declarative workflows that can:
- read from datasources or materialized datasets
- apply transformation logic
- validate and enrich records
- load results into downstream stores
- persist execution history and operator-visible status

This gives teams a repeatable path from source onboarding to governed output delivery.

## Lineage And Provenance

Lineage is a first-class part of the platform rather than a sidecar afterthought.

Publicly visible capability areas include:
- row-level lineage
- field-level lineage
- workflow-to-data provenance
- graph-native lineage and governance queries
- validation and audit-oriented histories

## Systems Of Systems Validation

ARCXA now includes a dedicated systems-of-systems layer for integration governance.

This covers:
- system and interface registration
- contract and policy modeling
- persisted validation reports and report lineage
- compatibility matrix, dependency graph, and what-if analytics
- approval, evidence, and attestation-oriented governance flows

Read [`systems-of-systems.md`](systems-of-systems.md) for the focused breakdown.

## File And Bulk Ingestion

The platform also includes file and loader-oriented surfaces such as:
- file library APIs
- CSV and bulk-oriented ingestion paths
- loader and transformation handoffs
- multi-source mapping and loading workflows

## Operational Surface

ARCXA ships the supporting runtime surfaces teams need in real environments:
- health and readiness endpoints
- metrics
- CLI operator utilities
- Docker and Kubernetes packaging
- public mirror sync support for the curated documentation and source surface

## A Practical Reading Order

If you are still orienting yourself:
1. Read [`architecture.md`](architecture.md)
2. Read [`getting-started.md`](getting-started.md)
3. Read [`systems-of-systems.md`](systems-of-systems.md) if validation governance is core to your use case
