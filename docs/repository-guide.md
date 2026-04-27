# Repository Guide

This guide explains how the curated public documentation maps onto the repository itself.

## Workspace Layout

| Path | Purpose |
| --- | --- |
| `arcxa-core/` | Shared contracts, connector abstractions, workflow types, and reusable domain primitives. |
| `arcxa-coordinator/` | Main control plane runtime and API surface. |
| `arcxa-shard/` | RDF/SPARQL shard runtime built separately from the main workspace. |
| `arcxa-model-service/` | Optional model inference service used by semantic matching and related workflows. |
| `arcxa-cli/` | CLI tools for migrations, admin workflows, and thin operator access. |
| `arcxa-migrations/` | Migration utilities and supporting migration logic. |
| `assets/` | Shared public-facing assets, including the architecture diagram used in the README. |
| `docs/public/` | Curated documentation surface intended to stay current. |
| `scripts/public-sync/` | Files used when staging the public mirror. |

## Why There Are So Many Older Markdown Files

A large amount of historical documentation accumulated while the platform was evolving quickly.

That material is still valuable as engineering history, but it is not a good public entry point. The new `docs/public/` surface exists to solve that problem by separating:
- curated public guidance
- historical implementation notes
- internal working artifacts

## Public Mirror Structure

The public mirror is staged through `./sync-public.sh`.

That script now does three important documentation-related things:
- renders the public root `README.md` from `scripts/public-sync/public.README.md`
- syncs `docs/public/` into the public repo as `docs/`
- carries the shared architecture asset from `assets/`

That means the private development workspace can keep a curated public documentation layer without dragging every historical note into the open-source mirror.

## Contributor Guidance

If you are updating public-facing docs:
- prefer `docs/public/` over adding more root-level markdown
- link from the root `README.md` or the docs hub when the new guide should be discoverable
- keep implementation diaries separate from the curated public path
- update `scripts/public-sync/public.README.md` when the public root narrative changes

## Suggested Next Reading

- [`architecture.md`](architecture.md) for the runtime model
- [`platform-capabilities.md`](platform-capabilities.md) for the functional surface
- [`deployment-and-operations.md`](deployment-and-operations.md) for the operator view
