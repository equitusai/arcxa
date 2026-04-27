# Frontend And CLI

ARCXA has two major operator-facing layers above the coordinator API:
- the React frontend
- the `arcxa-cli` binaries

This guide explains what each one is for, where each one is strongest today, and how to choose the right surface without guessing.

## Use This Guide When

Use this guide when you want to:
- decide whether a task is better served through the UI or the CLI
- understand what the frontend currently exposes across the platform, not just for SoS
- understand what the `admin` and `migrate` CLIs do and do not cover
- orient operator workflows before building additional tooling or onboarding new teams

## Table Of Contents

1. [Operator Experience Overview](#operator-experience-overview)
2. [Frontend Location And Runtime Model](#frontend-location-and-runtime-model)
3. [Frontend Route Surface](#frontend-route-surface)
4. [SoS Workspace](#sos-workspace)
5. [CLI Binaries](#cli-binaries)
6. [Choosing Between Frontend And CLI](#choosing-between-frontend-and-cli)
7. [Current Boundaries](#current-boundaries)

## Operator Experience Overview

| Surface | Best for | Current strength |
| --- | --- | --- |
| Frontend | Exploration, guided workflows, cross-domain navigation, dashboards, operator review | Strongest when teams want an interactive control plane and do not want to assemble API payloads manually. |
| `admin` CLI | Authenticated SoS maintenance, audit, analytics, and reconcile workflows from a terminal | Strongest for scripted or terminal-first operations around the SoS domain. |
| `migrate` CLI | Storage migration and migration inspection tasks | Focused and intentionally narrow. |

## Frontend Location And Runtime Model

In the public mirror, the frontend lives under:
- `frontend/`

In the private development workspace, the same frontend source currently lives in the sibling repository:
- `../graphica-frontend`

The frontend is a React and Vite application that talks to the coordinator over HTTP.

Important current local-development caveat:
- the frontend code still defaults to `http://localhost:8080/api/v1`
- the default `./run-local.sh` topology exposes the coordinator on `http://localhost:8082`

In practice, that means local frontend work should usually set `VITE_API_BASE_URL` explicitly when using the standard local runner.

## Frontend Route Surface

The main application routes currently include:
- `/`
- `/data-catalogue`
- `/catalogue`
- `/catalogue/:datasetId`
- `/entities`
- `/datasources`
- `/file-library`
- `/models`
- `/lineage`
- `/fusion`
- `/fusion-new`
- `/workflows`
- `/sos-validation`
- `/ontologies`
- `/sparql`
- `/settings`

The frontend is most useful when teams want a shared, navigable workspace across:
- data onboarding
- semantic mapping
- lineage inspection
- workflow authoring and review
- SoS governance and analytics

In secured deployments, protected routes should be treated as authenticated even though local development often runs with auth disabled.

## SoS Workspace

The SoS workspace is one of the richest operator surfaces in the UI right now.

Its current tab set includes:
- Pair Workbench
- Reports
- Catalog
- Policies
- Analytics
- Compatibility Matrix
- Operations

Those tabs now support:
- interface-pair validation and dry-run workflows
- persisted report inspection, change summaries, and lineage trends
- catalog navigation across systems, interfaces, contracts, and policies
- policy review and governance drill-down
- dependency and what-if analytics
- reconcile controls and governance audit views
- contract and policy signing-key status and rotation

This is important because the SoS UI is no longer just a thin demo surface. It has become a practical operator console for the SoS domain.

## CLI Binaries

The repository currently ships two CLI binaries under `arcxa-cli`.

### `admin`

The `admin` binary is the operator-facing CLI. It is currently centered on SoS operations.

Major command families include:
- `sos reconcile`
- `sos catalog systems|interfaces|contracts|policies`
- `sos validate interface-pair`
- `sos reports get|history|lineage`
- `sos analytics compatibility-matrix|dependency-graph|what-if`
- `sos contracts audit|get|lookup|approval-requests|signatures|signing-key`
- `sos policies audit|get|approval-requests|attestations|validate|signing-key`

Important current default:
- the CLI still uses historical naming and defaults `GRAPHICA_API_BASE_URL` to `http://localhost:8080/api/v1`
- when using `./run-local.sh`, operators should usually point it at `http://localhost:8082/api/v1`

### `migrate`

The `migrate` binary is separate and much narrower.

It exists for migration-oriented storage tasks such as:
- checking migration status
- running a migration
- running a dry-run migration for inspection

This binary is not a general operator shell. It is a focused maintenance tool.

## Choosing Between Frontend And CLI

A simple rule of thumb works well:
- use the frontend when the job is exploratory, collaborative, or cross-domain
- use the CLI when the job is repeatable, terminal-first, or needs to plug into scripting and automation

A few concrete examples:
- reviewing SoS report trends with another team: frontend
- rerunning reconcile in a controlled environment: CLI
- rotating an SoS signing key in a maintenance window: either, depending on your operating style
- tracing a dataset-to-workflow-to-lineage question interactively: frontend
- capturing audit output in CI or an incident runbook: CLI

## Current Boundaries

A few current boundaries are worth keeping in view:
- the frontend covers much more of the platform overall, but the CLI is intentionally focused and does not attempt full API parity
- the SoS workspace is currently the deepest end-to-end operator experience in the CLI
- both the frontend and CLI still reflect some historical `graphica` naming and `8080` defaults, so local-development setup benefits from explicit base-URL configuration

## Related Guides

- [`getting-started.md`](getting-started.md)
- [`api-surface.md`](api-surface.md)
- [`sdk-and-automation.md`](sdk-and-automation.md)
- [`systems-of-systems.md`](systems-of-systems.md)
