# ARCXA

Mapping intelligence for enterprise data migrations: schema mapping, lineage, and transformation traceability that compounds across every project.

ARCXA is a data governance and orchestration platform for teams that need to connect operational data sources, materialize governed datasets, map them into semantic models, and run repeatable transformation or loading workflows with traceable provenance.

One of the main reasons ARCXA exists is enterprise AI governance. In regulated or high-risk environments, multiple teams may be using LLMs, AI agents, model services, and transformation workflows against shared datasets. That creates a hard governance problem: what data was used where, what changed it, which workflow or service touched it, and what downstream systems or teams are now depending on it. ARCXA is built to make those relationships observable instead of implicit.

This public repository combines the Rust backend services and the React frontend used to operate the platform. The repository layout reflects the deployable surface of the system rather than just library internals, so it is suitable as both a codebase and an operational reference.

The codebase is organized for a distributed deployment model:
- `arcxa-coordinator` exposes the REST and gRPC control plane, manages metadata, orchestrates workflows, and routes shard-facing operations.
- `arcxa-shard` is the RDF/SPARQL data plane for graph storage and distributed query execution.
- `arcxa-model-service` provides the optional model-serving path used by semantic matching and ML-oriented workflow steps.
- `frontend/` contains the ARCXA web application.

## Why ARCXA

Enterprise AI programs usually fail governance before they fail modeling. The operational problem is not just storing data or running an LLM call. It is maintaining an auditable understanding of:

- which sources were connected
- which datasets were materialized or transformed
- which workflows changed them
- which mappings or ontology terms were applied
- which models, services, or downstream consumers used the resulting data

ARCXA focuses on that control plane. It gives teams a shared system for cataloging sources, governing transformation flows, materializing datasets, and tracing lineage across those boundaries so that "what changed what" and "what is using what" are answerable questions.

## Core Architecture

ARCXA separates orchestration, graph storage, and model inference into distinct runtime components.

- `Coordinator`
  Owns the control plane. It exposes the authenticated REST API, manages the datasource catalog, workflow definitions, scheduling, import jobs, lineage-oriented metadata, and operational endpoints such as health and metrics. It is also the place where most integration-facing business logic lives.

- `Shards`
  Own the distributed RDF storage layer and SPARQL execution path. They are the graph data plane behind lineage, governance, ontology-linked metadata, and other graph-native workloads.

- `Model Service`
  Provides optional gRPC inference services used for semantic matching and model-assisted workflow behavior. It is intentionally deployed separately so model dependencies and scaling concerns do not contaminate the coordinator runtime.

- `Frontend`
  Provides the operator UI for source onboarding, dataset and entity exploration, ontology work, lineage investigation, workflow design, and settings or administrative operations.

This split matters operationally. The coordinator can evolve independently from the shard storage engine, and the model service can be enabled only where semantic matching or model-backed workflow steps are required.

## What ARCXA Covers

- Data source catalog and connection management for relational, warehouse, file, object, and RDF-style sources.
- Schema discovery, query preview, connector metadata, and per-source capability reporting.
- Dataset import, catalogue browsing, entity views, and materialized dataset handling.
- Workflow authoring, validation, execution, scheduling, execution history, and dataset-backed workflow input.
- Semantic mapping, manual mapping, R2RML, ontology management, ontology-driven DDL, and SHACL/DDL related APIs.
- Lineage APIs covering row, field, model, and graph-native provenance use cases.
- File library and staged file ingestion for CSV and related file workflows.
- Model registry, prediction recording, quality rules, governance/SPARQL operations, GDPR routes, SoS validation, and cluster/ops endpoints.

Taken together, those areas support a common lifecycle:
1. connect a source
2. discover or inspect schema
3. import or materialize governed data
4. map source fields to semantic terms
5. run transformation or loading workflows
6. inspect resulting datasets, entities, and lineage
7. operate the platform through health, metrics, cluster, and admin surfaces

## Functional Areas

- `Data Sources`
  Registers and manages source connections, tests connectivity, infers schema, previews queries, and exposes datasource capabilities for UI and workflow gating.

