# Architecture

ARCXA is intentionally built as a set of cooperating runtimes instead of one monolithic process. That split is not cosmetic. It keeps the API control plane, the RDF data plane, and the optional semantic-matching inference path independently deployable and easier to evolve.

## Use This Guide When

Use this guide when you want to:
- understand which component owns which kind of work or data
- explain the coordinator versus shard split to another team
- reason about local versus distributed deployment shape
- understand where RDF projection fits into the rest of the platform

## Table Of Contents

1. [Platform Components](#platform-components)
2. [Current Naming And Packaging Reality](#current-naming-and-packaging-reality)
3. [Why The Shard Is Separate](#why-the-shard-is-separate)
4. [Control Plane, Data Plane, And Supporting Services](#control-plane-data-plane-and-supporting-services)
5. [Persistence Boundaries](#persistence-boundaries)
6. [Core Execution Flows](#core-execution-flows)
7. [Local And Distributed Topologies](#local-and-distributed-topologies)
8. [API Documentation Model](#api-documentation-model)
9. [Operator Surfaces](#operator-surfaces)

## Platform Components

| Component | What it is | Why it exists |
| --- | --- | --- |
| `arcxa-coordinator` | Main control-plane runtime | Centralizes APIs, orchestration, metadata, workflows, SoS validation, and operational control. |
| `arcxa-shard` | RDF and SPARQL data-plane runtime | Keeps graph persistence and query execution separate from the coordinator. |
| `arcxa-model-service` | Optional inference runtime | Isolates embedding and semantic-matching dependencies from the control plane. |
| `arcxa-cli` | Operator command-line tooling | Gives terminal-first operators an API-backed administration surface. |
| `arcxa-python` | Python automation package | Supports lightweight API automation and scripting. |
| `arcxa-core` | Shared library crate | Houses common types, connector abstractions, and workflow/domain contracts. |
| `frontend/` in the public mirror | React operator UI | Exposes day-to-day workflows across managed data, lineage, workflows, ontology, and SoS. |

## Current Naming And Packaging Reality

Public branding uses `ARCXA`, but the repository still contains historical `graphica` names in several places.

Examples:
- environment variables such as `GRAPHICA_MODEL_SERVICE_URL`
- the Python package name `graphica`
- some proto namespaces, comments, and helper scripts

This is mostly a naming transition issue, not an architectural split, but it matters because operators still interact with those identifiers.

## Why The Shard Is Separate

`arcxa-shard` is not a Cargo workspace member on purpose.

The reason is dependency isolation:
- the coordinator side uses the current workspace RocksDB dependency stack for metadata, WAL, and operational state
- the shard depends on `oxigraph`, which carries a different RocksDB binding expectation

Rather than forcing an unstable dependency compromise, the shard is built as a standalone package and wired into the rest of the system at runtime.

That decision has three practical effects:
- the top-level `build.sh` and `run-local.sh` scripts build the shard separately
- the shard can evolve on a different dependency cadence than the coordinator
- local and production deployment diagrams must account for at least two backend runtimes, not one

## Control Plane, Data Plane, And Supporting Services

### Control Plane

The coordinator owns:
- authentication and session entry points
- datasource registration, discovery, capability reporting, and connection testing
- file library, dataset import, catalogue, and entity APIs
- ontology and semantic mapping APIs
- workflow CRUD, validation, execution, scheduling, and approvals
- lineage and governance APIs
- systems-of-systems catalog, validation, analytics, and governance APIs
- health, readiness, metrics, and operator maintenance routes

### Data Plane

The shard owns:
- RDF triple storage
- SPARQL query execution
- graph-native persistence for lineage and governance projections
- shard-local identity and registration with the coordinator

### Supporting Services

The model service is optional, but useful when semantic matching matters. It is a separate process rather than a coordinator plugin because model dependencies, runtime libraries, and scaling pressures are materially different from the API layer.

In the default local topology, Kafka and Schema Registry are also part of the runtime story because lineage, replay, and distributed coordination paths depend on them.

## Persistence Boundaries

A simplified view of persistence ownership looks like this:

| Area | Primary owner |
| --- | --- |
| API metadata, datasource definitions, workflow state, file-library state, and SoS storage | `arcxa-coordinator` |
| RDF graph storage and SPARQL data | `arcxa-shard` |
| Semantic inference model weights and caches | `arcxa-model-service` |

Important nuance:
- the coordinator evaluates policies, orchestrates validations, and persists report records
- the coordinator also projects selected governance and lineage artifacts into RDF
- the shard is where those RDF projections live and where SPARQL queries execute
- explicit reconcile paths exist because coordinator-retained state and projected graph state must stay aligned

## Core Execution Flows

### Datasource To Dataset Flow

1. A datasource is registered in the coordinator.
2. Connector-specific discovery, schema inference, and preview APIs inspect the source.
3. Data can be imported into managed datasets or used directly by workflows.
4. Catalogue and entity APIs expose the resulting managed surface.

### Semantic Mapping Flow

1. Ontologies are registered in the coordinator.
2. Field mapping analysis combines heuristics, profiling, and optional model-service support.
3. Unified mapping sessions consolidate multiple sources into target-oriented mappings.
4. Loader paths write normalized outputs into downstream systems.

### Workflow Flow

1. Workflow definitions are registered and validated through the coordinator.
2. Execution can run synchronously, asynchronously, or through route and stream-oriented workflow surfaces.
3. Execution state, approvals, progress, and logs stay visible through coordinator APIs.
4. Lineage and provenance are emitted alongside the workflow activity.

### SoS Governance Flow

1. Systems, interfaces, contracts, and policies are stored in coordinator-managed SoS storage.
2. Validation reports and governance artifacts are persisted in coordinator storage.
3. Selected artifacts are projected into RDF for graph queries, analytics, and lineage-style audit views.
4. Reconcile and recovery paths rebuild those projections from retained storage when needed.

## Local And Distributed Topologies

### Default Local Topology

`./run-local.sh` currently starts:
- one coordinator on `http://localhost:8082`
- two shards on local shard ports
- one model service
- Kafka, ZooKeeper, and Schema Registry through Docker Compose

It also disables auth by default and downloads ONNX Runtime locally on first use if needed.

### HA Demo Topology

`./run-local-ha.sh` is the more distributed local shape.

It starts:
- three coordinators
- three shards
- one model service
- Kafka-backed infrastructure
- Raft-oriented coordinator communication paths

That script is meant for topology and recovery testing, not as the shortest path to first use.

### Container And Cluster Packaging

The repository also ships:
- Dockerfiles for the coordinator, shard, and model service
- `docker-compose.yml` for a fuller local container topology
- Kubernetes Helm assets under `kubernetes/helm-chart/`

## API Documentation Model

ARCXA uses modular OpenAPI and Swagger surfaces rather than one giant generated spec.

The coordinator exposes:
- a root OpenAPI index at `/openapi.yaml`
- module-specific Swagger UIs under `/api/v1/.../swagger-ui`

Examples:
- `/api/v1/datasources/swagger-ui`
- `/api/v1/workflows/swagger-ui`
- `/api/v1/lineage/swagger-ui`
- `/api/v1/file-library/swagger-ui`
- `/api/v1/sos/swagger-ui`

Important auth nuance:
- the root `/openapi.yaml` is publicly reachable
- the `/api/v1/...` surfaces live behind the main API router, so in secured deployments they inherit API auth requirements
- `./run-local.sh` disables auth, which is why local Swagger exploration is easier than a secured environment

## Operator Surfaces

Operator workflows now span four layers:
- coordinator maintenance and domain APIs
- the `arcxa-cli` binaries, especially `admin`
- the `arcxa-python` client for API-driven automation
- the React frontend, including the dedicated SoS workspace

## Related Guides

- [`glossary-and-concepts.md`](glossary-and-concepts.md)
- [`api-surface.md`](api-surface.md)
- [`model-service-and-inference.md`](model-service-and-inference.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)

