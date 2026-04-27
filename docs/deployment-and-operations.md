# Deployment And Operations

This guide focuses on the practical entry points for building, running, and operating ARCXA.

## Local Runtime Entry Points

| Script | Purpose |
| --- | --- |
| `./build.sh` | Build the main workspace using the repo's preferred environment handling. |
| `./run-local.sh` | Run the default local development topology. |
| `./run-local-ha.sh` | Run the HA-oriented local topology. |
| `./run-single-node.sh` | Run a tighter single-node topology when that is the right fit. |
| `./test.sh` | Run focused test targets. |
| `./sync-public.sh` | Stage the curated public mirror, including the public docs and architecture asset. |

## Docker And Compose

The repository includes multiple container entry points:
- `Dockerfile`
- `Dockerfile.coordinator`
- `Dockerfile.shard`
- `Dockerfile.ml`
- `docker-compose.yml`
- `docker-compose.test.yml`
- `docker-compose-db2.yml`

In practice, `docker-compose.yml` is the easiest entry point for a more complete local dependency stack.

## Kubernetes

Deployment assets live under:
- `kubernetes/`

Those assets are the right place to start if you are packaging ARCXA for cluster-based environments.

## Health And Discovery Surfaces

Useful runtime endpoints include:
- `/health`
- `/health/live`
- `/health/ready`
- `/openapi.yaml`

Module-scoped Swagger surfaces are also available under versioned API paths.

## Metrics

The coordinator exposes metrics suitable for runtime observability. When you are packaging ARCXA for shared environments, health and metrics should be part of the default deployment contract rather than an afterthought.

## Storage Reality

A few operational realities are worth calling out directly:
- the coordinator persists state for metadata, workflows, and operational features
- the shard owns graph storage concerns
- the SoS runtime now has explicit reconcile/recovery controls because projected graph state matters operationally
- the public docs now describe the current curated surface, but code-level configuration still remains the authoritative source for low-level runtime details

## Recommended Operational Reading Order

1. Read [`getting-started.md`](getting-started.md) to get a working local environment.
2. Read [`architecture.md`](architecture.md) so the deployment split is clear.
3. Read [`systems-of-systems.md`](systems-of-systems.md) if SoS validation is part of your rollout.
4. Use the root `README.md` and this guide as the public operational surface before diving into historical runbooks elsewhere in the repo.
