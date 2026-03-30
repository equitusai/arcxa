# ARCXA Performance Hardening Implementation Roadmap

## Purpose

This document is the working implementation plan for making ARCXA materially faster and more predictable across all workflow modes without breaking governance, lineage, or connector consistency.

The goal is not to optimize one connector or one workflow in isolation. The goal is to establish one performance model that applies consistently to:

- datasource-backed extraction and loading
- dataset-backed workflows
- file-backed workflows
- batch execution
- streaming execution
- lineage-aware execution

## Current Implementation Status

The roadmap is materially underway, but it is not close to complete yet. The work so far has been strongest in three areas:

- building the new runtime substrate and `BatchFrame` compatibility bridges
- shrinking and modularizing the monolithic workflow executor
- proving a limited set of safe, row-oriented batch-aware seams without breaking current JSON contracts

The work is intentionally compatibility-first. That has been the right choice so far, but it also means the codebase is still carrying both:

- the new runtime substrate
- the legacy JSON-oriented compatibility path

That is why the roadmap is best described as "foundationally in progress" rather than "performance-hardened".

### Assessment Snapshot

Current measurable state:

- `arcxa-core/src/orchestration/workflow/executor.rs` is now `586` lines
- executor support code is split across `35` internal files under `arcxa-core/src/orchestration/workflow/executor/`
- executor regressions are now split across `4` dedicated test modules under `arcxa-core/src/orchestration/workflow/executor/tests/`
- the runtime package exists under `arcxa-core/src/orchestration/workflow/runtime/` with `frame/`, `spill/`, `operators/`, `planner/`, `lineage/`, and `metrics/`
- the runtime tree currently contains `19` files
- the ongoing correctness gate remains the repo wrapper path `./test.sh core --lib`

### Phase Status

- Phase 0 `Baseline and Observability`: complete for the current workflow-runtime scope
  - the canonical workflow-runtime benchmark scaffold exists in `arcxa-core/benches/workflow_runtime_bench.rs`
  - coverage now includes:
    - `dataset input -> transform`
    - `dataset input -> transform -> validate`
    - `db_extract -> transform -> db_loader`
    - stream micro-batch `json_input -> transform -> validate`
    - baseline-only storage-tier round trip benchmarks for RocksDB and Parquet
  - the canonical invocation path exists via `./benchmark-workflow-runtime.sh`
  - the `quick` wrapper path uses the lighter `bench-quick` cargo profile and emits report artifacts under `artifacts/workflow-runtime-benchmarks/quick/`
  - the `baseline` wrapper path emits the heavier `10k` / `100k` / `1M` report artifacts under `artifacts/workflow-runtime-benchmarks/baseline/` and enables `workflow-storage` so the storage-tier benchmarks run against real RocksDB/Parquet backends
  - machine-readable and markdown report generation now exist via `scripts/ci/generate_workflow_runtime_benchmark_report.py`
  - the repo now has checked benchmark budgets in `config/performance/workflow_runtime_benchmark_thresholds.json`
  - the quick lane is CI/SLO-enforced, while the scheduled baseline lane is report-oriented so the 1M cases remain practical to run
  - the CI entrypoint now exists via `scripts/ci/run_workflow_runtime_benchmarks_ci.sh` and `.gitlab-ci.yml`
- Phase 1 `Canonical Batch Runtime`: materially in progress
  - `BatchFrame` exists
  - JSON <-> batch adapters exist
  - ingress metadata and compatibility bridges are in place
  - dataset/db/file ingress is not yet uniformly batch-native end to end
