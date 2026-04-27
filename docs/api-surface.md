# API Surface

ARCXA exposes a broad coordinator API. This guide is not a full endpoint reference; the Swagger UIs are better for that. Instead, this page maps the API families, their auth posture, and a few important caveats so you know where to look.

## Use This Guide When

Use this guide when you want to:
- find the right API family quickly
- understand which routes are easiest to discover through Swagger and which are not
- understand how local no-auth behavior differs from secured deployments
- orient yourself before writing automation or frontend integrations

## Table Of Contents

1. [Versioning And Discovery](#versioning-and-discovery)
2. [Auth Model In Practice](#auth-model-in-practice)
3. [Swagger UI Map](#swagger-ui-map)
4. [API Families Without Dedicated Swagger UIs](#api-families-without-dedicated-swagger-uis)
5. [Major Coordinator API Families](#major-coordinator-api-families)
6. [Important Current Boundary](#important-current-boundary)
7. [Practical Guidance](#practical-guidance)

## Versioning And Discovery

The coordinator exposes:
- health endpoints such as `/health`, `/health/live`, and `/health/ready`
- a root OpenAPI index at `/openapi.yaml`
- module-specific Swagger UIs under `/api/v1/.../swagger-ui`

After `./run-local.sh`, the local coordinator REST surface is typically:

```text
http://localhost:8082
```

## Auth Model In Practice

The current API shape has two important layers:
- public runtime inspection routes such as `/health` and `/openapi.yaml`
- the main `/api/v1/...` application surface, which is protected by auth middleware when auth is enabled

That means:
- in a secured deployment, most Swagger UIs under `/api/v1/.../swagger-ui` require auth
- `./run-local.sh` disables auth by default, so local Swagger exploration is easier than a production environment
- `/metrics` lives outside `/api/v1`, but the current router still treats it as an authenticated surface when auth is enabled

## Swagger UI Map

| Area | Swagger UI | Notes |
| --- | --- | --- |
| Data sources | `/api/v1/datasources/swagger-ui` | Datasource CRUD, connection testing, discovery, schema inference, and query endpoints. |
| Workflows | `/api/v1/workflows/swagger-ui` | Workflow CRUD, validation, execution, schedules, approvals, and execution control. |
| Ontology | `/api/v1/ontology/swagger-ui` | Ontology registration, activation, merge, and validation. |
| Unified mapping | `/api/v1/mapping/swagger-ui` | Field mapping, unified mapping sessions, manual mapping, and load-oriented flows. |
| Loader | `/api/v1/loader/swagger-ui` | Loader jobs, DLQ inspection, and loader health. |
| DDL | `/api/v1/ddl/swagger-ui` | SQL DDL generation surfaces. |
| File library | `/api/v1/file-library/swagger-ui` | Managed file ingress, folders, scanning, search, and lineage views. |
| Row and workflow lineage | `/api/v1/lineage/swagger-ui` | Row, record, run, column, and schema-evolution lineage surfaces. |
| Field lineage | `/api/v1/field-lineage/swagger-ui` | Entity field history, resolved entities, and conflict workflows. |
| Governance | `/api/v1/governance/swagger-ui` | SPARQL, RDF stats, auto-save status, and save triggers. |
| Systems-of-systems | `/api/v1/sos/swagger-ui` | SoS catalog, validation, analytics, policy and contract governance, and reconcile. |
| R2RML | `/api/v1/r2rml/swagger-ui` | Relational-to-RDF mapping flows. |

## API Families Without Dedicated Swagger UIs

Some active surfaces do not currently have their own module-specific Swagger UI even though they are part of the public router.

Examples:
- dataset and entity routes under `/api/v1/datasets` and `/api/v1/entities`
- profiling routes under `/api/v1/profiling`
- schema profiling routes under `/api/v1/schema/...`
- connector registry routes under `/api/v1/connectors`
- cluster, WAL, audit, and other admin routes under `/api/v1/admin/...` and `/api/v1/cluster/...`

For those, use the root OpenAPI index plus the code-matched guides in `docs/public/`.

## Major Coordinator API Families

### Auth, Health, And Runtime Inspection

Examples:
- `/auth/login`
- `/auth/setup`
- `/health`
- `/metrics`
- `/openapi.yaml`

Use this family for platform entry, basic liveness, and top-level spec discovery.

### Data Sources, Connectors, Discovery, And Profiling

Examples:
- `/api/v1/datasources`
- `/api/v1/connectors`
- `/api/v1/profiling/profile`
- `/api/v1/schema/...`

This is the right API family when you need to register sources, inspect connector capabilities, run connection tests, infer schemas, start async discovery, or generate profiling metadata.

### File Library, Datasets, Catalogue, And Entities

Examples:
- `/api/v1/file-library/...`
- `/api/v1/datasets`
- `/api/v1/entities`

This is the managed data surface that sits above raw connectors.

### Ontology, Mapping, R2RML, And Loaders

Examples:
- `/api/v1/ontology`
- `/api/v1/mapping`
- `/api/v1/r2rml`
- `/api/v1/loader`
- `/api/v1/ddl`

These APIs cover semantic alignment, multi-source consolidation, target planning, and loader orchestration.

### Workflows And Executions

Examples:
- `/api/v1/workflows`
- `/api/v1/executions`
- `/api/v1/approvals`

This family covers workflow CRUD, validate, dry-run, execute, schedules, progress, and execution lifecycle operations.

### Lineage And Governance

Examples:
- `/api/v1/lineage`
- `/api/v1/field-lineage`
- `/api/v1/governance`

This is the right surface for provenance, impact analysis, row and column lineage, schema evolution, and graph-oriented governance flows.

### Systems-Of-Systems

Examples:
- `/api/v1/sos/systems`
- `/api/v1/sos/interfaces`
- `/api/v1/sos/contracts`
- `/api/v1/sos/policies`
- `/api/v1/sos/validate`
- `/api/v1/sos/compatibility-matrix`

This family owns the SoS catalog, validation reports, analytics, and governance workflows.

### Administrative And Maintenance Routes

Examples visible in the coordinator include:
- audit query and export routes
- WAL status and replay routes
- cluster topology and scaling routes
- secret-store administration routes
- temporal admin routes
- Kafka and Raft-oriented routes when those features are enabled

Treat these as operator-facing surfaces, not as a general application API.

## Important Current Boundary

The repository contains an OpenLineage implementation module, but it is not currently mounted on the main public router. That means it is not part of the active public runtime contract described here.

This is worth calling out because the codebase contains the implementation, but public documentation should describe the running public surface, not every dormant or partially wired module.

## Practical Guidance

If you are not sure where to begin:
1. use `/openapi.yaml` to orient yourself at the top level
2. jump to the module-specific Swagger UI for the subsystem you need when one exists
3. read the matching focused guide in this docs directory for context and caveats

Recommended pairings:
- datasources and datasets: [`data-sources-and-datasets.md`](data-sources-and-datasets.md)
- semantic mapping: [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md)
- model service: [`model-service-and-inference.md`](model-service-and-inference.md)
- workflows: [`workflows-and-execution.md`](workflows-and-execution.md)
- lineage: [`lineage-and-governance.md`](lineage-and-governance.md)
- SoS: [`systems-of-systems.md`](systems-of-systems.md)
- automation: [`sdk-and-automation.md`](sdk-and-automation.md)


## Related Guides

- [`getting-started.md`](getting-started.md)
- [`sdk-and-automation.md`](sdk-and-automation.md)
- [`systems-of-systems.md`](systems-of-systems.md)
- [`deployment-and-operations.md`](deployment-and-operations.md)
