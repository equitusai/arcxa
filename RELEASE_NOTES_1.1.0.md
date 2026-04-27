# ARCXA 1.1.0 Release Notes

## Overview

ARCXA 1.1.0 is the next public product release of the platform repository. This release deepens the systems-of-systems governance surface, adds stronger operator workflows across the CLI and UI, improves recovery and runtime hardening, and refreshes the public documentation into a curated, maintainable structure.

Versioning note:
- `1.1.0` is the product release designation for the public repository and release notes.
- Internal crate and package versions remain on their own version lines where that keeps the workspace stable.

## Release Highlights

- Systems-of-systems validation matured into a revisioned governance control plane with persisted history, analytics, lifecycle states, and approval workflows.
- Contracts and policies now support approval requests, evidence capture, attestation metadata, and revision-aware audit trails.
- Operator surfaces expanded across the coordinator API, CLI, and frontend for reconcile, audit, signing-key inspection, and governance review.
- Recovery and projection behavior is better hardened with explicit reconcile controls, retention-aware replay coverage, and startup recovery testing.
- Public-facing documentation has been reorganized into a curated documentation hub with focused sub-guides and a refreshed repository landing page.

## Included in 1.1.0

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

This release expands the operator-facing UI with richer systems-of-systems surfaces for:
- catalog navigation
- reports and lineage summaries
- governance review
- operations and maintenance controls

### Operational and documentation assets

The release also includes:
- local build and run scripts
- Dockerfiles and compose manifests
- Kubernetes packaging
- curated public documentation under `docs/`
- the shared architecture diagram and public mirror sync workflow

## Functional Scope

ARCXA 1.1.0 publicly exposes the following major capability areas.

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
- graph-native provenance and validation history
- systems-of-systems catalog, analytics, and governance workflows
- approval, evidence, signature, and attestation-oriented audit paths

### Models, quality, and operations

- model registry and prediction endpoints
- quality rule surfaces
- health, readiness, and metrics endpoints
- cluster and administrative routes
- CLI operator workflows and deployment packaging

## Systems-Of-Systems Focus In 1.1.0

This release significantly expands the systems-of-systems layer with:

- interface compatibility validation and dry-run flows
- persisted validation reports, lineage, and retention controls
- compatibility matrix, dependency graph, and what-if analytics with deterministic truncation metadata
- revisioned contract governance with approval requests, evidence, and signature history
- revisioned policy governance with approval requests, evidence, lifecycle state, revision pinning, and attestation metadata
- CLI and UI operator surfaces for reconcile, audit review, and signing-key status/rotation

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
- `/api/v1/sos`

The OpenAPI index is available at:

```text
/openapi.yaml
```

Module-specific Swagger UIs are exposed under the versioned API.

## Architecture Summary

ARCXA 1.1.0 ships as a multi-component system:

- `Coordinator`
  The main control plane and API entry point. It owns orchestration, catalog management, workflow APIs, scheduling, validation services, and operational endpoints.

- `Shards`
  The distributed graph data plane for RDF/SPARQL-backed storage and execution.

- `Model Service`
  An optional inference service used for semantic matching and model-assisted behavior.

- `Frontend`
  The main operator experience for the platform.

This separation allows the platform to scale and deploy operationally by concern instead of collapsing all runtime responsibilities into a single process.

## Public Repository Changes

This release further improves the public repository presentation:

- a premium, friendlier root README with architecture imagery
- curated public docs split into focused guides
- public sync now carries docs and shared assets intentionally
- systems-of-systems operator workflows are visible across backend, CLI, and UI surfaces

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
- Internal crate/package versions are not all pinned to `1.1.0`.
- Connector maturity varies by source type and operation; do not infer full parity from connector presence alone.
- Some platform features are deployment-dependent or feature-gated, including ODBC-related paths, optional audit capabilities, and raft-related coordination.

## Recommended Evaluation Path

For first-time evaluators of the 1.1.0 release:

1. Read `README.md` for the repository-level overview.
2. Start with `docs/README.md` for the curated documentation hub.
3. Run the platform locally with `./run-local.sh`.
4. Inspect the API index at `/openapi.yaml`.
5. Use the frontend under `/frontend` for datasource, catalogue, workflow, lineage, and systems-of-systems governance exploration.

## Closing

ARCXA 1.1.0 is a stronger operator and governance release. It makes the platform easier to evaluate publicly, easier to operate, and better aligned with environments where data movement, semantic alignment, validation, and auditability have to work together.
