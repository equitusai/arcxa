# SDK And Automation

ARCXA is not only a web UI and a REST API. The repository also ships automation-friendly surfaces that let teams script common operations, build repeatable runbooks, and integrate the platform into larger delivery workflows.

## Use This Guide When

Use this guide when you want to:
- choose between HTTP, CLI, and Python-client automation paths
- understand current automation coverage and boundaries
- understand the historical `graphica` naming in the Python package and CLI environment variables
- build integrations without guessing which surface is most complete today

## Table Of Contents

1. [Automation Surfaces](#automation-surfaces)
2. [Choosing The Right Surface](#choosing-the-right-surface)
3. [Direct HTTP Automation](#direct-http-automation)
4. [Python Client](#python-client)
5. [CLI Automation](#cli-automation)
6. [Automation Patterns That Fit The Current Platform](#automation-patterns-that-fit-the-current-platform)
7. [Current Boundaries](#current-boundaries)

## Automation Surfaces

The repository currently exposes three practical automation paths:
- direct HTTP use against the coordinator API
- the `admin` and `migrate` CLI binaries in `arcxa-cli`
- the Python client under `arcxa-python`

Each one is useful for a different audience.

## Choosing The Right Surface

| Surface | Best when | Current tradeoff |
| --- | --- | --- |
| Direct HTTP | You want exact API control and the broadest current coverage | Requires you to manage payloads, auth, and retry behavior directly |
| CLI | You want terminal-friendly SoS workflows and operational tasks | Intentionally narrower than the full coordinator API |
| Python client | You want lightweight scripting in Python for selected coordinator domains | Does not yet wrap every public domain with parity |

## Direct HTTP Automation

Direct HTTP is still the most complete automation surface because the coordinator API is the underlying contract for everything else.

That is usually the right choice when you need:
- exact request and response control
- coverage of domains the Python client or CLI does not yet wrap
- fast validation against Swagger-documented payloads

A practical workflow is:
1. inspect the relevant Swagger UI under `/api/v1/.../swagger-ui`
2. verify payload shape against the live coordinator
3. codify the request in your preferred automation environment

Important local-development note:
- the standard local runner exposes the coordinator root at `http://localhost:8082`
- secured deployments should still be assumed to require auth for `/api/v1/...` routes

## Python Client

The Python client lives under:
- `arcxa-python/`

Important naming reality:
- the public platform name is `ARCXA`
- the Python package name is still `graphica`

That means imports still look like:

```python
from graphica import Client
```

Important base-URL behavior:
- the client expects the coordinator root URL, not `/api/v1`
- `Client.health()` calls `/health`
- module-specific API wrappers build their own `/api/v1/...` paths internally

Current default in code:
- `Client()` still defaults to `http://localhost:8080`

Recommended for the standard local topology:
- pass `http://localhost:8082` explicitly

The client currently wires modules for selected coordinator domains, including:
- ontology
- mapping
- lineage
- loader
- workflows
- datasources
- datasets
- GDPR-related flows
- R2RML-related flows

This makes it useful when teams want:
- lightweight scripting around coordinator APIs
- test fixtures or data-pipeline automation in Python
- programmatic access without driving the frontend manually

## CLI Automation

The CLI lives under:
- `arcxa-cli/`

The main automation-relevant binaries are:
- `admin` for operator-facing SoS workflows
- `migrate` for migration-oriented storage maintenance

The `admin` CLI is useful when teams want:
- authenticated SoS governance and maintenance flows from the terminal
- repeatable reconcile, analytics, audit, and report-inspection commands
- a stable wrapper around common SoS operator tasks

Important current default:
- the CLI still uses `GRAPHICA_API_BASE_URL` and defaults to `http://localhost:8080/api/v1`
- when using `./run-local.sh`, operators should usually point it at `http://localhost:8082/api/v1`

## Automation Patterns That Fit The Current Platform

The current platform works especially well with a few patterns:
- curl-first exploration that graduates into Python or shell automation
- CI or scheduled automation for SoS report inspection, analytics, or governance review
- operator runbooks that use the CLI for reconcile and audit tasks while leaving deeper domain automation to direct HTTP or Python
- frontend-driven exploration followed by API or CLI codification of the same workflow

The repository also contains agent and MCP-adjacent assets, but the core public automation story should still be understood as:
- coordinator HTTP APIs
- the CLI binaries
- the Python client

## Current Boundaries

A few current realities are worth keeping in mind:
- the Python client does not yet cover every coordinator domain with parity
- the CLI is intentionally narrow and SoS-heavy rather than a full generic platform shell
- historical `graphica` naming still appears in imports and environment variables
- some defaults still point at `8080`, while the standard local runner uses `8082`

## Related Guides

- [`api-surface.md`](api-surface.md)
- [`frontend-and-cli.md`](frontend-and-cli.md)
- [`getting-started.md`](getting-started.md)
- [`repository-guide.md`](repository-guide.md)
