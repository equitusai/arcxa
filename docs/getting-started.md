# Getting Started

This guide gets a local ARCXA environment running with the smallest amount of guesswork.

## Use This Guide When

Use this guide when you want to:
- build the repository successfully in the current workspace
- start the default local runtime without guessing which script to use
- understand the current port and auth assumptions
- verify quickly that the coordinator and UI are actually reachable

## Table Of Contents

1. [What You Are Running](#what-you-are-running)
2. [Private Workspace vs Public Mirror](#private-workspace-vs-public-mirror)
3. [Prerequisites](#prerequisites)
4. [Recommended Build Path](#recommended-build-path)
5. [Recommended Run Paths](#recommended-run-paths)
6. [First Verification Steps](#first-verification-steps)
7. [Running the Web UI](#running-the-web-ui)
8. [Running Tests](#running-tests)
9. [Local Feature Flags That Matter](#local-feature-flags-that-matter)
10. [CLI Entry Points](#cli-entry-points)
11. [Current Local Reality Checks](#current-local-reality-checks)

## What You Are Running

A normal local ARCXA session involves:
- the `arcxa-coordinator` control plane
- two local `arcxa-shard` processes for RDF and SPARQL storage
- the optional `arcxa-model-service` for semantic matching
- Kafka, ZooKeeper, and Schema Registry through Docker Compose

The default orchestration script for that topology is `./run-local.sh`.

## Private Workspace vs Public Mirror

There are two repository shapes you may encounter:

| Shape | Frontend location | Docs location |
| --- | --- | --- |
| Private development workspace | sibling repo at `../graphica-frontend` | `docs/public/` |
| Public mirror | `frontend/` inside the repo | `docs/` |

This guide is written so both shapes are understandable. When a step differs, it is called out explicitly.

## Prerequisites

You will want:
- Rust `1.91.1` or newer
- the stable Cargo toolchain
- Docker and Docker Compose
- `curl`
- Node.js and `npm` if you want the web UI
- ODBC driver-manager and vendor drivers only if you intend to build ODBC-backed connectors locally

The repository includes `rust-toolchain.toml`, and the top-level scripts already clear the Conda and OpenSSL environment variables that are known to interfere with local builds here.

## Recommended Build Path

Use the repo scripts first. They encode the build assumptions that match this workspace.

### 1. Build the main workspace

```bash
./build.sh
```

What this builds:
- `arcxa-coordinator`
- `arcxa-model-service`
- workspace libraries such as `arcxa-core`

What it does not build inside the workspace:
- `arcxa-shard`

That shard binary is built separately because it is intentionally excluded from the workspace.

### 2. Build the shard explicitly when you need to inspect it directly

```bash
cd arcxa-shard
cargo +stable build
cd ..
```

`./run-local.sh` will also build the shard for you.

## Recommended Run Paths

| Command | Use when |
| --- | --- |
| `./run-local.sh` | You want the normal development topology with one coordinator, two shards, Docker-backed Kafka infrastructure, and the model service. |
| `./run-local-ha.sh` | You want the three-coordinator Raft-oriented HA demo topology. Build with `ENABLE_HA=true ./build.sh` first. |
| `./run-single-node.sh` | You want a tighter local shape without the full sharded topology. |
| `start-coordinator-with-test-api.sh` | You need the coordinator with test-only API helpers enabled for targeted development work. |

### Start the default topology

```bash
./run-local.sh
```

The script currently does all of the following:
- clears conflicting local compiler and linker environment variables
- builds workspace binaries and the shard
- starts Kafka, ZooKeeper, and Schema Registry when needed
- starts the coordinator, two shards, and the model service
- downloads ONNX Runtime on first use if it is missing locally

## First Verification Steps

The default local runner exposes the coordinator REST API on `http://localhost:8082`.

Verify the core surface:

```bash
curl http://localhost:8082/health
curl http://localhost:8082/openapi.yaml
```

Useful follow-up endpoints:
- `http://localhost:8082/health/live`
- `http://localhost:8082/health/ready`
- `http://localhost:8082/metrics`
- `http://localhost:8082/api/v1/datasources/swagger-ui`
- `http://localhost:8082/api/v1/workflows/swagger-ui`
- `http://localhost:8082/api/v1/sos/swagger-ui`

## Running the Web UI

In the public mirror:

```bash
cd frontend
npm install
npm run dev
```

In the private development workspace:

```bash
cd ../graphica-frontend
npm install
npm run dev
```

The frontend talks to the coordinator API, so make sure the backend topology is already up.

## Running Tests

Use `./test.sh` rather than raw `cargo test` first. It encodes the same environment cleanup used by the build flow.

Examples:

```bash
./test.sh
./test.sh coordinator
./test.sh core
./test.sh coordinator -- --nocapture
```

## Local Feature Flags That Matter

The main build and test scripts recognize these environment variables:
- `ENABLE_AUDIT=true|false`
- `ENABLE_HA=true|false`
- `ENABLE_ODBC=true|false`
- `ENABLE_DB2=true|false` as a backward-compatible alias for `ENABLE_ODBC`

Practical guidance:
- keep `ENABLE_ODBC=false` unless you actually need Oracle, DB2, or SAP HANA locally
- enable `ENABLE_HA=true` only for the HA topology
- leave audit enabled unless you are intentionally testing without it

## CLI Entry Points

The repository currently ships two CLI binaries under `arcxa-cli`:
- `migrate` for RocksDB migration and storage maintenance work
- `admin` for operator-facing systems-of-systems API workflows

Example help commands:

```bash
cargo +stable run -p arcxa-cli --bin migrate -- --help
cargo +stable run -p arcxa-cli --bin admin --no-default-features -- --help
```

Important note:
- the `admin` CLI assumes an authenticated coordinator API and defaults to `http://localhost:8080/api/v1`
- `./run-local.sh` currently starts the local coordinator on `http://localhost:8082` with `ENABLE_AUTH=false`
- for the default local runner, direct `curl` and Swagger validation are usually simpler unless you are targeting another authenticated environment

## Current Local Reality Checks

A few current realities are worth keeping in mind:
- local docs examples use `8082` for the coordinator because that is what `./run-local.sh` currently does
- some older tools and code still default to `8080`, so pass the base URL explicitly when in doubt
- local auth is disabled by default, which is convenient for development but not representative of a secured deployment
- the repository still contains historical `graphica` naming in environment variables and helper tooling

## Related Guides

- [`glossary-and-concepts.md`](glossary-and-concepts.md)
- [`architecture.md`](architecture.md)
- [`api-surface.md`](api-surface.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)

