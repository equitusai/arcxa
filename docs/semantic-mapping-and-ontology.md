# Semantic Mapping And Ontology

This guide covers the part of ARCXA that turns source-native structure into something more governed and semantically meaningful.

## Use This Guide When

Use this guide when you want to:
- understand where ontology and mapping fit into the broader platform
- explain unified mapping and manual review workflows
- understand how profiling, R2RML, loaders, and the model service connect
- see where semantics stop being discovery and become executable transformation logic

## Table Of Contents

1. [Why This Layer Exists](#why-this-layer-exists)
2. [Major Capability Areas](#major-capability-areas)
3. [Ontology Management](#ontology-management)
4. [Field Mapping And Manual Review](#field-mapping-and-manual-review)
5. [Unified Mapping](#unified-mapping)
6. [R2RML And Profiling Relationship](#r2rml-and-profiling-relationship)
7. [Model Service Relationship](#model-service-relationship)
8. [Loaders, DDL, And Downstream Targets](#loaders-ddl-and-downstream-targets)

## Why This Layer Exists

Many data platforms stop at schema discovery. ARCXA goes further by adding an ontology-aware mapping layer.

That matters when source fields such as `cust_email`, `acct_num`, or `ship_dt` need to be aligned with shared business or domain concepts instead of remaining trapped in source-specific naming forever.

## Major Capability Areas

| Area | What it does |
| --- | --- |
| Ontology management | Registers and manages ontology assets used by mapping and downstream semantic workflows. |
| Field mapping | Analyzes source fields and proposes semantic alignments. |
| Manual mapping | Supports review, adjustment, and persistent manual overrides. |
| Unified mapping | Consolidates multiple sources into normalized target mappings. |
| R2RML and profiling | Bridges profiled data and RDF generation workflows. |
| Loader and DDL paths | Uses mapping outputs to support downstream database loading and schema-oriented generation flows. |

## Ontology Management

The ontology APIs are the main public surface for:
- registering ontologies
- listing and inspecting ontology assets
- activating and deactivating ontology versions
- validating ontology content
- building mapping sessions and ontology-driven workflows downstream

Relevant routes include:
- `POST /api/v1/ontology`
- `GET /api/v1/ontology`
- `GET /api/v1/ontology/:id`
- `GET /api/v1/ontology/:id/tree`
- `PUT /api/v1/ontology/:id`
- `POST /api/v1/ontology/:id/activate`
- `POST /api/v1/ontology/merge`
- `POST /api/v1/ontology/validate`

Related surfaces also exist for:
- ontology-oriented DDL generation
- SHACL and DDL-adjacent validation and planning paths
- R2RML workflows

## Field Mapping And Manual Review

The mapping surface includes both automatic and review-oriented workflows.

Publicly visible behaviors include:
- schema and field analysis
- candidate suggestion
- feedback recording
- mapping-session review and apply flows
- manual mapping CRUD and bulk import/export
- mapping-health inspection

Representative routes include:
- `POST /api/v1/mapping/analyze`
- `POST /api/v1/mapping/feedback`
- `GET /api/v1/mapping/health`
- `GET /api/v1/mapping/fields/:field_id/candidates`
- `GET /api/v1/mapping/sessions/:session_id`
- `POST /api/v1/mapping/sessions/:session_id/review`
- `POST /api/v1/mapping/sessions/:session_id/apply`
- `POST /api/v1/mapping/manual`
- `GET /api/v1/mapping/manual/:id`
- `GET /api/v1/mapping/manual/export`

The goal is to keep semantic alignment as a governed workflow instead of a private spreadsheet exercise.

## Unified Mapping

Unified mapping is the multi-source consolidation surface.

It supports:
- session creation and update
- AI and heuristic field-similarity suggestion
- conflict resolution
- goal-oriented SQL planning support
- ontology binding coverage and history views
- load orchestration into downstream systems

Relevant API family:
- `/api/v1/mapping`

Publicly visible implementation emphasis in the current codebase includes loader and target patterns for PostgreSQL, DB2, Oracle, and Databricks-oriented workflows.

## R2RML And Profiling Relationship

R2RML is where semantic mapping becomes RDF production logic.

Relevant public family:
- `/api/v1/r2rml`

It is closely related to profiling because:
- profiling produces structured dataset and column metadata
- R2RML flows can consume that metadata or related profiles
- RDF generation depends on both source structure and semantic intent

There is also a separate profiling surface under `/api/v1/profiling` for dataset-profile generation, especially around CSV workflows.

## Model Service Relationship

The model service is optional, but it is especially relevant here.

It is the runtime used when semantic matching needs model-assisted behavior. The rest of the platform does not require it to boot, but semantic-mapping quality and feature depth improve when it is available.

Practical guidance:
- if you are only validating infrastructure and core APIs, you can reason about the platform without centering the model service
- if semantic matching quality is important to your evaluation, include the model service in your local or deployed topology

## Loaders, DDL, And Downstream Targets

Mapping is not the end of the pipeline.

ARCXA includes loader-oriented surfaces for taking the normalized or aligned outputs and writing them into downstream systems. Those flows are where ontology, unified mapping, and workflow execution start to converge.

Related API families:
- `/api/v1/loader`
- `/api/v1/ddl`
- `/api/v1/workflows`

Relevant current capabilities include:
- loader job creation and inspection
- DLQ statistics and row inspection
- DDL generation for multiple target dialects
- ontology-driven DDL and semantic loading workflows

## Related Guides

- [`glossary-and-concepts.md`](glossary-and-concepts.md)
- [`model-service-and-inference.md`](model-service-and-inference.md)
- [`workflows-and-execution.md`](workflows-and-execution.md)
- [`data-sources-and-datasets.md`](data-sources-and-datasets.md)