- Phase 2 `Deterministic Spill and Storage Policy`: partially in progress
  - spill policy and tiering extraction exist
  - `memory.rs`, `rocksdb.rs`, `parquet.rs`, and `manager.rs` now exist under `runtime/spill/`
  - spill quota configuration and reservation tracking now exist in `StorageManager`
  - real Parquet read/write and row materialization now exist behind `runtime/spill/parquet.rs`
  - storage selection, spill/fallback decisions, reservation bytes, and storage locations are now visible through `ExecutionContextV2` metrics and tracing
  - live `StepResult` construction and stored step-result persistence now retain runtime telemetry when step output includes `_runtime_metrics`
  - coordinator-visible execution reporting now retains per-step runtime metrics and an execution-level runtime telemetry summary in both detailed execution reads and execution summary/list responses
  - coordinator `WorkflowMetrics` Prometheus reporting now records execution-level storage backend, tier, decision, reservation, spill, and high-water-mark summaries
  - streaming control-plane APIs and `WorkflowMetrics` now expose a consistent runtime/storage summary for active stream execution state
  - workflow progress, workflow execution progress lists, and active execution operational views now expose the same runtime/storage summary by enriching progress snapshots from persisted execution state
  - batch job detail and list operational views now enrich persisted workflow execution references with per-execution runtime telemetry and batch-level runtime/storage rollups
  - the remaining Phase 2 gap is broader runtime-wide operational surfacing beyond workflow execution reporting, workflow progress views, streaming control-plane paths, and batch job detail/list views
- Phase 3 `Batch-Native Operators`: materially in progress
  - batch-native operators exist for the safe row-oriented subset
  - several live step seams carry `BatchFrame` sidecars
  - join and a fuller semantic preparation path are not implemented as real batch-native operators
  - the primary transform chain is not yet fully batch-native end to end
- Phase 4 `Pushdown Planner`: not started in the roadmap sense
  - planner scaffolding exists only as a placeholder module
- Phase 5 `Batch and Stream Unification`: not started
- Phase 6 `Lineage Cost Control`: not started
- Phase 7 `Connector Execution Standardization`: not started as a performance phase
- Phase 8 `Legacy Path Retirement`: not started

### What Is Actually Complete

The following are real, implemented foundations:

- runtime substrate creation under `runtime/`
- `BatchFrame` creation, JSON bridging, schema helpers, and frame slicing helpers
- cached `BatchFrame` support in both `ExecutionContext` and `ExecutionContextV2`
- extracted spill tiering policy
- first-stage spill backend extraction into `runtime/spill/memory.rs`, `runtime/spill/rocksdb.rs`, `runtime/spill/parquet.rs`, and `runtime/spill/manager.rs`
- storage decision metrics for planned tier, actual backend, fallback reason, and spill bytes/events
- spill quota configuration, reservation tracking, and cleanup release behavior in `StorageManager`
- real Parquet spill write/read support with range and single-row materialization
- carried batch-sidecar handoff for safe live execution seams
- ingress metadata stamping for adapter-prepared workflow inputs
- lightweight batch metadata retention in stored `StepResult`s
- modularization of executor orchestration, state, completion, finalization, lineage, lifecycle, bookkeeping, decisioning, context-batch handling, extraction helpers, and live step wrappers
- first-stage extraction of executor regression suites into dedicated submodules
- the last large inline executor `mod tests` block has been extracted into `executor/tests/workflow_core_contracts.rs`

Batch-aware runtime operators that are real today:

- `csv_export`
- `data_validator`
- `aggregator`
- `deduplicator` exact/first path
- `field_transformer` row-oriented path
- `semantic_mapper` optimized pass-through seam

Live executor seams that are real today:

- `db_extract`
- `field_transformer`
- `data_validator`
- `aggregator`
- strict exact/first `deduplicator`
- `csv_export`

### What Is Only Partial

These areas have meaningful progress, but they do not meet roadmap completion criteria yet:

- benchmark infrastructure
  - now implemented for the initial runtime scope, but still limited to the currently targeted workflow classes rather than every roadmap workflow family
- spill management
  - policy exists, backend-manager architecture does not
- batch-aware execution
  - only the currently safe subset is on the runtime path
- executor modularization
  - most support logic is extracted, but the legacy dispatch and a few remaining helper concentrations still exist
- ingress normalization
  - multiple ingress types carry metadata now, but not all of the roadmap’s ingress/egress surfaces are unified under one runtime-first model

### What Is Not Complete Yet

