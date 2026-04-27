# Getting Started

This guide gets a local ARCXA environment running with the smallest number of moving parts.

## Prerequisites

You will want:
- Rust `1.91.1` or newer
- `cargo` on the stable toolchain
- Docker and Docker Compose
- `curl`
- Node.js and `npm` if you are also running the web UI

The repository includes `rust-toolchain.toml`, so `rustup` can select the expected toolchain automatically.

## Build The Backend Workspace

From the repository root:

```bash
./build.sh
```

That builds the main workspace components. The shard runtime is separate because it uses a different RocksDB binding through `oxigraph`.

## Build The Shard Separately

```bash
cd arcxa-shard
cargo +stable build
cd ..
```

## Start The Default Local Topology

```bash
./run-local.sh
```

The default local runner starts Docker-backed infrastructure and launches the ARCXA runtime in single-coordinator development mode.

For the HA-oriented local topology, use:

```bash
./run-local-ha.sh
```

## Optional: Run The Web UI

In the public mirror, the operator UI lives in `frontend/`.

```bash
cd frontend
npm install
npm run dev
```

## First Verification Steps

Once the coordinator is up, verify the basic surface:

```bash
curl http://localhost:8080/health
curl http://localhost:8080/openapi.yaml
```

Useful follow-up endpoints:
- `http://localhost:8080/health/live`
- `http://localhost:8080/health/ready`
- `http://localhost:8080/api/v1/datasources/swagger-ui`
- `http://localhost:8080/api/v1/workflows/swagger-ui`

## Local Entry Points You Will Use Often

| Script | Purpose |
| --- | --- |
| `./build.sh` | Build the main workspace components with the repo's preferred environment handling. |
| `./run-local.sh` | Bring up the standard local development topology. |
| `./run-local-ha.sh` | Bring up the HA-oriented local topology. |
| `./test.sh` | Run focused Rust test targets without re-creating the build environment manually. |
| `./sync-public.sh` | Stage the public mirror with the curated documentation and selected source surface. |

## What To Read Next

After you have a working local environment:
1. Read [`architecture.md`](architecture.md) to understand the runtime split.
2. Read [`platform-capabilities.md`](platform-capabilities.md) for the functional surface.
3. Read [`deployment-and-operations.md`](deployment-and-operations.md) before packaging this for shared environments.
