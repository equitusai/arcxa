# Systems Of Systems

ARCXA's systems-of-systems layer is a policy and contract-aware validation surface for complex integrations.

It is designed for situations where simply moving data is not enough; teams also need to know whether systems, interfaces, and contractual expectations remain compatible over time.

## What The SoS Layer Models

The SoS catalog centers on four core object types:

| Object | Purpose |
| --- | --- |
| Systems | Named runtime or mission systems participating in an integration landscape. |
| Interfaces | Described integration boundaries with schemas, metadata, and directionality. |
| Contracts | Provider/consumer expectations, lifecycle state, approval state, and signatures. |
| Policies | Validation rules that can target interface pairs, contracts, or broader rollout stages. |

## What The Runtime Does

On top of that catalog, the SoS runtime can:
- validate interface compatibility
- persist validation reports with lineage between revisions
- build compatibility-matrix views for visible interface pairs
- generate dependency-graph views
- run what-if analyses against hypothetical changes
- preserve governance history for contracts and policies

## Validation Reports Are First-Class

Validation is not just an ephemeral API result.

ARCXA persists:
- validation reports
- report-to-report lineage
- change summaries
- policy and contract references
- subject-oriented history views

That lets operators answer questions like:
- when did compatibility start degrading?
- which report superseded the last approved state?
- which policy revision or contract revision was involved?

## Governance Model

The SoS layer now has a real governance surface rather than simple boolean flags.

### Policies

Policies support:
- revisioned storage
- approval requests and evidence
- approval attestations
- lifecycle states for rollout
- explicit revision pinning during evaluation
- signing-key lifecycle visibility and rotation APIs

### Contracts

Contracts support:
- revisioned lifecycle and provenance
- approval requests and evidence
- cryptographic signing and signature history
- signing-key lifecycle visibility and rotation APIs
- policy-gated approval and signing paths

## Operator Surfaces

The SoS layer is no longer API-only.

Current operator access includes:
- CLI commands for reconcile, catalog reads, validation/report workflows, and governance inspection
- UI surfaces for operations and governance views
- report-centric change and lineage summaries inside the SoS reports workspace

## Analytics Notes

Compatibility matrix, dependency graph, and what-if analysis now expose deterministic truncation metadata when they hit evaluation budgets.

That means callers can distinguish:
- complete results
- intentionally partial results
- remaining work that was not evaluated in the current response window

## Operational Notes

The SoS runtime also includes:
- explicit reconcile to rebuild projected graph state
- startup recovery helpers that reconcile ontology-linked assets and graph projections
- retained-history replay coverage so restart behavior does not silently resurrect pruned reports

## Where To Go Next

If you want to operate this layer, read [`deployment-and-operations.md`](deployment-and-operations.md).
If you want to understand the runtime split beneath it, read [`architecture.md`](architecture.md).