These are the material roadmap gaps:

- no benchmark-backed performance gate beyond the current workflow-runtime scope
- no end-to-end Arrow-first execution path
- no unified spill manager with explicit backends and quotas
- no batch-native join operator
- no complete semantic-mapping-preparation operator family under runtime
- no unified pushdown planner or connector capability model
- no stream-engine convergence on `BatchFrame` micro-batches
- no configurable lineage modes (`full`, `batched`, `sampled`, `minimal`)
- no connector execution standardization layer for runtime performance behavior
- no retirement of duplicated row-oriented slow paths

### Remaining Structural Concentration

The biggest remaining concentrations are:

- the legacy compatibility dispatch inside the live executor path
- `row_storage.rs` still owning too much of the storage behavior directly
- the still-separate stream execution path
- the lack of a benchmark/SLO gate to steer refactors by measured outcome

## Completion Plan

The roadmap should now be completed in execution waves rather than more opportunistic seam work. The order matters.

### Wave 1: Finish the Executor Stabilization Layer

Goal:

- close the remaining structural debt in the executor before taking larger runtime changes

Work:

- isolate any remaining cohesive helper clusters still living inline around the executor boundary
- freeze the executor shape so new runtime work lands in `runtime/` instead of drifting back into executor internals

Acceptance gate:

- `executor.rs` stays small and no new large inline test concentration is reintroduced
- all existing `./test.sh core --lib` compatibility gates stay green

### Wave 2: Complete Phase 0 Properly

Goal:

- make performance work measurable before continuing major behavioral changes

Work:

- create a benchmark harness aligned to this roadmap for:
  - `db_extract -> transform`
  - `dataset input -> transform`
  - `transform -> db_loader`
  - full extract -> transform -> load
  - stream micro-batch flow
- define a reproducible benchmark invocation path through repo wrappers
- emit a baseline report for `10k`, `100k`, and `1M` rows

Acceptance gate:

- baseline report exists and is reproducible
- top hot paths are known from measurements, not intuition

Current status:

- completed
- the harness now covers:
  - `dataset input -> transform`
  - `dataset input -> transform -> validate`
  - `db_extract -> transform -> db_loader`
  - stream micro-batch `json_input -> transform -> validate`
- invocation is standardized through `./benchmark-workflow-runtime.sh`
- the `quick` wrapper path runs through the lighter `bench-quick` cargo profile, emits `raw-output.txt`, `report.json`, and `report.md`, and is the enforced benchmark gate
- the `baseline` wrapper path runs through the full bench profile, enables `workflow-storage`, emits the same report artifacts for `10k`, `100k`, and `1M`, and is the heavier scheduled/manual reporting lane
- benchmark reports are generated by `scripts/ci/generate_workflow_runtime_benchmark_report.py`
- threshold enforcement is implemented by `scripts/ci/check_workflow_runtime_benchmark_thresholds.py` and `config/performance/workflow_runtime_benchmark_thresholds.json`
- CI wiring exists through `scripts/ci/run_workflow_runtime_benchmarks_ci.sh` and `.gitlab-ci.yml`
- current quick snapshot:
  - `dataset input -> transform`
    - `10k`: `125.62 ms`, roughly `79.6 Kelem/s`
    - `100k`: `1.2585 s`, roughly `79.5 Kelem/s`
  - `dataset input -> transform -> validate`
    - `10k`: `160.88 ms`, roughly `62.2 Kelem/s`
    - `100k`: `1.9780 s`, roughly `50.6 Kelem/s`
