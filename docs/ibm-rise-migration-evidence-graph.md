# IBM RISE Presentation Narrative

Use This Guide When:
- you need the IBM-facing story for ARCXA.
- you want a source-controlled presentation narrative before building slides.
- you want messaging that fits IBM Rapid Move, Kyano / CrystalBridge, smartShift, SAP ECC, SAP HANA, and S/4HANA without creating an avoidable feature war.

## Table Of Contents

1. [Audience](#audience)
2. [One-Line Positioning](#one-line-positioning)
3. [Messaging Rules](#messaging-rules)
4. [What IBM Gets Out Of It](#what-ibm-gets-out-of-it)
5. [Twelve-Slide Deck Structure](#twelve-slide-deck-structure)
6. [Suggested Demo Story](#suggested-demo-story)
7. [Architecture Talking Points](#architecture-talking-points)
8. [Current Product Proof Points](#current-product-proof-points)
9. [Expansion Story](#expansion-story)
10. [Related Guides](#related-guides)

## Audience

Primary audience:
- IBM practice leads
- IBM delivery leaders
- IBM migration architects
- IBM audit and sign-off stakeholders for RISE programs

Secondary audience:
- SAP transformation program sponsors
- customer data owners and PMO stakeholders
- migration QA and reconciliation teams

## One-Line Positioning

IBM's current RISE migration stack already has strong execution machinery.

ARCXA does not replace that stack.

ARCXA sits beside it as the migration evidence layer: it normalizes mappings, execution runs, exceptions, controls, approvals, and sign-off artifacts into a persistent evidence graph so IBM can answer “why does this migrated value look this way?” with traceable evidence instead of project archaeology.

## Messaging Rules

Keep these rules tight.

Say this:
- ARCXA normalizes migration artifacts into a persistent, queryable evidence graph.
- ARCXA is the transformation traceability layer.
- ARCXA helps IBM defend sign-off decisions and post-go-live answers.

Avoid saying this:
- CrystalBridge does not produce governance artifacts.
- IBM's stack lacks lineage.
- IBM needs ARCXA because its migration method is weak.

Prefer this framing instead:
- IBM, SNP, smartShift, and SAP tooling already produce important migration, validation, and execution artifacts.
- The opportunity is to normalize those artifacts into a persistent evidence model that is queryable by field, business object, transformation rule, exception, control, and sign-off event.
- The opportunity is richer than conventional lineage because it includes rule-level, exception-level, and approval-level explainability.

## What IBM Gets Out Of It

Lead with business value that fits IBM's incentives.

1. Faster sign-off.
   Customers do not spend weeks debating unexplained data deltas.

2. Lower post-go-live support burden.
   Teams can trace issues without reconstructing the story from spreadsheets, SAP tables, logs, Jira tickets, and consultant memory.

3. Stronger audit posture.
   Auditors receive evidence packets instead of tribal explanations.

4. More defensible RISE engagements.
   If a migration is challenged, IBM has a structured record of transformation decisions, controls, exceptions, approvals, and outcomes.

5. Reusable migration intelligence.
   Patterns from one engagement become reusable control models and evidence templates for the next one.

## Twelve-Slide Deck Structure

### 1. IBM's current stack is strong at execution

Message:
- IBM Rapid Move, Kyano / CrystalBridge, smartShift, and SAP-native tooling are already strong at moving, transforming, and delivering SAP landscapes.

Visual idea:
- ecosystem map with IBM, SNP, smartShift, SAP, and ARCXA shown as adjacent layers rather than competitors.

### 2. The remaining gap is evidence and explainability

Message:
- Execution artifacts exist, but teams still spend time reconstructing why a specific migrated value or object looks the way it does.

Visual idea:
- “before ARCXA” chain of spreadsheets, tickets, logs, SAP objects, and manual approvals.

### 3. ARCXA Migration Evidence Graph

Message:
- ARCXA is the persistent evidence and transformation traceability layer for migration programs.

Visual idea:
- source systems and migration tools feeding an evidence graph that supports sign-off, audit, and triage.

### 4. Core object model

Show:
- source field
- target field
- transformation rule
- execution event
- exception
- control
- approval
- evidence packet

### 5. Explain this value walkthrough

Message:
- one field or record can be explained end to end across mapping, execution, verification, exception, and sign-off artifacts.

### 6. Why this shortens sign-off

Message:
- disputes become evidence review workflows, not archaeology projects.

### 7. Why this reduces post-go-live support load

Message:
- teams can answer “what happened here?” quickly and repeatably.

### 8. Why this improves audit and engagement defensibility

Message:
- evidence packets and approval trails are easier to defend than ad hoc project memory.

### 9. How it fits beside IBM, SNP, smartShift, and SAP

Message:
- ARCXA complements those tools by normalizing and preserving their artifacts.

### 10. Service architecture and deployment posture

Message:
- start coordinator-led and single-node friendly, then scale into ingestion, traceability, and verification services.

### 11. Demo scenario

Message:
- show a disputed SAP sales-order amount and explain it across source field, target field, rule, run, control, exception, approval, and evidence packet.

### 12. Expansion path

Message:
- move from one explained value to reusable control packs, evidence templates, and program-level dashboards.

## Suggested Demo Story

A strong opening demo story is:
- an ECC sales-order amount lands in S/4HANA
- a business owner disputes the migrated value
- IBM opens ARCXA and queries the target field or record
- ARCXA returns the source field, transformation rule, migration run, verification control, accepted exception, approver, and signed evidence packet
- the discussion changes from “who thinks this is right?” to “what does the evidence say?”

This is more compelling than a generic lineage demo because it directly maps to sign-off friction.

## Architecture Talking Points

Keep the architecture story disciplined.

Current foundation already in the repository:
- shared migration-evidence contracts in `arcxa-core`
- coordinator REST gateway under `/api/v1/migration-evidence`
- standalone service crates for evidence ingestion, traceability, and verification
- signed evidence packets
- read-only verification support including SAP HANA SQL reads
- verification can emit canonical execution, control, and exception evidence onto the same direct/Kafka backbone used by ingestion

Explain the rollout path as phased:
- phase 1: coordinator-led gateway with single-node friendly deployment
- phase 2: pull ingestion, traceability, and verification out as separately deployed services while keeping the coordinator as the public gateway
- phase 3: make Kafka-backed delivery the normal service-to-service backbone and expand reusable controls and program-level dashboards

## Current Product Proof Points

As of the current codebase, the concrete proof points are:
- connectors can be registered and run through the coordinator API
- manual canonical migration events can be ingested today
- verification runs can emit control results and exception evidence
- value explanations can assemble source field, target field, rule, execution context, exceptions, controls, approvals, and evidence packet references
- evidence packets are signed and verifiable

That is enough to support a credible “first proof point” narrative today.

## Expansion Story

After the first wedge proves itself, the next narrative is:
- approval review queues
- object-level audit packs
- reusable migration control libraries
- engagement-to-engagement evidence templates
- program-level dashboards for exception hotspots, reconciliation drift, and approval bottlenecks

That turns the wedge from a point feature into a repeatable migration intelligence layer.

## Related Guides

- [`migration-evidence-graph.md`](migration-evidence-graph.md)
- [`architecture.md`](architecture.md)
- [`api-surface.md`](api-surface.md)
- [`platform-capabilities.md`](platform-capabilities.md)
