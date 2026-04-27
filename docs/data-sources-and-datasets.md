# Data Sources And Datasets

This guide covers the front half of the platform: getting data into ARCXA, understanding what a source can do, and promoting that source data into a managed surface that workflows and operators can actually use.

## Use This Guide When

Use this guide when you want to:
- understand the datasource lifecycle from registration through discovery and use
- choose between raw datasource operations and managed file or dataset flows
- understand what connector capability metadata actually means
- explain how stored credentials and managed datasets fit together

## Table Of Contents

1. [Supported Source Families](#supported-source-families)
2. [Connector Registry And Capability Reporting](#connector-registry-and-capability-reporting)
3. [Datasource Lifecycle](#datasource-lifecycle)
4. [Async Discovery And Schema Inference](#async-discovery-and-schema-inference)
5. [Credentials And Secrets](#credentials-and-secrets)
6. [File Library](#file-library)
7. [Datasets, Catalogue, And Entities](#datasets-catalogue-and-entities)
8. [Operational Caveats](#operational-caveats)

## Supported Source Families

The connector registry currently includes implementations for:
- PostgreSQL
- MySQL
- Snowflake
- Databricks
- S3 Parquet
- CSV files
- RDF N-Triples
- Oracle, DB2, and SAP HANA when the coordinator is built with the `odbc` feature and the required drivers are available

Important boundary:
- connector presence does not imply identical operational depth across all source types
- discovery, preview, query execution, and loader behavior vary by connector
- live connector capability responses should be treated as the precise contract for a given source type

## Connector Registry And Capability Reporting

The platform does not treat every source as a raw connection string. It keeps connector metadata, including:
- display name and source type
- version metadata
- required credential fields
- optional configuration fields
- capability flags
- enablement status

Relevant public routes:
- `GET /api/v1/connectors`
- `GET /api/v1/connectors/:id`
- `GET /api/v1/connectors/statistics`
- `POST /api/v1/connectors/:id/enable`
- `POST /api/v1/connectors/:id/disable`

In practice, that means the UI and API can reason about what a source is supposed to support before a workflow tries to use it.

## Datasource Lifecycle

A typical datasource lifecycle looks like this:

1. Register a datasource.
2. Test connectivity against the stored datasource definition.
3. Inspect connector metadata and capabilities.
4. Run schema inference or async discovery.
5. Feed the result into mapping, workflow, dataset-import, or catalogue flows.

Relevant coordinator routes include:
- `POST /api/v1/datasources`
- `GET /api/v1/datasources`
- `GET /api/v1/datasources/:id`
- `PUT /api/v1/datasources/:id`
- `DELETE /api/v1/datasources/:id`
- `POST /api/v1/datasources/test`
- `POST /api/v1/datasources/:id/query`
- `GET /api/v1/datasources/search`

## Async Discovery And Schema Inference

The datasource module supports more than a one-shot schema peek.

Routes currently include:
- `POST /api/v1/datasources/:id/schema/infer`
- `POST /api/v1/datasources/:id/schema/infer-enhanced`
- `POST /api/v1/datasources/:id/discover`
- `GET /api/v1/datasources/:id/discovery/progress`
- `GET /api/v1/datasources/:id/discovery/result`
- `DELETE /api/v1/datasources/:id/discovery`
- `GET /api/v1/datasources/:id/discovery/stream`

This matters operationally because discovery can be long-running. The current API gives you:
- an async start path
- polling for progress and results
- an SSE stream for progress
- a cancellation path

There is also a separate profiling surface under `/api/v1/profiling` for CSV and dataset profiling-oriented workflows.

## Credentials And Secrets

The coordinator supports stored credentials and secret-oriented workflows rather than forcing every downstream operation to pass credentials inline.

The current direction of travel is:
- keep actual credentials in the configured secret or storage layer
- return references and metadata through APIs
- have datasource interactions resolve the stored credentials when operations run

This matters operationally because connection tests, discovery, and workflow reads should behave against the same stored datasource definition, not against ad hoc one-off credentials.

## File Library

The file library is the managed entry point for file-backed workflows.

It is the right place to start when you need to:
- register uploaded or staged files
- organize them in folders
- tag and search them
- preview and scan them
- inspect file lineage, impact analysis, and usage stats
- feed files into dataset imports or workflows

Representative routes include:
- `GET /api/v1/file-library/files`
- `POST /api/v1/file-library/files`
- `GET /api/v1/file-library/files/:id/preview`
- `POST /api/v1/file-library/files/:id/scan`
- `POST /api/v1/file-library/files/bulk-upload`
- `GET /api/v1/file-library/folders`
- `POST /api/v1/file-library/search`
- `GET /api/v1/file-library/files/:id/lineage`

The file library sits alongside datasource registration rather than replacing it. Think of it as the managed ingress surface for file-oriented data.

## Datasets, Catalogue, And Entities

ARCXA also exposes a managed data surface above raw source connections.

That surface includes:
- dataset imports and import status tracking
- dataset listing and detail inspection
- entity browsing and related metadata access
- catalogue-style downstream views in the frontend

Relevant routes include:
- `GET /api/v1/datasets`
- `GET /api/v1/datasets/:id`
- `POST /api/v1/datasets/import`
- `POST /api/v1/datasets/import-datasource`
- `POST /api/v1/datasets/import-batch`
- `GET /api/v1/datasets/imports`
- `GET /api/v1/datasets/imports/:import_id`
- `GET /api/v1/entities`
- `GET /api/v1/entities/:id`
- `GET /api/v1/entities/:id/attributes`
- `GET /api/v1/entities/:id/lineage`

This is important because many users should not have to reason directly about the source system after onboarding. The managed dataset and entity layers give downstream consumers a more stable surface.

## Operational Caveats

A few current realities matter:
- ODBC-backed sources require local driver installation and an `ENABLE_ODBC=true` build when you want Oracle, DB2, or SAP HANA support.
- The live connector capability response is a better indicator than brand-name assumptions. Not every connector supports every path at the same depth.
- `./run-local.sh` disables auth by default, which makes local datasource exploration easy, but secure environments should be assumed to require auth for `/api/v1/...` routes.
- The repository still contains older `graphica` naming in environment variables and helper scripts.

## Related Guides

- [`glossary-and-concepts.md`](glossary-and-concepts.md)
- [`semantic-mapping-and-ontology.md`](semantic-mapping-and-ontology.md)
- [`workflows-and-execution.md`](workflows-and-execution.md)
- [`lineage-and-governance.md`](lineage-and-governance.md)