- current baseline storage-tier snapshot:
  - `workflow_runtime/storage_tiering_round_trip/rocksdb_round_trip/150000`
    - median: `821.64 ms`
    - throughput: roughly `182.56 Kelem/s`
  - `workflow_runtime/storage_tiering_round_trip/parquet_round_trip/1200000`
    - median: `4.6541 s`
    - throughput: roughly `257.84 Kelem/s`
  - note:
    - the Parquet case currently emits a Criterion target-time warning in the baseline lane
    - that warning is tolerated in the scheduled/manual lane because the purpose here is reproducible large-payload storage visibility, not a quick CI gate
  - `db_extract -> transform -> db_loader`
    - `10k`: `194.79 ms`, roughly `51.3 Kelem/s`
    - `100k`: `1.5102 s`, roughly `66.2 Kelem/s`
  - stream micro-batch `transform -> validate`
    - `1k`: `18.348 ms`, roughly `54.5 Kelem/s`
    - `5k`: `106.07 ms`, roughly `47.1 Kelem/s`
- current baseline snapshot:
  - `dataset input -> transform`
    - `10k`: `76.761 ms`, roughly `130.3 Kelem/s`
    - `100k`: `820.83 ms`, roughly `121.8 Kelem/s`
    - `1M`: `8.7549 s`, roughly `114.2 Kelem/s`
  - `dataset input -> transform -> validate`
    - `10k`: `165.14 ms`, roughly `60.6 Kelem/s`
    - `100k`: `1.8023 s`, roughly `55.5 Kelem/s`
    - `1M`: `11.395 s`, roughly `87.8 Kelem/s`
  - `db_extract -> transform -> db_loader`
    - `10k`: `119.43 ms`, roughly `83.7 Kelem/s`
    - `100k`: `972.90 ms`, roughly `102.8 Kelem/s`
    - `1M`: `9.5583 s`, roughly `104.6 Kelem/s`
  - stream micro-batch `transform -> validate`
    - `1k`: `11.738 ms`, roughly `85.2 Kelem/s`
    - `5k`: `67.847 ms`, roughly `73.7 Kelem/s`
    - `10k`: `138.38 ms`, roughly `72.3 Kelem/s`
- note on the baseline lane:
  - the `1M` cases still emit Criterion target-time warnings under the practical baseline profile
  - that warning is intentionally tolerated in the scheduled/manual baseline lane so the run stays operationally affordable
  - the enforced CI/SLO gate is the warning-clean quick lane

### Wave 3: Complete Phase 2 Before Expanding Operators Further

Goal:

- make storage behavior deterministic enough that larger operator migrations are safe

Work:

- move backend selection out of ad hoc behavior in `row_storage.rs`
- extend the new storage decision metrics into a broader runtime/runtime-operator observability contract
- extend quota-aware backend controls beyond the current `StorageManager` reservation model
- surface storage decisions and reservations beyond `ExecutionContextV2` into broader runtime/operator telemetry

Acceptance gate:

- large workloads spill deterministically
- storage backend choice is observable
- benchmark runs show bounded RSS under configured thresholds

### Wave 4: Finish the Phase 3 Operator Set

Goal:

- move the main row-oriented transform chain onto the runtime surface

Work:

- complete the runtime operator family structure described by the roadmap:
  - `transform/`
  - `validate/`
  - `aggregate/`
  - `dedup/`
  - `join/`
  - `semantic/`
- implement a real batch-native join operator
- implement the semantic preparation path as a real runtime operator, not just the current optimized seam
- expand live runtime use only where contract coverage already exists

Acceptance gate:

- the primary transform chain executes without row-by-row JSON reconstruction
- existing workflow behavior stays functionally identical on current tests
- benchmark deltas are measurable and positive

### Wave 5: Deliver the Planner Layer

Goal:

- stop doing datasource optimization ad hoc

Work:

- implement `runtime/planner/capabilities.rs`
- implement `runtime/planner/pushdown.rs`
- define connector capability contracts for:
  - projection
  - filter
  - limit
  - aggregate
  - ordering
  - incremental cursor

Acceptance gate:

- supported connectors push down projections and filters by default
- explain/debug output can show what was pushed down

### Wave 6: Unify Batch and Stream Execution

Goal:

- stop maintaining separate operator semantics between batch and stream

Work:

- refactor stream execution onto `BatchFrame` micro-batches
- reuse the same operator layer between production and stream executors

Acceptance gate:

- batch and stream execution share the same transform operator layer
- stream mode behavior matches batch mode for the same step contracts

