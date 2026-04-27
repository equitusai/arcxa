# Architecture

ARCXA is built as a set of cooperating runtimes instead of a single monolith. That split is not just aesthetic; it keeps the control plane, RDF data plane, and optional model inference path independently deployable.

## The Runtime Model

| Runtime | Responsibility |
| --- | --- |
| `arcxa-coordinator` | Authenticated REST and gRPC APIs, metadata, orchestration, workflow execution, policy and validation services, catalog management, and operational endpoints. |
| `arcxa-shard` | Distributed RDF storage and SPARQL execution. This is the graph-native data plane behind lineage and governance-heavy use cases. |
| `arcxa-model-service` | Optional gRPC model service used for semantic matching and model-assisted operations. |
| `arcxa-cli` | Thin operator tooling over the coordinator API plus migration/runtime utilities. |
| `arcxa-core` | Shared contracts, connector abstractions, workflow primitives, and cross-cutting domain types. |

## Why The Shard Is Built Separately

The coordinator-side workspace and the shard runtime depend on different RocksDB bindings.

That separation is intentional:
- the coordinator side uses the current workspace RocksDB dependency set for metadata, WAL, and operational state
- the shard side uses `oxigraph` with its own RocksDB requirements for RDF storage

Rather than forcing a fragile dependency compromise, ARCXA treats the shard as a separately built runtime.

## Control Plane vs Data Plane

### Control Plane

The coordinator owns:
- authentication and API composition
- datasource registration and capability reporting
- workflow definition, validation, execution, and scheduling
- mapping sessions, ontology workflows, and policy evaluation
- systems-of-systems catalog and validation services
- operational surfaces such as health, metrics, and maintenance endpoints

### Data Plane

The shard owns:
- RDF triple persistence
- SPARQL query execution
- distributed graph data access
- graph-native provenance and governance data that is projected into the RDF layer

This split lets ARCXA evolve orchestration behavior without tightly coupling it to graph storage internals.

## Primary Data Flow

A typical governed flow looks like this:

1. A datasource is registered in the coordinator.
2. Schema or capability discovery runs against that source.
3. Data is materialized into governed datasets or used directly by workflows.
4. Mapping and ontology workflows align source-native fields with domain terms.
5. Workflows transform, validate, and load outputs into downstream systems.
6. Lineage, validation history, and governance artifacts preserve the chain of custody.

## Systems-Of-Systems Layer

ARCXA now includes a dedicated systems-of-systems control layer on top of the core platform.

That layer models:
- systems
- interfaces
- contracts
- policies
- validation reports and lineage
- compatibility and dependency analytics

If that area is central to your use case, read [`systems-of-systems.md`](systems-of-systems.md) next.

## Deployment Shape

ARCXA supports several practical shapes:
- single-coordinator development mode
- multi-shard distributed RDF storage
- optional HA/local-raft-oriented coordinator topologies
- optional model-service deployments where semantic matching is needed

For day-to-day local work, start with `./run-local.sh`.
For packaging and operations, continue into [`deployment-and-operations.md`](deployment-and-operations.md).
