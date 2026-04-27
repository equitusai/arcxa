# Repository Guide

This guide explains how the curated public documentation maps onto the repository and how the private development workspace differs from the public mirror.

## Use This Guide When

Use this guide when you want to:
- understand where a subsystem lives before you change it
- explain the difference between the private workspace and the public mirror
- understand which docs are curated versus historical
- update public-facing docs without accidentally leaning on outdated internal notes

## Table Of Contents

1. [Workspace Layout](#workspace-layout)
2. [Historical Naming Reality](#historical-naming-reality)
3. [Private Workspace Vs Public Mirror](#private-workspace-vs-public-mirror)
4. [Documentation Authority](#documentation-authority)
5. [Public Sync Behavior](#public-sync-behavior)
6. [Contributor Guidance](#contributor-guidance)

## Workspace Layout

| Path | Purpose |
| --- | --- |
| `arcxa-core/` | Shared contracts, connector abstractions, orchestration types, lineage types, and reusable domain primitives. |
| `arcxa-coordinator/` | Main control plane runtime and the largest API surface in the repository. |
| `arcxa-shard/` | RDF and SPARQL shard runtime built outside the Cargo workspace because of the RocksDB and `oxigraph` split. |
| `arcxa-model-service/` | Optional model inference service for semantic matching and model-assisted operations. |
| `arcxa-cli/` | CLI binaries for operator-facing administration and storage migration. |
| `arcxa-python/` | Python client package and automation examples. |
| `arcxa-migrations/` | Storage migration helpers and related migration logic. |
| `assets/` | Shared public-facing assets, including the architecture diagram used by the README. |
| `docs/public/` in the private workspace | Curated documentation source that syncs into `docs/` in the public mirror. |
| `docker/` | Compose files, demo packaging, health checks, and related container assets. |
| `kubernetes/` | Helm chart and cluster packaging assets. |
| `scripts/public-sync/` | Templates and release-note assets used by the public mirror sync flow. |

## Historical Naming Reality

Public branding uses `ARCXA`, but the repository still contains historical `graphica` names in several places.

That shows up in examples such as:
- environment variables like `GRAPHICA_API_BASE_URL`
- the Python package name `graphica`
- some Kubernetes labels and chart values
- internal module names and legacy scripts

When you see both names, treat:
- `ARCXA` as the public platform name
- `graphica` as a still-active implementation and packaging identifier

## Private Workspace Vs Public Mirror

The development setup most contributors use is not identical to the public mirror.

In the private workspace:
- the backend repository is this repository
- the frontend source often lives in the sibling repository at `../graphica-frontend`
- there may be a large amount of historical markdown, scratch notes, test assets, and local-only artifacts

In the public mirror:
- the frontend is packaged into `frontend/`
- curated docs are published under `docs/`
- shared assets such as the architecture image are included
- a large amount of engineering noise is intentionally trimmed away

That split is deliberate. It keeps the public repository navigable without forcing the engineering workspace to pretend it has no history.

## Documentation Authority

Not all markdown in the repository is equal.

The documentation authority for public-facing usage is:
1. `README.md`
2. `docs/public/` in the private workspace
3. the synced `docs/` tree in the public mirror

Older markdown elsewhere in the repository can still be useful as engineering history, but it should not be treated as the maintained public contract unless a curated doc links to it directly.

## Public Sync Behavior

`./sync-public.sh` is the bridge between the private workspace and the public mirror.

At a high level, it currently:
- stages a clean mirror repository
- syncs selected root files, crate directories, assets, Docker and Kubernetes assets, and the curated docs
- injects public templates for files such as the public README and release notes
- brings the sibling frontend repo into `frontend/`
- trims a large amount of internal or historical noise from the public output

A practical implication of that trimming behavior is:
- not every Markdown file in the engineering workspace is meant to appear publicly
- `docs/public/` exists precisely so public docs can be curated intentionally instead of leaking every historical note

## Contributor Guidance

A few habits will keep this repository easier to navigate:
- update the focused guide in `docs/public/` when you change a public-facing behavior, route family, default port, operator workflow, or repository split
- prefer linking from curated docs rather than sending readers into historical notes by default
- call out historical `graphica` names when they materially affect setup or automation
- when the private workspace and public mirror differ, document both realities clearly instead of pretending there is only one repository shape
- treat public docs as an interface: if a claim is easy to verify in code or scripts, verify it before publishing it

## Related Guides

- [`README.md`](README.md) for the curated documentation hub
- [`getting-started.md`](getting-started.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)
- [`architecture.md`](architecture.md)