### Wave 7: Add Lineage Cost Control

Goal:

- make bulk execution performance tunable without deleting lineage capability

Work:

- implement lineage modes under `runtime/lineage/`
- batch lineage emission where possible
- separate row identity preservation from full lineage persistence

Acceptance gate:

- lineage overhead is measurable by mode
- high-throughput workflows can run with reduced lineage cost

### Wave 8: Standardize Connector Execution

Goal:

- make connector performance behavior uniform enough for the planner to reason about it

Work:

- standardize runtime-facing connector contracts for:
  - batch extraction
  - parameter binding
  - cancellation
  - progress
  - bulk load
  - row count estimation

Acceptance gate:

- planner logic becomes connector-capability-driven instead of connector-special-cased

### Wave 9: Retire Legacy Slow Paths

Goal:

- remove duplicated row-oriented compatibility paths after the runtime path is proven

Work:

- retire obsolete slow-path internals
- keep adapters only at ingress/egress boundaries
- reduce executor complexity by removing branches that are no longer needed

Acceptance gate:

- no major workflow hot path depends on row-by-row JSON execution internally
- batch runtime becomes the default execution substrate

## Recommended Immediate Next Slice

The best next move is not another small executor seam.

The correct immediate order is:

1. stabilize the now-thin executor boundary and avoid backsliding into inline growth
2. complete the Phase 2 spill-manager architecture
3. widen benchmark coverage only after the spill-manager architecture is in place

That sequence gives us:

- a stable executor shell
- measured guidance for the rest of the roadmap
- a safe storage model before we widen the runtime operator migration

## Problem Statement

ARCXA is strong as a governance and orchestration framework, but the execution path is still too row-oriented and JSON-heavy for high-throughput workloads.

The current performance risks are:

- too much `serde_json::Value` movement in hot paths
- repeated row cloning and materialization between steps
- incomplete spill behavior for large workloads
- inconsistent execution behavior between batch and stream workflows
- connector pushdown that is ad hoc instead of planned
- lineage work that is too eager on hot execution paths

## Non-Goals

This roadmap does not require:

- changing workflow definition formats up front
- removing lineage or governance features
- replacing existing connectors wholesale
- introducing a separate execution engine product

## Architectural Principles

1. One internal execution payload
- All workflow modes should converge on one batch-oriented runtime payload.

2. JSON only at boundaries
- `serde_json::Value` remains valid at API boundaries, configuration boundaries, and final response boundaries, but should not be the dominant in-memory execution format.

3. One storage policy
- All workflows use the same deterministic memory -> spill policy.

4. One operator surface
- Batch and stream execution should share operator implementations.

5. Pushdown by plan, not by accident
- The planner decides what executes in-source versus in-engine.

6. Performance must be measurable
- No performance-sensitive changes merge without benchmark evidence.

## Target Runtime Shape

Create a new runtime layer under:

- `arcxa-core/src/orchestration/workflow/runtime/frame/`
- `arcxa-core/src/orchestration/workflow/runtime/planner/`
- `arcxa-core/src/orchestration/workflow/runtime/operators/`
- `arcxa-core/src/orchestration/workflow/runtime/spill/`
- `arcxa-core/src/orchestration/workflow/runtime/lineage/`
- `arcxa-core/src/orchestration/workflow/runtime/metrics/`

This layer becomes the canonical execution substrate for all workflow modes.

### Core Runtime Types

- `BatchFrame`
  - Arrow-backed tabular batch representation
  - schema-aware
  - cheap slicing/projection
  - convertible to/from JSON rows at boundaries

- `ExecutionBatch`
  - wraps `BatchFrame` plus execution metadata
  - source metadata, row identity mode, lineage mode, partition metadata

- `ExecutionPlan`
  - operator graph produced by planner
  - includes pushdown decisions, spill hints, and lineage policy

- `SpillHandle`
  - unified reference for in-memory, RocksDB, and Parquet backed data

## Phase Plan

## Phase 0: Baseline and Observability

