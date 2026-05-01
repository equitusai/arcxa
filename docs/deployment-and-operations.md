# Deployment And Operations

This guide covers the practical entry points for building, running, packaging, and operating ARCXA.

## Use This Guide When

Use this guide when you want to:
- choose the right local or clustered runtime shape
- understand what the top-level scripts actually do
- orient operational discussions around health, metrics, storage, secrets, and recovery
- understand how the public sync flow relates to the private engineering workspace

## Table Of Contents

1. [Operating Modes](#operating-modes)
2. [Top-Level Scripts](#top-level-scripts)
3. [Build Flags And Feature Gates](#build-flags-and-feature-gates)
4. [Default Local Topology](#default-local-topology)
5. [HA Topology](#ha-topology)
6. [Docker Assets](#docker-assets)
7. [Kubernetes Assets](#kubernetes-assets)
8. [Storage, Secrets, And Recovery Reality](#storage-secrets-and-recovery-reality)
9. [Health, Metrics, And API Docs](#health-metrics-and-api-docs)
10. [Public Sync And Documentation](#public-sync-and-documentation)
11. [Current Operational Caveats](#current-operational-caveats)

## Operating Modes

| Mode | Best for | Shape |
| --- | --- | --- |
| Default local | Day-to-day development and debugging | One coordinator, two local shards, Docker-backed Kafka infrastructure, and the model service via `./run-local.sh` |
| HA local | Raft-oriented coordinator development and multi-node behavior | Three coordinators, three shards, and the model service via `./run-local-ha.sh` |
| Single-node local | Tighter local experiments | Simplified local setup via `./run-single-node.sh` |
| Container demos | Scenario-specific demos and validation | Docker assets under `docker/` |
| Cluster deployment | Stateful deployed environments | Helm chart under `kubernetes/helm-chart/` |

## Top-Level Scripts

| Script | Purpose |
| --- | --- |
| `./build.sh` | Build the main workspace using the repository's preferred Conda-safe environment cleanup. |
| `./test.sh` | Run focused Rust test targets with the same environment cleanup used by builds. |
| `./run-local.sh` | Start the default development topology: one coordinator, two shards, Docker-backed Kafka infrastructure, and the model service. |
| `./run-local-ha.sh` | Start the HA-oriented topology: three coordinators with Raft coordination, three shards, and the model service. |
| `./run-single-node.sh` | Start a tighter single-node local setup. |
| `start-coordinator-with-test-api.sh` | Start the coordinator with test-only API helpers for targeted development scenarios. |
| `./sync-public.sh` | Stage the public mirror from the private workspace, including curated docs and shared assets. |

## Build Flags And Feature Gates

The main top-level scripts understand several useful environment variables:
- `ENABLE_AUDIT=true|false`
- `ENABLE_HA=true|false`
- `ENABLE_ODBC=true|false`
- `ENABLE_DB2=true|false` as a backward-compatible alias for `ENABLE_ODBC`

Important current behavior:
- `build.sh` defaults ODBC off for local convenience
- `test.sh` defaults ODBC on for broader connector coverage in tests
- both scripts clear a number of compiler and linker environment variables to avoid Conda-related build conflicts

## Default Local Topology

The default local runner is:
- `./run-local.sh`

That script currently:
- builds the coordinator, model service, and shard
- starts ZooKeeper, Kafka, and Schema Registry with Docker Compose if they are not already running
- starts one coordinator and two shards
- starts the model service
- disables auth by default with `ENABLE_AUTH=false`
- uses the coordinator REST surface at `http://localhost:8082`

Important practical notes:
- the script wipes selected local state such as the file library database and workflow execution database on startup
- ONNX Runtime can be downloaded on first use if it is not already present locally
- older tooling defaults may still point at `8080`, so local operators often need to pass the `8082` base URL explicitly to the frontend, CLI, or custom scripts

## HA Topology

The HA-oriented local runner is:
- `./run-local-ha.sh`

Use it when you need to reason about:
- coordinator leadership and Raft behavior
- multi-node control-plane topology
- cluster-like startup shape in a local environment

The HA topology is more operationally realistic, but it is also heavier than the default local path.

## Docker Assets

The `docker/` directory is not one generic setup. It contains several scenario-specific assets, including:
- general Docker support files and health checks
- Oracle demo assets
- Oracle end-to-end assets
- ML pipeline demo assets
- MCP Oracle/DB2 assets
- Prometheus configuration
- database initialization scripts for PostgreSQL and TimescaleDB

This makes `docker/` a mix of:
- local infrastructure helpers
- demo packaging
- scenario-specific test environments

## Kubernetes Assets

The cluster packaging surface currently centers on:
- `kubernetes/helm-chart/`

Important characteristics of the current chart:
- the coordinator is deployed as a `StatefulSet`
- shards are deployed as a `StatefulSet`
- both coordinator and shard mount persistent volumes at `/data`
- ingress routes `/` to the frontend and `/api` and `/auth` to the coordinator
- coordinator config defaults the secret store to a file-backed store under `/data/secrets`

That default is important because it means the current cluster packaging already assumes persistent secret storage rather than purely in-memory credentials.

## Storage, Secrets, And Recovery Reality

A few storage and recovery realities matter operationally:
- the coordinator uses persistent RocksDB-backed state for several major subsystems
- the shard persists RDF data under its mounted data path
- the current cluster packaging uses persistent volume claims for both coordinator and shard state
- the coordinator secret-store defaults can be configured through `GRAPHICA_SECRET_STORE_TYPE`, `GRAPHICA_SECRET_STORE_DIR`, and `GRAPHICA_SECRET_STORE_FORMAT`
- the current Kubernetes default is a file-backed secret store rooted at `/data/secrets`

For SoS specifically, the platform now supports:
- explicit reconcile through `POST /api/v1/sos/reconcile`
- retention-aware replay of persisted validation history
- startup recovery helpers around SoS projection and ontology synchronization

## Migration Evidence Graph Operations

The migration-evidence wedge currently has two deployment modes:
- coordinator-hosted gateway mode for simpler topologies and local development
- standalone service mode using `arcxa-evidence-ingestion`, `arcxa-traceability`, and `arcxa-verification`

Helm chart support now exists for both modes:
- the default chart values keep migration evidence embedded in the coordinator
- `kubernetes/helm-chart/values-migration-evidence-external-direct.yaml` switches the coordinator into an external gateway mode and deploys the three migration-evidence services with direct gRPC delivery
- `kubernetes/helm-chart/values-migration-evidence-external-kafka.yaml` does the same, but enables Kafka-backed producer fan-out into traceability

Current service persistence is PVC-friendly, but no longer uniform:
- connector state now defaults to a RocksDB-backed ingestion store with one-time import support for older JSON connector snapshots
- traceability now persists read models and a replayable event log in RocksDB
- signed evidence packets are persisted as part of traceability state
- older JSON traceability state can be imported into RocksDB on first startup
- both ingestion and traceability are currently single-writer services from a persistence perspective, so the split-service chart profiles keep them at one replica by default

Current event-delivery posture is intentionally phased:
- `direct` delivery is still the default for coordinator-hosted and single-node setups
- `kafka` delivery is now available as an opt-in runtime mode for evidence-producer fan-out into traceability
- both `arcxa-evidence-ingestion` and `arcxa-verification` can publish onto that shared backbone
- Kafka delivery uses schema-versioned envelopes and idempotent producer settings
- traceability deduplicates replay and retry traffic by `event_id`
- malformed Kafka messages are logged and skipped so one poison payload does not wedge the consumer

Current operator controls now include:
- `GET /api/v1/migration-evidence/runtime/status`
- `POST /api/v1/migration-evidence/runtime/rebuild`
- CLI parity through `admin migration-evidence runtime status|rebuild`
- these controls rebuild the traceability service read models; they are not yet a full shard-graph reconcile primitive

`runtime/status` now also exposes event-bus posture for migration-evidence operators:
- whether the service is in `direct` or `kafka` mode
- the Kafka topic and consumer group when configured
- consumer state, startup-failure reason, and the most recent async-delivery error
- broker reachability, discovered broker count, assigned partitions, and topic partition count
- lag posture, lag diagnostics, and the latest observed estimated lag when Kafka delivery is active
- counters for processed, malformed, and retried messages
- connector-store backend, health, connector count, and writability through the aggregated ingestion status

Relevant runtime configuration now includes:
- `MIGRATION_EVIDENCE_GATEWAY_MODE` with `embedded` or `external`
- `MIGRATION_EVIDENCE_INGESTION_ENDPOINT`
- `MIGRATION_EVIDENCE_TRACEABILITY_ENDPOINT`
- `MIGRATION_EVIDENCE_CONNECTOR_ROCKSDB_PATH`
- `MIGRATION_EVIDENCE_EVENT_BUS_MODE` with `direct` or `kafka`
- `MIGRATION_EVIDENCE_KAFKA_BOOTSTRAP_SERVERS`
- `MIGRATION_EVIDENCE_KAFKA_TOPIC`
- `MIGRATION_EVIDENCE_KAFKA_CONSUMER_GROUP`
- `MIGRATION_EVIDENCE_TRACEABILITY_ROCKSDB_PATH`
- legacy `MIGRATION_EVIDENCE_CONNECTOR_STATE_PATH` and `MIGRATION_EVIDENCE_TRACEABILITY_STATE_PATH` retention for one-time import and compatibility

Current runtime direction is:
- REST through coordinator
- gRPC between internal services
- optional Kafka-backed fan-out between migration-evidence producers and traceability
- shard projection as the long-lived graph of record

In the external gateway mode specifically:
- the coordinator proxies migration-evidence REST calls to `arcxa-evidence-ingestion` and `arcxa-traceability` over gRPC
- verification runs still start from the coordinator REST surface, but execute through the ingestion service and then through the standalone verification service
- this keeps the public API stable while allowing the evidence path to be deployed and operated as distinct services

## Health, Metrics, And API Docs

Operationally useful surfaces include:
- `/health`
- `/health/live`
- `/health/ready`
- `/openapi.yaml`
- module-specific Swagger UIs under `/api/v1/.../swagger-ui`

The shard also exposes metrics-oriented behavior through its deployed annotations, and the repository includes Prometheus assets under `docker/prometheus/`.

## Public Sync And Documentation

The private development workspace and public mirror are intentionally different.

`./sync-public.sh` currently:
- pulls curated docs from `docs/public/` into `docs/` in the public mirror
- syncs shared assets such as `assets/arcxa-arch.png`
- keeps the public mirror cleaner than the engineering workspace by trimming a large amount of internal or historical noise

That means the curated docs under `docs/public/` are the documentation source that should be maintained for public accuracy.

## Current Operational Caveats

A few current realities are worth stating directly:
- local development is intentionally looser than secured environments because `./run-local.sh` disables auth by default
- some operator surfaces still default to `8080`, while the standard local coordinator listens on `8082`
- `arcxa-shard` is intentionally built outside the main Cargo workspace because of its dependency split
- the Kubernetes chart and docker demos still carry historical `graphica` naming even though the public documentation uses `ARCXA`

## Related Guides

- [`getting-started.md`](getting-started.md)
- [`architecture.md`](architecture.md)
- [`api-surface.md`](api-surface.md)
- [`repository-guide.md`](repository-guide.md)
