# ARCXA Documentation Hub

Welcome. This directory is the curated, maintained documentation surface for ARCXA.

If you are browsing this repository for the first time, start here instead of at the older markdown scattered across the repo. Those historical files may still be useful as engineering notes, but this section is intended to be the public reference set we keep current.

## Table Of Contents

1. [Start Here](#start-here)
2. [Guide Map](#guide-map)
3. [Recommended Reading Paths](#recommended-reading-paths)
4. [Documentation Policy](#documentation-policy)

## Start Here

ARCXA is a governed data platform for teams that need to:
- connect operational data sources
- discover and materialize datasets
- align source-native fields with semantic models
- orchestrate repeatable transformation and loading workflows
- inspect lineage, provenance, and policy-driven validation

If that is the problem you are trying to solve, the guide map below is the fastest way into the repository.

## Guide Map

| Guide | Best for | What you will get |
| --- | --- | --- |
| [`getting-started.md`](getting-started.md) | First-time users | Prerequisites, build, run, and first validation steps. |
| [`architecture.md`](architecture.md) | Architects and platform leads | Runtime topology, control-plane/data-plane split, and primary data flows. |
| [`platform-capabilities.md`](platform-capabilities.md) | Delivery teams | A structured view of what ARCXA covers across ingestion, mapping, workflows, lineage, and governance. |
| [`systems-of-systems.md`](systems-of-systems.md) | Integration and governance teams | SoS catalog, policies, contracts, validation history, analytics, and operator controls. |
| [`deployment-and-operations.md`](deployment-and-operations.md) | Operators | Local scripts, health, metrics, Docker, Kubernetes, and deployment considerations. |
| [`repository-guide.md`](repository-guide.md) | Contributors | Workspace layout, crate responsibilities, and how the public documentation surface is organized. |

## Recommended Reading Paths

### If you are evaluating the platform

1. Read [`architecture.md`](architecture.md)
2. Read [`platform-capabilities.md`](platform-capabilities.md)
3. Read [`systems-of-systems.md`](systems-of-systems.md) if validation governance is part of your use case

### If you want to run ARCXA locally

1. Read [`getting-started.md`](getting-started.md)
2. Read [`deployment-and-operations.md`](deployment-and-operations.md)
3. Use the root `README.md` for quick links into scripts and assets

### If you want to contribute or extend the codebase

1. Read [`repository-guide.md`](repository-guide.md)
2. Read [`architecture.md`](architecture.md)
3. Use the module-local source trees as the next level of detail

## Documentation Policy

For the public surface, we are optimizing for:
- current architecture over historical narrative
- task-oriented guides over long implementation diaries
- stable entry points over exhaustive internal notes

When this documentation and older repo markdown disagree, treat this directory as the maintained source of truth unless a newer code-adjacent document explicitly says otherwise.