### Scope

Instrument the current engine before changing execution semantics.

### Work

- Add benchmark suites for:
  - `db_extract -> transform`
  - `dataset input -> transform`
  - `transform -> db_loader`
  - full extract -> transform -> load
  - stream micro-batch flow
- Add metrics for:
  - rows/sec
  - p50/p95/p99 step latency
  - peak RSS
  - spill bytes
  - per-step materialization count
  - lineage cost per row and per batch

### Code Areas

- `arcxa-core/benches/`
- `arcxa-core/tests/`
- `arcxa-coordinator/tests/`
- workflow execution metrics in `arcxa-core` and `arcxa-coordinator`

### Acceptance Criteria

- Every major workflow class has a reproducible benchmark.
- CI can produce a baseline report for at least `10k`, `100k`, and `1M` row runs.
- We can identify the top 5 hottest execution paths by measured cost.

## Phase 1: Canonical Batch Runtime

### Scope

Introduce the new runtime package without changing the external workflow contract.

### Work

- Implement `BatchFrame` using `arrow2`.
- Implement adapters:
  - JSON rows -> `BatchFrame`
  - `BatchFrame` -> JSON rows
  - datasource row stream -> `BatchFrame`
  - Parquet dataset -> `BatchFrame`
- Keep current executors operational while new runtime is integrated behind adapters.

### Code Areas

- new submodules under `arcxa-core/src/orchestration/workflow/runtime/`
- adapters in:
  - `arcxa-coordinator/src/workflows/dataset_input.rs`
  - `arcxa-coordinator/src/workflows/db_extract_callback.rs`
  - file/dataset ingress paths

### Acceptance Criteria

- `db_extract`, dataset input, and CSV input can all emit `BatchFrame`.
- No workflow definition changes are required.
- The runtime can convert back to current JSON output for compatibility.

## Phase 2: Deterministic Spill and Storage Policy

### Scope

Replace fallback-heavy storage behavior with one explicit policy.

### Work

- Refactor the current row storage stack into:
  - `runtime/spill/policy.rs`
  - `runtime/spill/memory.rs`
  - `runtime/spill/rocksdb.rs`
  - `runtime/spill/parquet.rs`
  - `runtime/spill/manager.rs`
- Remove the behavior where large datasets silently remain in shared memory.
- Add configurable thresholds:
  - in-memory threshold
  - RocksDB spill threshold
  - Parquet spill threshold
  - maximum local spill quota

### Code Areas

- current:
  - `arcxa-core/src/orchestration/workflow/row_storage.rs`
  - `arcxa-core/src/orchestration/workflow/execution_context_v2.rs`
- new spill modules under runtime

### Acceptance Criteria

- Large workloads spill deterministically.
- Peak RSS remains within configured bounds in benchmark runs.
- Storage backend selection is visible in metrics and logs.

### Current Status

- `runtime/spill/policy.rs`, `runtime/spill/memory.rs`, `runtime/spill/rocksdb.rs`, and `runtime/spill/manager.rs` now exist
- `runtime/spill/parquet.rs` now exists as the Parquet backend boundary
- `runtime/spill/parquet.rs` now performs real Parquet write/read and targeted row materialization
- `row_storage.rs` delegates inline storage placement and storage-manager behavior into the runtime spill package
- `ExecutionContextV2` now records:
  - planned tier selection
  - actual backend chosen
  - fallback reason
  - spill event count
  - spill bytes
  - reserved spill bytes
  - per-execution reserved spill bytes
  - total reserved spill bytes
  - storage location when on-disk storage is used
  - recent storage decisions
- `StorageManager` now enforces configurable total/per-execution spill reservations and releases those reservations during cleanup
- the benchmark harness now exercises real RocksDB and Parquet storage tiers through `workflow_runtime/storage_tiering_round_trip/*`
- the baseline benchmark wrapper now runs an isolated storage-profile pass for the RocksDB and Parquet cases and merges sampled RSS / VmHWM / spill reservation data into the benchmark JSON and Markdown artifacts
- checked-in storage-tier budgets now exist for:
  - `workflow_runtime/storage_tiering_round_trip/rocksdb_round_trip/150000`
  - `workflow_runtime/storage_tiering_round_trip/parquet_round_trip/1200000`
