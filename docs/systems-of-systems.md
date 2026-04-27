# Systems Of Systems

ARCXA's systems-of-systems layer is the platform's integration-governance control plane. It treats interoperability as a managed domain with catalogued systems and interfaces, revisioned contracts and policies, persisted validation history, analytics, reconcile and recovery controls, and operator-facing audit workflows.

## Use This Guide When

Use this guide when you want to:
- understand what the SoS catalog models and why it exists separately from generic workflow or schema tooling
- understand how validation becomes retained operational history rather than a one-time check
- explain the current governance semantics for contracts, policies, approvals, evidence, and attestations
- understand where reconcile, RDF projection, analytics budgets, and recovery fit into SoS operations

## Table Of Contents

1. [Mental Model](#mental-model)
2. [Catalog Objects And Governance Artifacts](#catalog-objects-and-governance-artifacts)
3. [Core Validation Flows](#core-validation-flows)
4. [Validation History, Lineage, And Change Tracking](#validation-history-lineage-and-change-tracking)
5. [Contract Governance](#contract-governance)
6. [Policy Governance](#policy-governance)
7. [Analytics And Large-Catalog Behavior](#analytics-and-large-catalog-behavior)
8. [Operator Surfaces](#operator-surfaces)
9. [Projection, Reconcile, And Recovery](#projection-reconcile-and-recovery)
10. [Current Design Boundaries](#current-design-boundaries)

## Mental Model

A useful way to think about the SoS layer is:
- authoritative catalog state lives in coordinator-managed storage
- validations execute against that state and can persist reports
- governance workflows reference exact contract and policy revisions, not just mutable top-level IDs
- selected catalog and governance artifacts are projected into RDF for audit, provenance, and graph-native analysis
- reconcile can rebuild the projected graph from retained authoritative state

That split matters because the platform is intentionally not relying on the RDF projection as the only source of truth.

## Catalog Objects And Governance Artifacts

The SoS catalog centers on four primary object types:

| Object | Purpose |
| --- | --- |
| Systems | Named operational or mission systems participating in an integration landscape. |
| Interfaces | Integration boundaries with schemas, metadata, directionality, coordinate-system, and unit-system context. |
| Contracts | Provider and consumer agreements with lifecycle, approval, signature, and revision history. |
| Policies | Validation rules that can be evaluated directly or applied to rollout stages such as contract approval and signing. |

Around those primary objects, the SoS layer also persists governance artifacts such as:
- approval requests
- approval evidence
- validation reports
- policy approval attestations
- contract signature attestations

The dedicated SoS API family lives under:
- `/api/v1/sos`

It also has its own Swagger UI at:
- `/api/v1/sos/swagger-ui`

## Core Validation Flows

The SoS module currently supports:
- interface-pair compatibility validation
- dry-run validation without persisting a report
- schema validation against stored interface definitions
- direct policy validation and dry-run evaluation
- retained report history and lineage lookup

Representative routes include:
- `POST /api/v1/sos/validate`
- `POST /api/v1/sos/validate/dry-run`
- `POST /api/v1/sos/interfaces/{id}/validate-schema`
- `POST /api/v1/sos/policies/{id}/validate`
- `POST /api/v1/sos/policies/{id}/validate/dry-run`
- `GET /api/v1/sos/validation-history`
- `GET /api/v1/sos/validation-lineage`

This is not just a transient check surface. Validation is intentionally modeled as retained operational history that can support governance and operations later.

## Validation History, Lineage, And Change Tracking

Validation reports are first-class records.

The current SoS layer persists:
- validation reports
- report-to-report lineage
- subject-oriented history views
- stable and revision-aware references to policies and contracts used during evaluation
- change summaries and trend data used by the frontend reports workspace

Why that matters:
- operators can see when compatibility degraded, not just whether it is currently passing
- approval workflows can point to exact revisions and evidence instead of relying on mutable top-level state
- retained-history replay and reconcile behavior can be tested and reasoned about operationally

## Contract Governance

Contract governance has moved beyond boolean approval flags.

The current contract surface includes:
- contract CRUD
- direct interface-pair contract lookup
- revisioned contract storage
- approval requests and approval evidence
- approve and reject flows tied to approval requests
- contract signing and signature history
- signing-key status and rotation APIs
- RDF projection of approval requests, evidence, and signature attestations

Representative routes include:
- `GET /api/v1/sos/contracts/lookup`
- `POST /api/v1/sos/contracts/{id}/approval-requests`
- `POST /api/v1/sos/contracts/{id}/approval-requests/{request_id}/evidence`
- `POST /api/v1/sos/contracts/{id}/approve`
- `POST /api/v1/sos/contracts/{id}/reject`
- `POST /api/v1/sos/contracts/{id}/sign`
- `GET /api/v1/sos/contracts/{id}/signatures`
- `GET /api/v1/sos/contracts/signing-key`
- `POST /api/v1/sos/contracts/signing-key/rotate`

Important semantic point:
- semantic contract changes require new revisions
- governance metadata changes do not pretend to be new semantic revisions
- signed contracts can now carry attestation material tied to the exact revision that was approved and signed

## Policy Governance

Policy governance now behaves more like a control plane than like free-form configuration.

The current policy surface includes:
- policy CRUD
- revision-aware policy storage
- lifecycle states such as `draft`, `dry_run`, `active`, `deprecated`, and `retired`
- approval requests and approval evidence
- approval and rejection workflows
- explicit revision pinning during evaluation
- policy approval attestation material
- policy signing-key status and rotation APIs
- policy audit and attestation views exposed through the API, CLI, and frontend

Representative routes include:
- `POST /api/v1/sos/policies/{id}/validate`
- `POST /api/v1/sos/policies/{id}/validate/dry-run`
- `POST /api/v1/sos/policies/{id}/approval-requests`
- `POST /api/v1/sos/policies/{id}/approval-requests/{request_id}/evidence`
- `POST /api/v1/sos/policies/{id}/approve`
- `POST /api/v1/sos/policies/{id}/reject`
- `GET /api/v1/sos/policies/{id}/attestations`
- `GET /api/v1/sos/policies/signing-key`
- `POST /api/v1/sos/policies/signing-key/rotate`

Practical effect:
- policies can gate contract approval and signing stages
- direct evaluation can be pinned to an explicit revision for reproducibility
- governance review can inspect approval requests, evidence, and approval attestations as separate artifacts

## Analytics And Large-Catalog Behavior

The SoS analytics surface currently includes:
- compatibility matrix generation
- dependency graph generation
- what-if analysis

Representative routes include:
- `GET /api/v1/sos/compatibility-matrix`
- `GET /api/v1/sos/dependency-graph`
- `POST /api/v1/sos/what-if`

These paths now expose deterministic truncation metadata when they hit configured budgets.

That means callers can distinguish:
- complete results
- intentionally partial results
- the amount of remaining candidate work that was not evaluated yet

This matters for large catalogs because the platform no longer has to pretend every large cross-product or graph expansion request is always cheap.

## Operator Surfaces

The SoS layer is no longer API-only.

### Frontend

The dedicated SoS workspace in the React UI currently includes tabs for:
- Pair Workbench
- Reports
- Catalog
- Policies
- Analytics
- Compatibility Matrix
- Operations

Together, those tabs cover validation, trend-aware reports, catalog navigation, policy review, analytics, reconcile, approval audit, and signing-key operations.

### CLI

The `admin` CLI exposes authenticated SoS workflows such as:
- reconcile
- catalog listing
- interface-pair validation
- report fetch, history, and lineage
- compatibility matrix, dependency graph, and what-if analysis
- contract governance audit and signing-key operations
- policy governance audit and signing-key operations

## Projection, Reconcile, And Recovery

The SoS layer persists authoritative state in coordinator-managed storage and projects selected artifacts into RDF.

Operationally, three things matter:
- incremental projection during normal mutations
- explicit reconcile to rebuild graph state from retained storage
- startup and restart recovery behavior that does not resurrect pruned history incorrectly

The current codebase now includes:
- an explicit `POST /api/v1/sos/reconcile` control
- retention-aware replay coverage
- startup recovery helpers around the SoS projection and ontology-sync path
- operator surfaces in the CLI and frontend for reconcile and governance review

## Current Design Boundaries

A few current realities are worth stating plainly:
- the SoS surface is one of the most actively evolving areas of the platform, so governance semantics are more mature than some older repository-wide docs imply
- contract and policy governance are revision-aware and audit-oriented, but true external trust backends remain a separate concern from the current software-managed signing paths
- analytics are bounded for large catalogs, so callers should treat truncation metadata as part of the response contract
- local runs disable auth through `./run-local.sh`, but real `/api/v1/sos/...` usage should be treated as authenticated in secured environments

## Related Guides

- [`glossary-and-concepts.md`](glossary-and-concepts.md)
- [`frontend-and-cli.md`](frontend-and-cli.md)
- [`sdk-and-automation.md`](sdk-and-automation.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)
