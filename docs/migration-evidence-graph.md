# ARCXA Migration Evidence Graph

Use This Guide When:
- you want the clearest explanation of ARCXA's IBM RISE and SAP HANA migration wedge.
- you need to understand what is implemented today versus what is planned next.
- you want to see how ARCXA fits beside IBM Rapid Move, SNP Kyano / CrystalBridge, smartShift, SAP ECC, SAP S/4HANA, and SAP HANA.

## Table Of Contents

1. [What This Module Is](#what-this-module-is)
2. [Positioning](#positioning)
3. [The Core Question: Explain This Value](#the-core-question-explain-this-value)
4. [Canonical Domain Model](#canonical-domain-model)
5. [What Is Implemented Today](#what-is-implemented-today)
6. [Current API Surface](#current-api-surface)
7. [Service Topology](#service-topology)
8. [Evidence Packet Semantics](#evidence-packet-semantics)
9. [Verification Semantics](#verification-semantics)
10. [Current Gaps](#current-gaps)
11. [How This Helps IBM](#how-this-helps-ibm)
12. [Related Guides](#related-guides)

## What This Module Is

ARCXA Migration Evidence Graph is the product wedge for migration explainability, evidence capture, and defensible sign-off.

The idea is straightforward:
- existing migration tooling moves, transforms, validates, and delivers systems.
- ARCXA captures the evidence trail around those actions in a persistent graph and query layer.
- when someone asks why a migrated value looks the way it does, ARCXA answers with evidence rather than project archaeology.

This wedge is intentionally adjacent to execution tools. It is not framed as a replacement for IBM Rapid Move, SNP Kyano / CrystalBridge, smartShift, or SAP-native migration mechanics.

## Positioning

The intended positioning is:
- IBM Rapid Move, Kyano / CrystalBridge, smartShift, and SAP tooling remain the execution stack.
- ARCXA becomes the transformation traceability and evidence layer.

That means ARCXA should be described as doing things like:
- normalizing migration, validation, approval, and exception artifacts
- projecting those artifacts into a persistent evidence graph
- answering field-level and record-level explainability questions
- generating signed evidence packets for sign-off and audit review

It should not be described as proving that the surrounding stack is weak or incomplete.

## The Core Question: Explain This Value

The first proof point for this module is:

> Explain this value.

Given one migrated field or record, ARCXA should be able to return:
- the source field reference
- the target field reference
- the transformation rule used
- the execution run that moved or transformed the value
- any related exceptions
- any related control or reconciliation results
- the approval chain
- the evidence packet that summarizes the story and can be signed

That is the contract this MVP is designed around.

## Canonical Domain Model

The canonical shared model lives in `arcxa-core/src/migration_evidence/mod.rs`.

Current core objects include:
- `MigrationConnector`
- `MigrationProgram`
- `MigrationObject`
- `SourceFieldRef`
- `TargetFieldRef`
- `TransformationRule`
- `ExecutionEvent`
- `ExceptionRecord`
- `ControlResult`
- `ApprovalEvent`
- `EvidencePacket`
- `ValueLocator`
- `ValueExplanation`
- `MigrationEvidenceEvent`
- `VerificationRequest`
- `VerificationResult`
- `ConnectorRunRequest`
- `ConnectorRunSummary`

This shared domain is the basis for:
- coordinator REST responses
- gRPC service contracts
- persistent traceability state
- Kafka event envelopes used by the async delivery path

## What Is Implemented Today

The current repository already contains a working MVP foundation.

Shared contracts:
- canonical migration-evidence domain types live in `arcxa-core`
- gRPC contract definitions live in `proto/migration_evidence_service.proto`

Coordinator gateway:
- REST routes are exposed under `/api/v1/migration-evidence`
- the coordinator can host an in-process gateway for local and single-node deployments

Standalone service crates:
- `arcxa-evidence-ingestion`
- `arcxa-traceability`
- `arcxa-verification`

Traceability behavior implemented today:
- ingestion of canonical migration evidence events
- RocksDB-backed traceability read models and append-only event-log persistence
- one-time import of older JSON traceability state into the RocksDB event log on first startup
- projection-ready graph references and optional shard triple pushes
- value explanation assembly across rules, executions, exceptions, controls, approvals, and packets
- signed evidence packet generation using Ed25519
- runtime status and deterministic read-model rebuild from persisted events

Verification behavior implemented today:
- HTTP JSON verification sources
- SAP HANA read-only SQL verification path through the existing connector seam
- generation of `ControlResult`, `ExecutionEvent`, and optional `ExceptionRecord`
- canonical migration-evidence event emission directly from the verification service
- the same `direct` or `kafka` delivery posture used by ingestion, so verification can publish onto the shared evidence backbone without depending on ingestion-side translation

Evidence-ingestion behavior implemented today:
- RocksDB-backed connector persistence with one-time import of older JSON state
- direct or Kafka-backed forwarding into traceability
- verification delegation without ingestion-side evidence duplication
- manual canonical-event ingestion
- HTTP artifact fetch and normalization
- verification-run orchestration through the coordinator-facing connector API
- direct in-process delivery for single-node deployments
- opt-in Kafka publishing with schema-versioned event envelopes for async delivery

## Current API Surface

The coordinator currently exposes these endpoints:
- `POST /api/v1/migration-evidence/connectors`
- `POST /api/v1/migration-evidence/connectors/{id}/runs`
- `GET /api/v1/migration-evidence/values/explain`
- `GET /api/v1/migration-evidence/objects/{id}/evidence-packet`
- `GET /api/v1/migration-evidence/objects/{id}/controls`
- `GET /api/v1/migration-evidence/programs/{id}/exceptions`
- `GET /api/v1/migration-evidence/programs/{id}/approvals`
- `GET /api/v1/migration-evidence/runtime/status`
- `POST /api/v1/migration-evidence/runtime/rebuild`

The current REST surface is intentionally thin and evidence-oriented.

Current documentation nuance:
- the REST routes are live
- unlike some older coordinator modules, the migration-evidence surface does not yet have its own dedicated Swagger UI module page
- the route family should currently be treated as documented through the curated docs and source-controlled request/response contracts

Current operator entry points:
- frontend workspace: `/migration-evidence`
- CLI: `admin migration-evidence ...`

The first operator slice now supports:
- value explanation for one migrated field or record
- signed evidence-packet lookup
- object-level control review
- program-level exception and approval review
- connector registration and connector-run kickoff from both the web UI and CLI
- connector-run summaries that report whether delivery was `direct` or `kafka`, plus whether traceability acknowledged the write synchronously
- runtime inspection of backend type, event-log availability, replay support, read-model counts, and event-bus consumer posture
- operator-triggered local read-model rebuild when replay-backed recovery is needed

Notable current behavior:
- connector create/update is upsert-style
- connector runs can ingest manual canonical events or execute verification flows
- explain-value responses can trigger evidence packet generation if no packet exists yet
- object-level evidence packet lookup can resolve either a fully-qualified value key or a bare field path when the object already carries record identifiers

## Service Topology

The intended topology is phased rather than a big-bang decomposition.

Current shape:
- `arcxa-coordinator` is still the public gateway and auth boundary
- standalone service crates exist, but coordinator can host the MVP gateway locally without requiring every service to run out of process

Implemented service responsibilities:
- `arcxa-evidence-ingestion`
  - connector persistence
  - HTTP artifact ingestion
  - verification-run orchestration
  - direct or Kafka-backed event delivery into traceability for non-verification evidence sources
- `arcxa-traceability`
  - evidence state projection and query
  - RocksDB-backed read models plus append-only event-log persistence
  - replay-safe deduplication by `event_id`
  - replay-driven rebuild and runtime status reporting
  - value explanation assembly
  - evidence packet signing
- `arcxa-verification`
  - read-only verification against external systems
  - current focus on HTTP JSON and SAP HANA SQL reads
  - canonical `ExecutionEvent` / `ControlResult` / `ExceptionRecord` emission onto the same shared evidence backbone used by ingestion

Current deployment direction:
- REST at the coordinator boundary
- gRPC between the three evidence services
- opt-in async event fan-out through Kafka from evidence producers into traceability, with direct delivery still available for simpler topologies
- PVC-backed local state for service-owned stores

Deployment profiles now exist in the Helm chart:
- embedded mode stays coordinator-hosted and is still the default
- external-direct mode deploys ingestion, traceability, and verification as separate services while keeping direct gRPC delivery
- external-kafka mode deploys the same split services, but uses Kafka-backed producer fan-out into traceability

Current runtime-status coverage now includes:
- whether the service is operating in `direct` or `kafka` mode
- the configured Kafka topic and consumer group when async delivery is enabled
- consumer state such as `running`, `recovering`, or `stopped`
- broker reachability and discovered broker count
- partition assignment, topic partition count, and per-partition lag snapshots
- processed-message, malformed-message, and retry counters
- lag posture and human-readable lag diagnostics
- timestamps for the last consumed, last assigned, last broker probe, and last successfully ingested async message
- aggregated ingestion status with connector-store backend, health, connector count, and writability

## Evidence Packet Semantics

Evidence packets are designed to be the audit-friendly summary object.

Today they are:
- canonical JSON objects first
- signed with Ed25519
- reproducible from the evidence graph inputs available in traceability state

A packet currently includes:
- source and target field references
- optional transformation rule
- optional execution event
- collected exceptions
- collected control results
- collected approvals
- graph references
- generated narrative text
- signature metadata

The packet signature carries:
- algorithm
- payload hash algorithm
- payload hash
- public key
- key fingerprint
- signature bytes
- signing timestamp

HTML or PDF evidence renderers are not the current source of truth. Canonical JSON is.

## Verification Semantics

Verification is deliberately read-only in this wedge.

Implemented behavior:
- compare expected and actual values
- support numeric tolerance when provided
- emit `Passed`, `Warning`, or `Failed` control states
- emit exception records for warning/failure cases

SAP HANA focus:
- the verification service uses the existing HANA connector seam for SQL reads
- phase 1 is about spot-checking and control evidence, not taking ownership of load/write execution

This distinction matters. The product story is explainability and defensibility, not replacing the migration engine.

## Current Gaps

The MVP is real, but it is still an MVP.

Important current boundaries:
- traceability now has replayable RocksDB-backed read models, Kafka-backed delivery is available, and connector persistence has moved onto a service-owned RocksDB path with legacy JSON import compatibility
- runtime status now exposes richer consumer health, broker reachability, partition assignment, and connector-store posture, but it is still focused on service-local observability rather than cluster-wide SLO dashboards
- traceability can optionally project triples to shard, but the full evidence-graph query story is still early
- SAP HANA verification exists as a connector path, but broader SAP ECC and S/4-specific live verification coverage still needs to deepen
- evidence packet HTML and PDF renderers are not yet the canonical operational path
- reusable cross-engagement control packs and pattern libraries are still future work

## How This Helps IBM

The IBM-facing value is not “better migration execution.”

The stronger and more accurate value story is:
- faster sign-off because value disputes can be explained quickly
- lower post-go-live support burden because teams can trace evidence instead of reconstructing it manually
- stronger audit posture because evidence packets can be assembled from persistent artifacts
- more defensible RISE engagements because transformation decisions and outcomes are queryable after the project rush is over
- reusable migration intelligence because controls, patterns, and approval models can be reused from one engagement to the next

The short version is:
- Kyano moves it.
- smartShift cleans it.
- IBM delivers it.
- ARCXA makes it explainable and defensible.

## Related Guides

- [`architecture.md`](architecture.md)
- [`api-surface.md`](api-surface.md)
- [`platform-capabilities.md`](platform-capabilities.md)
- [`lineage-and-governance.md`](lineage-and-governance.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)
