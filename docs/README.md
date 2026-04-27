# ARCXA Documentation Hub

This directory is the curated public documentation surface for ARCXA.

ARCXA is a governed data platform that combines datasource onboarding, dataset materialization, semantic mapping, workflow orchestration, lineage, and systems-of-systems validation in one codebase. This documentation is meant to be the stable entry point for evaluators, operators, and contributors.

Historical markdown elsewhere in the repository may still be useful as engineering history, but it should not be treated as the public reference surface unless it is linked from here.

## Table Of Contents

1. [How To Use This Documentation](#how-to-use-this-documentation)
2. [Documentation Conventions](#documentation-conventions)
3. [Naming Note](#naming-note)
4. [Guide Map](#guide-map)
5. [Recommended Reading Paths](#recommended-reading-paths)
6. [Current Accuracy Contract](#current-accuracy-contract)

## How To Use This Documentation

Start with the guide that matches the question you are trying to answer:
- what does the platform do?
- how do I run it locally?
- where do specific APIs and operator surfaces live?
- how is the repository split across coordinator, shard, model service, CLI, SDK, and frontend?

The guide map below is organized around those questions rather than around the repository's historical evolution.

## Documentation Conventions

A few conventions are used throughout this docs set:
- `ARCXA` is the public platform name.
- older `graphica` identifiers are called out when they still matter operationally.
- local-development examples assume the default `./run-local.sh` topology unless stated otherwise.
- secured environments should be assumed to require auth for `/api/v1/...` routes, even when local development does not.
- the curated docs in this directory override older ad hoc markdown elsewhere in the repo when they disagree.

If you want the shared vocabulary first, read [`glossary-and-concepts.md`](glossary-and-concepts.md).

## Naming Note

Public documentation uses `ARCXA` as the platform name.

You will still encounter historical `graphica` identifiers in the repository and runtime surface, including:
- environment variables such as `GRAPHICA_API_BASE_URL`
- the Python package name `graphica`
- some script names, comments, and internal module paths

Those identifiers are still operationally relevant, so this documentation calls them out when they affect usage.

## Guide Map

| Guide | Best for | What it covers |
| --- | --- | --- |
| [`glossary-and-concepts.md`](glossary-and-concepts.md) | Everyone | Shared terminology, naming reality, and platform concepts that recur across the rest of the docs. |
| [`getting-started.md`](getting-started.md) | First-time users | Prerequisites, local build and run flows, auth and port caveats, and the difference between the private workspace and the public mirror. |
| [`architecture.md`](architecture.md) | Architects and platform leads | Runtime topology, storage boundaries, control-plane and data-plane split, and the main execution flows. |
| [`platform-capabilities.md`](platform-capabilities.md) | Product, solution, and delivery teams | The capability map for the platform and how the major product areas fit together. |
| [`api-surface.md`](api-surface.md) | API consumers and integrators | Versioned API layout, module-specific Swagger UIs, auth expectations, and the major REST surfaces exposed by the coordinator. |
| [`data-sources-and-datasets.md`](data-sources-and-datasets.md) | Data onboarding teams | Connectors, capability discovery, datasource discovery jobs, file library, dataset import, catalogue, and entity exploration. |
| [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md) | Semantic modeling teams | Ontology management, field mapping, unified mapping, R2RML, loader paths, and the optional model service. |
| [`model-service-and-inference.md`](model-service-and-inference.md) | ML and platform teams | The optional embedding service, its runtime contract, and how it improves semantic workflows. |
| [`workflows-and-execution.md`](workflows-and-execution.md) | Workflow authors and operators | Workflow CRUD, validation, execution, schedules, approvals, progress tracking, and runtime modes. |
| [`lineage-and-governance.md`](lineage-and-governance.md) | Audit, lineage, and platform governance teams | Row, column, workflow, and schema-evolution lineage plus SPARQL and graph-oriented governance surfaces. |
| [`systems-of-systems.md`](systems-of-systems.md) | Integration governance teams | SoS catalog, validation, analytics, policy and contract governance, reconcile, and recovery. |
| [`frontend-and-cli.md`](frontend-and-cli.md) | Operators and enablement teams | The web UI surface, SoS workspace tabs, and the operator CLI binaries and usage patterns. |
| [`sdk-and-automation.md`](sdk-and-automation.md) | Automation teams | Python client, CLI automation, curl-first workflows, and current automation boundaries. |
| [`deployment-and-operations.md`](deployment-and-operations.md) | Operators | Scripts, Docker and Kubernetes assets, metrics, health, topology choices, and operational caveats. |
| [`repository-guide.md`](repository-guide.md) | Contributors | Repository layout, historical naming, public sync behavior, and documentation maintenance guidance. |

## Recommended Reading Paths

### If you are evaluating the platform

1. Read [`architecture.md`](architecture.md).
2. Read [`platform-capabilities.md`](platform-capabilities.md).
3. Read the focused area guide for the part of the platform you care about most.

### If you want to run ARCXA locally

1. Read [`getting-started.md`](getting-started.md).
2. Read [`deployment-and-operations.md`](deployment-and-operations.md).
3. Use [`api-surface.md`](api-surface.md) to find the right Swagger UI or endpoint family.

### If you want the vocabulary before the architecture

1. Read [`glossary-and-concepts.md`](glossary-and-concepts.md).
2. Read [`architecture.md`](architecture.md).
3. Continue into the domain guide you care about.

### If you need semantic mapping, lineage, or SoS details

1. Read [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md).
2. Read [`model-service-and-inference.md`](model-service-and-inference.md) if model-assisted matching matters.
3. Read [`lineage-and-governance.md`](lineage-and-governance.md).
4. Read [`systems-of-systems.md`](systems-of-systems.md) for the policy and contract-heavy control layer.

### If you are automating against ARCXA

1. Read [`api-surface.md`](api-surface.md).
2. Read [`sdk-and-automation.md`](sdk-and-automation.md).
3. Read the focused guide for the domain you are automating.

### If you want to contribute

1. Read [`repository-guide.md`](repository-guide.md).
2. Read [`architecture.md`](architecture.md).
3. Read the focused guide for the subsystem you intend to change before diving into module-local code.

## Current Accuracy Contract

These docs have been reviewed against the current repository structure, build scripts, public sync flow, frontend routes, CLI entry points, and exposed coordinator API modules.

Important scope notes:
- The private development workspace keeps the frontend in a sibling repository at `../graphica-frontend`.
- The public mirror packages that same frontend inside `frontend/`.
- The default local runner is `./run-local.sh`, and it currently exposes the coordinator REST API on `http://localhost:8082`.
- `./run-local.sh` disables auth by default for local development.
- The `admin` CLI and Python client still use older `graphica` naming and default to `localhost:8080`, which is why the docs call that out explicitly.
- `arcxa-shard` is intentionally built outside the Cargo workspace because of the `oxigraph` and RocksDB dependency split.
- The OpenLineage implementation exists in the repository, but it is not currently mounted on the main public router, so it is not documented here as part of the active public runtime contract.

When this curated directory and older ad hoc markdown disagree, treat this directory as the maintained public source of truth.