- `RuntimeStepMetrics` now carries storage backend, tier, decision reason, reservation, spill, and storage-location telemetry in a serializable step-level shape
- `ExecutionContextV2` now exposes a structured runtime-step telemetry snapshot builder, and the optimized operator paths now emit that telemetry in `_runtime_metrics`
- the main live workflow execution path now parses `_runtime_metrics` into `StepResult.runtime_metrics` and preserves that telemetry in stored step results after batch-frame stripping
- coordinator execution materialization now retains per-step runtime telemetry in `ExecutionResultDto`, persisted `WorkflowExecution.step_results`, and an execution-level runtime telemetry summary
- coordinator execution summary/list surfaces now expose the same execution-level runtime telemetry summary for both workflow execution history and workflow execution list endpoints
- workflow progress, workflow execution progress list, and active execution APIs now enrich persisted progress snapshots with the same execution-level runtime telemetry summary
- coordinator `WorkflowMetrics` Prometheus reporting now rolls execution-level runtime summaries into backend/tier/decision counters plus spill/high-water-mark histograms
- streaming `StreamHandle` / `StreamStats` now carry a structured runtime summary for execution engine, storage backend, persistence, state location, and checkpoint cadence
- streaming control-plane responses now expose that runtime summary in start/stats/list payloads, and shared `WorkflowMetrics` now record active-stream, backend, throughput, latency, lag, worker-count, and checkpoint telemetry for streaming workflows
- batch job detail and list APIs now enrich persisted `WorkflowExecutionRef` entries with per-execution runtime telemetry and batch-level runtime/storage rollups derived from `ExecutionStore`
- remaining work is concentrated in:
  - broader quota surfacing and runtime-wide storage observability beyond workflow execution reporting, workflow progress views, streaming control-plane paths, and batch job detail/list views
  - consistent runtime/storage telemetry in additional non-workflow operational surfaces beyond the currently instrumented workflow, streaming control-plane, and batch job APIs

## Phase 3: Batch-Native Operators

### Scope

Move core workflow steps off row-by-row JSON execution.

### Work

- Implement batch-native operators for:
  - field transformation
  - validation
  - aggregation
  - deduplication
  - join
  - semantic mapping preparation
- Create operator subdirectories:
  - `runtime/operators/transform/`
  - `runtime/operators/validate/`
  - `runtime/operators/aggregate/`
  - `runtime/operators/dedup/`
  - `runtime/operators/join/`
  - `runtime/operators/semantic/`

### Code Areas

- current:
  - `arcxa-core/src/orchestration/workflow/executor.rs`
  - `arcxa-core/src/orchestration/workflow/streaming_deduplicator.rs`
- new operator modules under runtime

### Acceptance Criteria

- The primary transform chain can execute without converting every row back to `serde_json::Value`.
- Benchmarks show a measurable improvement over the old row-oriented path.
- Operator behavior remains functionally identical on existing workflow tests.

## Phase 4: Pushdown Planner

### Scope

Centralize extract/load/query optimization logic.

### Work

- Define connector pushdown capabilities:
  - projection
  - filter
  - limit
  - aggregate
  - ordering
  - incremental cursor
- Build `runtime/planner/pushdown.rs`.
- Build `runtime/planner/capabilities.rs`.
- Plan datasource-backed steps before execution starts.

### Code Areas

- `arcxa-core/src/catalog/`
- `arcxa-coordinator/src/catalog_impl.rs`
- runtime planner modules

### Acceptance Criteria

- Supported source connectors push down projections and filters by default.
- Workflow explain/debug output can show which operations were pushed down.
- Extract-heavy benchmarks improve for supported connectors.

## Phase 5: Batch and Stream Unification

### Scope

Unify semantics and operator code between batch and stream workflows.

### Work

