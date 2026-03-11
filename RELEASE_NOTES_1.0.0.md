# ARCXA 1.0.0 Release Notes

## Overview

ARCXA 1.0.0 is the first public product release of the platform repository. It packages the backend runtime, frontend application, local orchestration scripts, container assets, and Helm packaging into a single deployable public codebase.

This release establishes the public `ARCXA` product surface while preserving the existing internal crate structure and most internal library naming. The result is a cleaner external repo without forcing a disruptive internal rewrite.

Versioning note:
- `1.0.0` is the product release designation for the public repository and release notes.
- Some internal crate and package versions in the workspace remain on their own version lines.

## Release Highlights

- Public repository packaging for the ARCXA backend and frontend in one repo.
- Product-facing rebrand from Graphica to ARCXA across the visible runtime and UI surface.
- Curated public repo layout with `arcxa-*` service directories and `/frontend` for the web application.
- Data source, catalogue, ontology, lineage, workflow, and governance surfaces all represented in the public codebase.
- Public sync workflow for maintaining GitHub as a clean mirror of the private development repos.

## Included in 1.0.0

### Backend runtime surface

The repository includes the Rust service and crate layout that powers ARCXA:
- `arcxa-coordinator`
- `arcxa-shard`
- `arcxa-model-service`
- `arcxa-core`
- `arcxa-cli`
- `arcxa-migrations`

### Frontend surface

The repository includes the ARCXA React frontend in:
- `frontend/`

The UI surface includes:
- dashboard views
- data sources
- data catalogue and dataset detail
- entities
- file library
- models
- lineage
- fusion
- workflow design and execution
- ontologies
- SPARQL playground
- admin settings

### Operational assets

The release also includes:
- local build and run scripts
- Dockerfiles and compose manifests
- Kubernetes Helm packaging
- demo and vendor assets that are part of the deployable tree

## Functional Scope

ARCXA 1.0.0 publicly exposes the following major capability areas.

### Data sources and discovery

- datasource registration and CRUD
- connection testing
- schema inference and discovery
- query preview and execution paths
- connector registry and capability metadata

### Datasets, catalogue, and entities

- dataset import and import tracking
- catalogue browsing
- dataset detail inspection
- entity browsing and related metadata exploration

### Ontology and semantic mapping

- ontology registration and management
- mapping sessions and review flows
- manual mapping APIs
- R2RML-related APIs
- ontology-driven DDL and SHACL/DDL related surfaces

### Workflow orchestration

- workflow create, read, update, delete
- validation, dry-run, and execution
- scheduling
- execution history, progress, and cancellation
- datasource-backed and dataset-backed workflow input paths

### Lineage and governance

- lineage APIs
- field-lineage APIs
- governance and SPARQL-oriented API surface
- graph-native metadata and provenance-oriented workflows

### Models, quality, and operations

- model registry and prediction endpoints
- quality rule surfaces
- health, readiness, and metrics endpoints
- cluster and administrative routes
- operational scripts and deployment packaging

## Supported Source Classes

The connector registry in this release includes support for:

- Relational and warehouse sources
  PostgreSQL, MySQL, Oracle, DB2, SAP HANA, Snowflake, Databricks

- File and object sources
  CSV, S3 Parquet

- Semantic sources
  RDF N-Triples

Important:
- Connector support is not identical across all source types.
- Query, inference, workflow-read, workflow-write, parameter, and cancellation behavior varies by connector and operation.
- Live datasource capabilities should be treated as the authoritative runtime contract.

## API Surface

The coordinator exposes authenticated versioned APIs under `/api/v1`, along with health and auth entry points.

Major API areas in this release:
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

The OpenAPI index is available at:

```text
/openapi.yaml
```

Module-specific Swagger UIs are exposed under the versioned API.

## Architecture Summary

ARCXA 1.0.0 ships as a multi-component system:

- `Coordinator`
  The main control plane and API entry point. It owns orchestration, catalog management, workflow APIs, scheduling, and operational endpoints.

- `Shards`
  The distributed graph data plane for RDF/SPARQL-backed storage and execution.

- `Model Service`
  An optional inference service used for semantic matching and model-assisted behavior.

- `Frontend`
  The main operator experience for the platform.

This separation allows the platform to scale and deploy operationally by concern instead of collapsing all runtime responsibilities into a single process.

## Public Repository Changes

This release also formalizes the public repository presentation:

- public-facing package and runtime names use `arcxa-*`
- the frontend is packaged under `/frontend`
- top-level README and release notes are curated for public consumption
- the public repo is maintained through a one-way sync process from the private development repos

## Build and Local Run Paths

The intended entry points for working with this release are:

```bash
./build.sh
./run-local.sh
./run-local-ha.sh
./test.sh
```

Frontend:

```bash
cd frontend
npm install
npm run build
```

## Known Release Notes Caveats

- Public product naming is standardized as `ARCXA`, but some internal code references still retain historical naming where that avoids destabilizing internal crate or library boundaries.
- Internal crate/package versions are not all pinned to `1.0.0`.
- Connector maturity varies by source type and operation; do not infer full parity from connector presence alone.
- Some platform features are deployment-dependent or feature-gated, including ODBC-related paths, optional audit capabilities, and raft-related coordination.

## Recommended Evaluation Path

For first-time evaluators of the 1.0.0 release:

1. Read `README.md` for the repository-level overview.
2. Start the platform locally with `./run-local.sh`.
3. Inspect the API index at `/openapi.yaml`.
4. Use the frontend under `/frontend` for datasource, catalogue, workflow, and lineage exploration.
5. Treat datasource capability responses and module-specific Swagger UIs as the precise contract for source-specific behavior.

## Closing

ARCXA 1.0.0 marks the first public packaging of the platform as a coherent product repository with a stable external name, curated documentation surface, and synchronized backend/frontend distribution story.
