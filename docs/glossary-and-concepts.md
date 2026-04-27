# Glossary And Concepts

This guide gives the shared vocabulary behind the rest of the ARCXA documentation. If you want the fast path to understanding what terms mean before reading the deeper subsystem guides, start here.

## Use This Guide When

Use this guide when you want to:
- understand the difference between a datasource, dataset, catalogue entry, and entity
- understand why the coordinator and shard are separate
- decode older `graphica` names that still appear in code and tooling
- understand the operational meaning of terms such as `dry-run`, `reconcile`, `approval request`, and `projection`

## Table Of Contents

1. [Naming Reality](#naming-reality)
2. [Platform Components](#platform-components)
3. [Data And Semantics Terms](#data-and-semantics-terms)
4. [Workflow And Execution Terms](#workflow-and-execution-terms)
5. [Lineage And Governance Terms](#lineage-and-governance-terms)
6. [Systems-Of-Systems Terms](#systems-of-systems-terms)
7. [Operational Terms](#operational-terms)

## Naming Reality

### ARCXA

The public product name used throughout this curated documentation.

### graphica

A historical internal and packaging name that still appears in:
- environment variables such as `GRAPHICA_API_BASE_URL`
- the Python package name `graphica`
- some proto namespaces, comments, and helper scripts

When you see both names, treat `ARCXA` as the platform brand and `graphica` as a still-active implementation identifier.

## Platform Components

### Coordinator

The main control-plane runtime. It exposes the REST API, manages metadata and workflows, persists SoS state, and coordinates the rest of the platform.

### Shard

The RDF and SPARQL data-plane runtime. It stores graph data and serves graph-oriented query paths.

### Model Service

An optional inference runtime that provides embeddings and model-assisted semantic matching. The platform can run without it, but semantic workflows are richer with it available.

### CLI

The operator command-line layer. The current `admin` binary is SoS-focused and API-backed; `migrate` is for migration and maintenance work.

### Python Client

A lightweight automation client for selected coordinator APIs. It still uses the historical `graphica` package name.

## Data And Semantics Terms

### Connector

A source-type implementation such as PostgreSQL, Snowflake, CSV, Oracle, or Databricks. Connectors expose capability metadata, but not all connectors support every path equally.

### Datasource

A stored source definition that uses a connector plus connection details, configuration, and credentials. Datasources are what workflows and discovery jobs operate against.

### File Library

The managed ingress surface for file-backed work. It provides a governed place to register, scan, organize, preview, and reuse files.

### Dataset

A managed data object exposed above raw source connectivity. Datasets are the stable surface many downstream operators should reason about instead of the original source system.

### Catalogue

The operator-facing managed-data view over datasets and related metadata. In the UI, catalogue views are where many teams explore governed data without rethinking connector details.

### Entity

A higher-level managed record or object exposed through entity APIs, often used for browsing attributes, lineage, and resolution-oriented views.

### Ontology

The semantic vocabulary or conceptual model used to align source-native structure with domain concepts.

### Unified Mapping

The multi-source normalization surface used to align and consolidate several sources into a target-oriented mapping or downstream load path.

### R2RML

The relational-to-RDF mapping surface used when structured source data needs RDF-oriented mapping behavior.

## Workflow And Execution Terms

### Workflow

A stored executable definition that can orchestrate ingestion, transformation, loading, validation, lineage emission, and related actions.

### Validate

A pre-execution check of workflow structure or payload compatibility.

### Dry-Run

A lower-risk path that executes validation logic without persisting the same sort of downstream operational change you would expect from a normal mutable path.

### Schedule

A time-based execution rule attached to a workflow.

### Approval

A human or policy-gated control point for a workflow execution or a governance transition.

## Lineage And Governance Terms

### Row Lineage

Traceability for a specific row or record over time.

### Column Lineage

Traceability for how one field or column is derived from another.

### Governance Projection

The act of writing selected metadata, lineage, or governance artifacts into RDF so they can be queried graphically or via SPARQL.

### SPARQL Surface

The API surface used to query RDF-managed governance data.

## Systems-Of-Systems Terms

### System

An operational or mission system that participates in an integration landscape.

### Interface

A modeled integration boundary with a schema and contextual metadata.

### Contract

A provider-consumer agreement with lifecycle state, revision history, approval workflow, and signature material.

### Policy

A validation or governance rule that can apply to interface pairs, contracts, or rollout stages.

### Approval Request

A first-class governance object representing a pending decision, along with its evidence and resulting approval or rejection outcome.

### Attestation

Recorded proof or signature material that binds an approval or signing event to a specific policy or contract revision.

### Reconcile

An operator-triggered rebuild of projected SoS graph state from retained authoritative storage.

## Operational Terms

### Authoritative State

The persistence layer ARCXA treats as the source of truth for a given kind of object.

### Projected State

A derived representation, often in RDF, generated from authoritative state for querying or governance purposes.

### Retention

The rule set that bounds how much historical operational or validation data remains persisted.

### Recovery

The startup or operator-triggered behavior that rebuilds or rehydrates state without incorrectly resurrecting data that has been intentionally pruned.

## Related Guides

- [`architecture.md`](architecture.md)
- [`getting-started.md`](getting-started.md)
- [`systems-of-systems.md`](systems-of-systems.md)
- [`repository-guide.md`](repository-guide.md)

