# ARCXA

<p align="center">
  <a href="https://github.com/equitusai/arcxa"><img src="https://img.shields.io/badge/GitHub-equitusai%2Farcxa-181717?logo=github" alt="GitHub repository" /></a>
  <img src="https://img.shields.io/badge/Rust-1.91%2B-000000?logo=rust" alt="Rust 1.91+" />
  <img src="https://img.shields.io/badge/Deploy-Docker%20%26%20Kubernetes-2496ED?logo=docker&logoColor=white" alt="Docker and Kubernetes" />
  <img src="https://img.shields.io/badge/UI-React%20%2B%20Vite-61DAFB?logo=react&logoColor=061A23" alt="React and Vite" />
  <a href="./LICENSE.md"><img src="https://img.shields.io/badge/License-BSL%201.1-EA580C" alt="BSL 1.1 license" /></a>
</p>

<p align="center"><strong>Governed data movement, semantic mapping, lineage, and systems-of-systems validation in one platform.</strong></p>

<p align="center">
  ARCXA helps teams connect operational data sources, materialize governed datasets, orchestrate repeatable workflows,
  preserve lineage, and apply policy-driven validation without stitching together five separate control planes.
</p>

<p align="center">
  <img src="./assets/arcxa-arch.png" alt="ARCXA architecture diagram" width="100%" />
</p>

> Welcome to the curated public repository. Historical internal working notes are intentionally not mirrored here. The maintained documentation set for external users starts in `docs/README.md`.

## Table Of Contents

1. [Why ARCXA](#why-arcxa)
2. [Platform Snapshot](#platform-snapshot)
3. [Architecture At A Glance](#architecture-at-a-glance)
4. [Quick Start](#quick-start)
5. [Documentation](#documentation)
6. [Repository Map](#repository-map)
7. [License](#license)

## Why ARCXA

Most data platforms can move data. Fewer can explain, with confidence, what changed, why it changed, which workflow touched it, which ontology terms were applied, which policies were in force, and what downstream systems are now depending on it.

ARCXA is designed for that second problem.

It combines:
- source onboarding and schema discovery
- governed dataset materialization
- ontology-aware semantic mapping
- workflow orchestration and loading
- row, field, and graph-native lineage
- policy and contract validation for systems-of-systems integrations

## Platform Snapshot

| Area | What it does |
| --- | --- |
| Data Sources | Registers relational, warehouse, file, and RDF sources; tests connectivity; exposes source capabilities. |
| Semantic Mapping | Aligns source fields with ontology terms through statistical and model-assisted matching workflows. |
| Workflows | Runs declarative ETL and loading pipelines with validation, scheduling, and execution history. |
| Lineage & Governance | Tracks row, field, workflow, and graph lineage with audit-friendly provenance. |
| Systems Of Systems | Models systems, interfaces, contracts, and policies with persisted validation history and analytics. |
| Operations | Ships health, metrics, Docker, Kubernetes, CLI, and operator-oriented maintenance surfaces. |

## Architecture At A Glance

ARCXA is intentionally split into deployable components rather than one oversized runtime.

| Component | Role |
| --- | --- |
| `arcxa-coordinator` | Control plane: authenticated APIs, metadata, workflows, validation services, and orchestration. |
| `arcxa-shard` | RDF/SPARQL data plane for graph storage and distributed query execution. |
| `arcxa-model-service` | Optional model inference path for semantic matching and model-assisted workflow behavior. |
| `arcxa-cli` | Thin operator tooling over coordinator APIs and migration/runtime utilities. |
| `arcxa-core` | Shared contracts, workflow primitives, connector abstractions, and cross-cutting domain types. |
| `frontend/` | React and Vite web application for operators, analysts, and platform admins. |

A few important implementation realities:
- the shard is built separately because it depends on `oxigraph` and an older RocksDB binding than the rest of the workspace
- the coordinator owns orchestration, policy evaluation, and API composition, but it does not store RDF data directly
- the model service is optional by design so teams can deploy semantic matching only where it adds value

## Quick Start

### 1. Build the backend workspace

```bash
./build.sh
```

### 2. Run the default local topology

```bash
./run-local.sh
```

### 3. Verify the coordinator

```bash
curl http://localhost:8080/health
curl http://localhost:8080/openapi.yaml
```

### 4. Run the frontend

```bash
cd frontend
npm install
npm run dev
```

### 5. Explore the curated docs

Start with `docs/README.md`, then follow the guide that matches your role.

## Documentation

The documentation set below is the maintained public surface for this repository.

| Guide | Best for | Covers |
| --- | --- | --- |
| [`docs/README.md`](docs/README.md) | Everyone | Documentation hub, reading paths, and guide map. |
| [`docs/getting-started.md`](docs/getting-started.md) | First-time users | Local prerequisites, build, run, and first verification steps. |
| [`docs/architecture.md`](docs/architecture.md) | Architects and platform leads | Runtime topology, control-plane/data-plane split, and data flow model. |
| [`docs/platform-capabilities.md`](docs/platform-capabilities.md) | Product and delivery teams | What the platform covers across sources, mapping, workflows, lineage, and governance. |
| [`docs/systems-of-systems.md`](docs/systems-of-systems.md) | Integration and governance teams | SoS catalog, contracts, policies, validation reports, analytics, and operator surfaces. |
| [`docs/deployment-and-operations.md`](docs/deployment-and-operations.md) | Operators | Scripts, Docker/Kubernetes entry points, health, metrics, and deployment concerns. |
| [`docs/repository-guide.md`](docs/repository-guide.md) | Contributors | Workspace layout, crate responsibilities, and public-repo structure. |

## Repository Map

```text
.
├── assets/
├── arcxa-cli/
├── arcxa-coordinator/
├── arcxa-core/
├── arcxa-migrations/
├── arcxa-model-service/
├── arcxa-shard/
├── docker/
├── docs/
├── frontend/
├── kubernetes/
├── proto/
├── build.sh
├── run-local.sh
├── run-local-ha.sh
├── sync-public.sh
└── test.sh
```

## License

ARCXA is released under the Business Source License 1.1.

See `LICENSE.md` for terms, change-date behavior, and commercial-use guidance.
