# Model Service And Inference

This guide covers the optional model service that supports embedding generation and model-assisted semantic matching in ARCXA.

## Use This Guide When

Use this guide when you want to:
- understand what the model service does and does not do
- decide whether you need it in a local or deployed topology
- explain why inference is isolated from the coordinator
- understand the current runtime contract for model-assisted matching

## Table Of Contents

1. [What The Model Service Is](#what-the-model-service-is)
2. [Why It Is Separate](#why-it-is-separate)
3. [Current Runtime Contract](#current-runtime-contract)
4. [Configuration And Local Usage](#configuration-and-local-usage)
5. [Where It Fits In The Platform](#where-it-fits-in-the-platform)
6. [Current Boundaries](#current-boundaries)

## What The Model Service Is

`arcxa-model-service` is a standalone gRPC runtime that provides model inference support for semantic workflows.

In the current codebase, its main job is embedding generation for semantic matching rather than acting as a general-purpose ML platform.

It is most relevant when teams care about:
- richer field and semantic similarity behavior
- cleaner separation between inference dependencies and API orchestration
- independent scaling of model-heavy workloads

## Why It Is Separate

Separating inference from the coordinator gives the platform a cleaner operating model:
- the coordinator stays focused on APIs, orchestration, metadata, and governance
- ONNX Runtime and model dependencies are isolated to a dedicated service
- inference capacity can scale differently from the control plane
- model updates can be reasoned about without reshaping the entire coordinator runtime

This is also why the default local topology starts a separate model-service process instead of linking everything into the coordinator binary.

## Current Runtime Contract

The service currently exposes gRPC methods for:
- single-text embedding generation
- batch embedding generation
- health inspection
- model-info inspection

Operationally, the current public facts that matter are:
- the default port is `50051`
- the service is optional for the broader platform to boot
- semantic-matching-heavy workflows are better with it available
- the default local runner starts it for you

## Configuration And Local Usage

The service expects a model path via:
- `--model-path`, or
- `GRAPHICA_MODEL_PATH`

Other current command-line options include:
- `--port`
- `--model-name`
- `--cache-size`

For the standard developer path, you usually do not need to start it manually because `./run-local.sh` already does that.

If you do want to run it directly, use the package-local instructions in `arcxa-model-service/README.md` in the private workspace, or inspect the crate directly in the public mirror.

## Where It Fits In The Platform

The model service primarily improves:
- semantic mapping
- candidate generation for field similarity
- model-assisted matching workflows that benefit from embeddings

It is therefore best understood as part of the semantics stack, not as a general workflow engine feature.

## Current Boundaries

A few current realities matter:
- this is a gRPC service, not a public REST subsystem parallel to the coordinator
- the broader public docs should not imply that every ARCXA feature depends on it
- older `graphica` naming still appears in its environment variables and proto namespace
- it improves semantic workflows, but it is not the source of truth for mappings, workflows, or governance state

## Related Guides

- [`architecture.md`](architecture.md)
- [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)
- [`sdk-and-automation.md`](sdk-and-automation.md)

