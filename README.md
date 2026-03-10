# ARCXA

ARCXA is a data governance and orchestration platform built around a graph-native metadata layer, workflow execution, lineage tracking, and source-to-semantic mapping. This public repository combines the Rust backend services and the React frontend used to operate the platform.

The codebase is organized for a distributed deployment model:
- `arcxa-coordinator` exposes the REST and gRPC control plane, manages metadata, orchestrates workflows, and routes shard-facing operations.
- `arcxa-shard` is the RDF/SPARQL data plane for graph storage and distributed query execution.
- `arcxa-model-service` provides the optional model-serving path used by semantic matching and ML-oriented workflow steps.
- `frontend/` contains the ARCXA web application.

## What ARCXA Covers

- Data source catalog and connection management for relational, warehouse, file, object, and RDF-style sources.
- Schema discovery, query preview, connector metadata, and per-source capability reporting.
- Dataset import, catalogue browsing, entity views, and materialized dataset handling.
- Workflow authoring, validation, execution, scheduling, execution history, and dataset-backed workflow input.
- Semantic mapping, manual mapping, R2RML, ontology management, ontology-driven DDL, and SHACL/DDL related APIs.
- Lineage APIs covering row, field, model, and graph-native provenance use cases.
- File library and staged file ingestion for CSV and related file workflows.
- Model registry, prediction recording, quality rules, governance/SPARQL operations, GDPR routes, SoS validation, and cluster/ops endpoints.

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

## Supported Source Classes

The connector registry in `arcxa-core` currently includes:
- Relational and warehouse sources: PostgreSQL, MySQL, Oracle, DB2, SAP HANA, Snowflake, Databricks
- File and object sources: CSV, S3 Parquet
- Semantic source: RDF N-Triples

Connector parity is intentionally not described as uniform. Read, write, inference, parameter, workflow, and cancellation support varies by connector and operation. Use the live connector registry and datasource capability responses instead of assuming every source supports every path:
- `GET /api/v1/connectors`
- `GET /api/v1/datasources`

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

## Local Development

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

## Deployment and Operations

- Dockerfiles and `docker-compose*.yml` are included at the repository root.
- A Helm chart is included under `kubernetes/helm-chart`.
- Health endpoints are available at `/health`, `/health/live`, and `/health/ready`.
- Metrics are exposed from the coordinator at `/metrics` behind authentication.

## Accuracy Notes

This README is intentionally conservative. ARCXA has a broad surface area, but some subsystems are source-specific or feature-flagged:
- connector capabilities vary by source and operation
- some workflow and loader paths are only valid for specific source classes
- optional features such as ODBC, cryptographic audit, and raft-backed coordination are build- or deployment-dependent
- the model service is optional and primarily relevant for semantic matching and model-backed workflow behavior

For exact request and response contracts, use the live OpenAPI documents and the source modules under `arcxa-coordinator/src/api`.
