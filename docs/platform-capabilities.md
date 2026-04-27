# Platform Capabilities

This guide is the capability atlas for ARCXA. It is intentionally broad: it shows what the platform covers, where the major seams are, and which focused guide to read next for each domain.

## Use This Guide When

Use this guide when you want to:
- explain the platform to someone who is not starting from the code
- see how the major product areas fit together
- understand where ARCXA is opinionated compared with a simple connector or workflow tool
- choose which focused guide to read next

## Table Of Contents

1. [Capability Map](#capability-map)
2. [What ARCXA Is Opinionated About](#what-arcxa-is-opinionated-about)
3. [Data Sources And Managed Data](#data-sources-and-managed-data)
4. [Semantic Mapping And Ontology](#semantic-mapping-and-ontology)
5. [Model-Assisted Inference](#model-assisted-inference)
6. [Workflows And Execution](#workflows-and-execution)
7. [Lineage And Governance](#lineage-and-governance)
8. [Systems-Of-Systems](#systems-of-systems)
9. [Operator And Automation Experience](#operator-and-automation-experience)

## Capability Map

| Area | What ARCXA provides | Focused guide |
| --- | --- | --- |
| Data sources and managed data | Connector registration, capability inspection, schema discovery, datasource discovery jobs, file library, dataset import, catalogue, and entities | [`data-sources-and-datasets.md`](data-sources-and-datasets.md) |
| Semantic mapping and ontology | Ontology registration, field mapping, unified multi-source mapping, loader handoffs, and optional model-assisted matching | [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md) |
| Model-assisted inference | Optional embedding and inference runtime for semantic-matching-heavy workflows | [`model-service-and-inference.md`](model-service-and-inference.md) |
| Workflows and execution | Workflow CRUD, validation, dry-run, execution, schedules, approvals, progress tracking, and stream-oriented controls | [`workflows-and-execution.md`](workflows-and-execution.md) |
| Lineage and governance | Row, column, workflow, and schema-evolution lineage plus governance and provenance APIs | [`lineage-and-governance.md`](lineage-and-governance.md) |
| Systems-of-systems | Systems, interfaces, contracts, policies, persisted validation reports, analytics, reconcile, and governance workflows | [`systems-of-systems.md`](systems-of-systems.md) |
| Operator and automation experience | Web UI, CLI, Python client, modular Swagger surfaces, Docker and Kubernetes entry points, health, metrics, and local scripts | [`frontend-and-cli.md`](frontend-and-cli.md), [`sdk-and-automation.md`](sdk-and-automation.md), [`deployment-and-operations.md`](deployment-and-operations.md), [`api-surface.md`](api-surface.md) |

## What ARCXA Is Opinionated About

ARCXA is not just a connector layer or a workflow runner. Its strongest opinions are:
- data movement should stay connected to lineage and governance
- semantic mapping should be treated as a managed workflow, not only a one-off integration task
- interoperability validation should be persisted and auditable, not only computed transiently
- graph-native governance matters enough to justify a dedicated shard runtime

Those choices explain why the platform looks broader than a traditional ETL point solution.

## Data Sources And Managed Data

ARCXA is not just a connector registry. It also provides a managed surface above those connectors.

Major capabilities include:
- registering data sources with typed connector metadata
- testing connectivity with connector-specific logic using stored datasource credentials
- discovering schemas and sample data through async discovery jobs
- using the file library for managed file-based inputs
- importing datasets into the catalogue
- exposing dataset and entity views for downstream workflows and operators

Practical boundary:
- connector presence does not mean identical capability depth across all source types
- ODBC-backed connectors such as Oracle, DB2, and SAP HANA require the `odbc` build feature and local driver support

## Semantic Mapping And Ontology

ARCXA includes an ontology-aware mapping layer instead of leaving semantic alignment entirely to downstream consumers.

Publicly visible capability areas include:
- ontology registration and management
- field mapping analysis and review flows
- manual mapping support
- unified mapping sessions for multi-source normalization
- ontology-driven DDL and related loader paths
- R2RML mapping support and profile-assisted RDF generation

## Model-Assisted Inference

The model service is optional, but it meaningfully improves the semantic side of the platform.

It provides:
- embedding generation for semantic-matching workflows
- inference isolation from the coordinator runtime
- a cleaner scaling boundary for model-heavy workloads

This matters most for teams evaluating semantic quality or planning a production deployment where inference capacity should scale independently.

## Workflows And Execution

The workflow surface is larger than a simple CRUD API.

It includes:
- workflow registration, listing, details, update, and delete
- validation, dry-run, and test-step endpoints
- synchronous and asynchronous execution
- execution history, progress, pause, resume, stop, and abort flows
- schedules and schedule preview
- route and stream-oriented workflow surfaces
- approval workflows for executions that need human gating
- dedicated SoS validation workflow support inside the shared workflow engine

## Lineage And Governance

ARCXA treats lineage and provenance as first-class concerns.

Major capabilities include:
- row-level lineage and journey views
- workflow run and record lineage
- column-level dependency and impact analysis
- schema change, drift, and migration impact tracking
- governance and SPARQL-oriented metadata access
- graph-oriented provenance projection for audit-heavy use cases

## Systems-Of-Systems

ARCXA also includes a policy and contract-aware systems-of-systems control layer.

That layer provides:
- a catalog of systems, interfaces, contracts, and policies
- persisted validation reports and report lineage
- compatibility matrix, dependency graph, and what-if analytics
- revisioned governance workflows for contracts and policies
- operator controls for reconcile, signing-key status and rotation, and governance audit

## Operator And Automation Experience

The operator-facing experience is intentionally multi-surface:
- REST and modular Swagger UIs for direct API use
- React frontend pages for day-to-day operator workflows
- CLI utilities for authenticated maintenance and audit scenarios
- a Python client for selected automation paths
- local scripts, Docker assets, and Kubernetes assets for packaging and deployment

## Related Guides

- [`glossary-and-concepts.md`](glossary-and-concepts.md)
- [`api-surface.md`](api-surface.md)
- [`architecture.md`](architecture.md)
- [`frontend-and-cli.md`](frontend-and-cli.md)

