# Workflows And Execution

ARCXA's workflow surface is broader than simple workflow CRUD. It includes definition validation, dry-run, synchronous and asynchronous execution, schedule management, approvals, streaming controls, and execution lifecycle inspection.

## Use This Guide When

Use this guide when you want to:
- understand the current workflow API shape before integrating with it
- explain how validation, dry-run, execution, and approvals fit together
- understand where workflow control surfaces exist for operators
- connect workflows to mapping, loaders, lineage, and SoS validation

## Table Of Contents

1. [Workflow API Shape](#workflow-api-shape)
2. [Workflow Lifecycle](#workflow-lifecycle)
3. [Execution Modes And Variants](#execution-modes-and-variants)
4. [Validation And Dry-Run](#validation-and-dry-run)
5. [Scheduling And Approvals](#scheduling-and-approvals)
6. [Progress And Execution Control](#progress-and-execution-control)
7. [Relationship To Other Subsystems](#relationship-to-other-subsystems)
8. [Current Boundaries And Caveats](#current-boundaries-and-caveats)

## Workflow API Shape

The coordinator exposes a dedicated workflow module under:
- `/api/v1/workflows`

The workflow router currently includes:
- workflow create, list, details, get, update, and delete
- validate, dry-run, and test-step endpoints
- synchronous and asynchronous execution
- schedule CRUD and schedule preview
- execution history and active execution views
- progress endpoints
- stop, pause, resume, abort, and cancel controls
- approval list, approve, reject, and cancel flows
- route and stream-oriented workflow endpoints

Representative routes include:
- `POST /api/v1/workflows`
- `GET /api/v1/workflows`
- `GET /api/v1/workflows/:id/details`
- `POST /api/v1/workflows/validate`
- `POST /api/v1/workflows/dry-run`
- `POST /api/v1/workflows/test-step`
- `POST /api/v1/workflows/:id/execute`
- `POST /api/v1/workflows/:id/execute-async`
- `GET /api/v1/executions`
- `GET /api/v1/executions/:id`
- `POST /api/v1/executions/:id/stop`
- `POST /api/v1/executions/:id/pause`
- `POST /api/v1/executions/:id/resume`
- `POST /api/v1/executions/:id/abort`

## Workflow Lifecycle

A typical workflow lifecycle looks like this:

1. Define the workflow.
2. Validate it.
3. Dry-run it when you want a lower-risk check.
4. Execute it synchronously or asynchronously.
5. Track progress and logs.
6. Review approvals or intervene if the execution lifecycle needs operator action.

That lifecycle matters because workflows in this platform can touch connectors, loaders, lineage, semantic mapping, and SoS validation. They are not just a wrapper around a single SQL job.

## Execution Modes And Variants

Publicly visible variants in the current codebase include:
- standard workflow CRUD and execution routes
- route-oriented and streaming workflow endpoints under `routing-workflows`
- asynchronous execution tracking through the executions surface
- workflow approvals and approval statistics

This is one of the reasons the workflow Swagger surface is more useful than a short prose list: the workflow API really is a subsystem, not a thin wrapper around one background worker.

## Validation And Dry-Run

The workflow module includes:
- definition validation
- dry-run endpoints
- test-step endpoints

That makes it possible to catch structural problems before a workflow is allowed to mutate downstream systems.

This matters even more now that workflow execution can intersect with loaders, ontology-driven flows, lineage emission, and SoS validation steps.

## Scheduling And Approvals

The current workflow router includes:
- `POST /api/v1/workflows/:id/schedules`
- `GET /api/v1/workflows/:id/schedules`
- `GET /api/v1/workflows/:id/schedules/:schedule_id`
- `PUT /api/v1/workflows/:id/schedules/:schedule_id`
- `DELETE /api/v1/workflows/:id/schedules/:schedule_id`
- `POST /api/v1/schedule/preview`
- `GET /api/v1/approvals`
- `GET /api/v1/approvals/stats`
- `GET /api/v1/approvals/:request_id`
- `POST /api/v1/approvals/:request_id/approve`
- `POST /api/v1/approvals/:request_id/reject`
- `POST /api/v1/approvals/:request_id/cancel`

That means the public execution model is not just fire-and-forget. It also includes operator-facing control points for time-based automation and human review.

## Progress And Execution Control

Operators can inspect and control executions through endpoints such as:
- active execution listings
- execution progress views
- execution log retrieval
- stop, pause, resume, abort, and cancel controls

This is the surface you use when you need live operational awareness rather than just a success or failure bit.

## Relationship To Other Subsystems

Workflows are where multiple ARCXA areas converge:
- inputs come from datasources, files, or managed datasets
- mapping and ontology logic can feed or shape workflow behavior
- lineage is emitted as part of execution
- loaders can materialize outputs into downstream systems
- SoS validation can participate through dedicated workflow step support

## Current Boundaries And Caveats

A few practical realities are worth calling out:
- the router contains both modern workflow endpoints and legacy route-oriented workflow surfaces
- local development usually happens with auth disabled via `./run-local.sh`, but `/api/v1/...` should be assumed authenticated in secured environments
- the repository still contains historical `graphica` naming in some environment variables and helper code

## Related Guides

- [`data-sources-and-datasets.md`](data-sources-and-datasets.md)
- [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md)
- [`lineage-and-governance.md`](lineage-and-governance.md)
- [`systems-of-systems.md`](systems-of-systems.md)