- `Catalogue, Datasets, and Entities`
  Supports dataset import, catalogue browsing, dataset detail inspection, and entity-centric exploration of governed data.

- `Ontology and Semantic Mapping`
  Manages ontologies, mapping sessions, manual mapping workflows, R2RML, and ontology-driven schema or DDL generation.

- `Workflow Orchestration`
  Supports workflow CRUD, validation, dry-run, synchronous and asynchronous execution, scheduling, execution history, progress, cancellation, and materialized dataset handoff.

- `Lineage and Governance`
  Exposes row lineage, field lineage, lineage query APIs, graph-native governance endpoints, and SPARQL-oriented metadata access.

- `File and Bulk Ingestion`
  Provides file library APIs, CSV-oriented ingest utilities, loader APIs, and multi-source mapping flows.

- `Models, Quality, and Operations`
  Includes model registry endpoints, prediction recording, quality rules, health/readiness/metrics, audit paths, cluster admin routes, and WAL/temporal admin surfaces.

## End-to-End Usage Model

ARCXA is easiest to understand as a pipeline from source registration to governed outputs.

### 1. Source onboarding

Users register a datasource through the catalog API or the frontend. The coordinator stores the normalized connection model, validates connector compatibility, and exposes capability metadata so the UI and workflow engine know whether that source can be queried, inferred, read by workflows, or written to by loaders.

### 2. Discovery and preview

For supported source types, ARCXA can infer schema, preview queries, and expose connector metadata before data is imported. This is the stage where operators decide whether a source should remain query-only, feed the catalogue, or become a workflow input.

### 3. Dataset materialization

Datasets can be imported and materialized into governed storage, then surfaced in the catalogue and dataset detail views. That creates a cleaner handoff between raw sources and downstream workflow execution.

### 4. Semantic alignment

Ontologies, mapping sessions, manual mapping, and R2RML-related APIs provide the semantic layer. This is where source-native names and structures are aligned to domain terms so downstream consumption is not forced to remain source-specific.

### 5. Workflow execution

Workflows can read from datasources or materialized datasets, execute transformation and loading steps, and emit outputs such as loaded tables, RDF-oriented results, exported files, or additional materialized datasets depending on the configured flow.

### 6. Lineage and governance

Once data has moved through the system, ARCXA exposes lineage and governance views so operators can trace what happened, which workflow or mapping session was involved, and how governed entities or datasets relate to their originating sources.

That is the point where the platform becomes especially relevant for AI-heavy environments. When teams are training, enriching, validating, or operationalizing data through model-backed services and automated agents, ARCXA is intended to preserve the chain of custody across those steps rather than leaving it scattered across logs, notebooks, and one-off pipelines.

## Supported Source Classes

The connector registry in `arcxa-core` currently includes:
- Relational and warehouse sources: PostgreSQL, MySQL, Oracle, DB2, SAP HANA, Snowflake, Databricks
- File and object sources: CSV, S3 Parquet
- Semantic source: RDF N-Triples

Connector parity is intentionally not described as uniform. Read, write, inference, parameter, workflow, and cancellation support varies by connector and operation. Use the live connector registry and datasource capability responses instead of assuming every source supports every path:
- `GET /api/v1/connectors`
- `GET /api/v1/datasources`

In practice, the connector registry should be treated as the authoritative contract for front-end behavior and workflow eligibility. The platform does not assume all connectors are symmetrical.

## API Surface

The coordinator exposes versioned REST APIs under `/api/v1` plus health and auth entry points.

Key API areas:
- `/api/v1/datasources`
- `/api/v1/workflows`
- `/api/v1/ontology`
- `/api/v1/governance`
- `/api/v1/lineage`
- `/api/v1/field-lineage`
- `/api/v1/file-library`
- `/api/v1/loader`
- `/api/v1/r2rml`
- `/api/v1/mapping`
- `/api/v1/gdpr`
- `/api/v1/connectors`
- `/api/v1/datasets`
- `/api/v1/entities`

The OpenAPI index is exposed at:

```text
GET /openapi.yaml
```