- Refactor the stream engine to process `BatchFrame` micro-batches.
- Reuse the same operator implementations in batch and stream modes.
- Keep stream ingress source-specific, but make downstream execution common.

### Code Areas

- `arcxa-coordinator/src/workflows/engine/stream_executor.rs`
- `arcxa-coordinator/src/workflows/engine/production_executor.rs`
- runtime operators and planner

### Acceptance Criteria

- Batch and stream execution share the same transform operator layer.
- A workflow step behaves identically in batch mode and stream micro-batch mode.
- Stream throughput improves relative to row-oriented JSON processing.

## Phase 6: Lineage Cost Control

### Scope

Make lineage capture configurable and cheaper for bulk workloads.

### Work

- Add lineage modes:
  - `full`
  - `batched`
  - `sampled`
  - `minimal`
- Batch lineage emission and flush asynchronously where possible.
- Separate row identity preservation from full lineage persistence.

### Code Areas

- `runtime/lineage/`
- `arcxa-core/src/core/lineage/`
- coordinator lineage sinks and API surfaces

### Acceptance Criteria

- High-throughput workflows can run with reduced lineage cost without changing workflow definitions.
- Full lineage mode remains available for regulated/debug runs.
- Benchmark reports show lineage overhead by mode.

## Phase 7: Connector Execution Standardization

### Scope

Make connector performance behavior consistent across supported sources.

### Work

- Standardize connector interfaces for:
  - batch extraction
  - parameter binding
  - cancellation
  - progress reporting
  - bulk load
  - row count estimation
- Align PostgreSQL, Oracle, DB2, Databricks, and file-backed sources to that interface.

### Acceptance Criteria

- All production connectors expose the same execution contract shape.
- Workflow planner can reason about connectors uniformly.
- Connector-specific performance behavior becomes capability-driven instead of ad hoc.

## Phase 8: Legacy Path Retirement

### Scope

Remove duplicated slow-path logic after runtime migration is proven.

### Work

- Retire row-oriented fallback paths that are no longer needed.
- Keep compatibility adapters only at ingress/egress boundaries.
- Reduce logic inside:
  - `executor.rs`
  - legacy reader/loader paths
  - duplicated transformation paths

### Acceptance Criteria

- No major workflow hot path depends on row-by-row JSON execution internally.
- Batch runtime is the default execution substrate.
- Code ownership is clearer because runtime concerns live in dedicated submodules.

## Cross-Cutting Rules

### Module Size Rule

Do not put new large-scale runtime logic into existing monolithic files.

New work must land in clearly named subdirectories, especially:

- `runtime/frame/`
- `runtime/planner/`
- `runtime/operators/`
- `runtime/spill/`
- `runtime/lineage/`
- `runtime/metrics/`

If a file exceeds practical reviewability, split it by concern instead of adding another thousand lines.

### Compatibility Rule

- Workflow definitions remain stable unless a migration is explicitly approved.
- JSON response compatibility is preserved at API edges.
- New runtime behavior is hidden behind adapters and capability gates until proven.

### Benchmark Rule

Any change that affects:

- extraction
- transform performance
- spill behavior
- stream execution
- lineage cost
- loading throughput

must include benchmark evidence against the baseline.

## Final Acceptance Criteria

ARCXA can be considered performance-hardened when all of the following are true:

- all workflow modes use one internal batch-oriented execution substrate
- large workloads spill predictably instead of drifting into shared memory
- batch and stream execution share operator code
- source pushdown is planned and measurable
- lineage cost is configurable and benchmarked
- connector execution behavior is standardized
- CI catches performance regressions before merge

## Recommended Next Slice

Start with:

1. run the workflow-runtime benchmark harness in `quick` mode and capture the initial numbers
2. run the same harness in `baseline` mode and publish the first baseline report
3. complete the Phase 2 deterministic spill-manager architecture before widening operator migration again

That sequence gives the highest leverage with the least architectural churn because it establishes measurement, a reproducible benchmark path, and a stable memory/spill model before rewriting all operators.