Module-specific Swagger UIs are mounted under the versioned API, for example:
- `/api/v1/datasources/swagger-ui`
- `/api/v1/workflows/swagger-ui`
- `/api/v1/ontology/swagger-ui`
- `/api/v1/governance/swagger-ui`
- `/api/v1/lineage/swagger-ui`
- `/api/v1/file-library/swagger-ui`

The API surface is intentionally modular rather than a single monolith. The code under `arcxa-coordinator/src/api` is split by business area, and the OpenAPI index points to module-specific documentation rather than collapsing everything into one giant spec.

## Frontend

The React frontend lives in `/frontend` and provides the main operating interface for:
- dashboard and status views
- data catalogue and dataset detail
- data sources
- entities
- file library
- models
- lineage
- fusion
- workflow design and execution
- ontologies
- SPARQL playground
- admin settings

Frontend routes are implemented in `frontend/src/App.tsx`, and the UI is branded as `ARCXA`.

The frontend is not just a thin API shell. It contains dedicated operating surfaces for datasource management, dataset and catalogue views, workflow design and execution, ontology work, lineage exploration, and related admin tooling.

## Repository Layout

```text
/
├── arcxa-cli/
├── arcxa-coordinator/
├── arcxa-core/
├── arcxa-migrations/
├── arcxa-model-service/
├── arcxa-shard/
├── frontend/
├── docker-compose.yml
├── build.sh
├── run-local.sh
└── run-local-ha.sh
```

Notes:
- The root Cargo workspace includes `arcxa-core`, `arcxa-coordinator`, `arcxa-model-service`, `arcxa-migrations`, and `arcxa-cli`.
- `arcxa-shard` is built separately because of the RocksDB dependency split between shard storage and the rest of the workspace.

`arcxa-core` contains the shared contracts, workflow engine pieces, connector abstractions, and schema or catalog types used across the rest of the system.

## Local Development

Toolchain requirement:
- Rust `1.91.1` or newer is required for the current AWS SDK dependency set.
- The repo includes `rust-toolchain.toml` pinned to `1.91.1` so `rustup` can select the right toolchain automatically.

Build the backend components:

```bash
./build.sh
```

Run the default local topology with Docker-backed infrastructure and local binaries:

```bash
./run-local.sh
```

Run the HA-oriented local topology:

```bash
./run-local-ha.sh
```

Build the frontend:

```bash
cd frontend
npm install
npm run build
```

Run the frontend in development mode:

```bash
cd frontend
npm run dev
```

If you want to explore the API without the frontend first, start the coordinator and use the OpenAPI index plus module-specific Swagger UIs under `/api/v1/*/swagger-ui`.

## Deployment and Operations

- Dockerfiles and `docker-compose*.yml` are included at the repository root.
- A Helm chart is included under `kubernetes/helm-chart`.
- Health endpoints are available at `/health`, `/health/live`, and `/health/ready`.
- Metrics are exposed from the coordinator at `/metrics` behind authentication.

For local and test environments, the repository also includes helper scripts such as `build.sh`, `run-local.sh`, `run-local-ha.sh`, and `test.sh`. Those scripts are the intended entry points for the public repo layout.

## What This Repository Is

This repository is the deployable product surface for ARCXA:
- Rust services and shared crates
- the React frontend
- local orchestration scripts
- Docker and compose assets
- Helm packaging
- demo and vendor artifacts that are part of the runnable product tree

It is not intended to claim that every subsystem has identical maturity across every source type or deployment mode. The codebase has a broad feature surface, and some areas are connector-specific, feature-gated, or optional by design.

## Accuracy Notes

This README is intentionally conservative. ARCXA has a broad surface area, but some subsystems are source-specific or feature-flagged:
- connector capabilities vary by source and operation
- some workflow and loader paths are only valid for specific source classes
- optional features such as ODBC, cryptographic audit, and raft-backed coordination are build- or deployment-dependent
- the model service is optional and primarily relevant for semantic matching and model-backed workflow behavior

For exact request and response contracts, use the live OpenAPI documents and the source modules under `arcxa-coordinator/src/api`.
