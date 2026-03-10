# Graphica

So this is a distributed data governance platform that uses RDF triples as storage. When you're tracking lineage through ML models and need to query "show me everything this model touched", having a graph database that speaks SPARQL actually makes sense. It's not about being academic, it's about not having to write 50 different JOIN queries.

The architecture: coordinator processes route queries to N shard processes. Each shard owns a chunk of the hash space and stores RDF triples in Oxigraph (which uses RocksDB). The coordinator maintains a shard registry (also in RocksDB) that tracks which shards exist and auto-assigns hash ranges when they register. Routing is deterministic consistent hashing with 150 virtual nodes per shard for even distribution.

You can run a single coordinator (good enough for most use cases), or run 3+ coordinators with Raft consensus for automatic failover (sub-second). Shards don't vote on anything - they just store triples and answer queries. If you add/remove shards, the system rebalances data automatically when imbalance exceeds 20%.

Kafka integration includes durable writes with write-ahead log (WAL) guarantees. Event writes go through a WAL before hitting Kafka, so if the broker is down you don't lose data. There's a hybrid mode with progressive feature flags (percentage-based rollout, tenant targeting, hash-based bucketing) for migrating from legacy fire-and-forget to durable mode without downtime. Distributed replay coordination uses Raft for leader election across coordinators - only the leader replays Kafka events to prevent duplicate processing during failover.

All write operations go through an optional cryptographic audit chain (Ed25519 signatures, SHA-256 hash chain, Merkle tree for batch verification). This is for compliance with SOX/HIPAA/GDPR requirements. It's feature-gated so you can disable it.

CSV-to-DB bulk loading with PostgreSQL COPY support (50-200K rows/sec), integrated transformation engine for SQL-like field transformations (UPPER, TRIM, COALESCE, etc.), and unified mapping sessions that consolidate multiple CSV sources into target database tables.

Declarative workflow definitions in YAML/JSON with GitOps support. Workflows persist to RocksDB with automatic checkpointing (configurable interval). Recovery from crashes resumes workflows from last checkpoint instead of restarting. File usage tracking maintains referential integrity between workflows and registered files in a separate RocksDB store. Batch execution with DAG-based dependency resolution, transaction coordination, and dead letter queues for failed operations. 

## Why Multi-Process?

Because RocksDB version conflicts are a pain. The coordinator needs one version (librocksdb-sys), Oxigraph needs another (oxrocksdb-sys v6.x). Instead of fighting the Rust type system and cargo resolver, we just run separate binaries. Bonus: you can scale shards independently and actually kill them without taking down the API.

## Architecture

```
                ┌──────────────────┐
                │   Coordinator    │  <- REST/gRPC API, routes queries
                │   (gRPC Server)  │     Stores indexes in RocksDB
                │                  │     Has no actual RDF data
                │  Auto-assigns    │     [Optional: Audit Chain]
                │  shard IDs       │
                │  dynamically     │
                └──────────────────┘
                         ▲
        ┌────Register────┤───────Register────┐
        │  (Get ID: 0)   │    (Get ID: 1)    │
        │                │                    │
   ┌────▼───┐      ┌────▼───┐          ┌────▼───┐
   │ Shard 0│      │ Shard 1│          │ Shard 2│
   │ 0-33%  │      │ 33-67% │          │ 67-100%│
   │        │      │        │          │        │
   │Machine │      │Machine │          │Machine │
   │ID: abc │      │ID: def │          │ID: ghi │
   └────────┘      └────────┘          └────────┘
       │                │                    │
       └────Heartbeat───┴────Heartbeat──────┘
            every 30s         every 30s
```

**Auto-Registration Flow:** Shards connect to coordinator on startup, send their machine ID (UUID), and get assigned sequential shard IDs (0, 1, 2...). Coordinator automatically distributes hash ranges evenly. Shards persist their assigned ID to disk (`.graphica/shard_identity.json`) so they reconnect with the same identity after restarts. No manual configuration needed.

The coordinator doesn't store RDF data. It just routes based on hash(subject URI) and merges results. Shards are dumb - they don't talk to each other, they just store triples and answer SPARQL queries.

## Field Mapping Engine (Because Schema != Ontology)

When you import data from external sources (Postgres, CSV, Snowflake, RDF/N-Triples, etc.), the field names are things like `cust_email` and `customer_phone_nbr`. But your ontology uses URIs like `http://schema.org/email`. The field mapping engine figures out which fields map to which ontology terms, lets users review the mappings, and then imports the data with proper semantic alignment.

**Why this exists:** Without it, you'd have to manually write mapping configs for every data source. Or worse, your entities would have `cust_email` as a predicate instead of `<http://schema.org/email>`, which defeats the point of using RDF in the first place.

### Custom Ontologies

The platform supports custom domain ontologies in both **Turtle** and **RDF/XML** formats. Upload via `POST /api/v1/ontology` and the system auto-detects the format and parses classes, properties, hierarchies, and data type constraints. The ontology registry feeds the field mapping engine, so when you analyze a data source, candidates are matched against your custom terms (not just schema.org defaults).

You can also import existing RDF data directly using the **RDF/N-Triples datasource connector**. This lets you register N-Triples files (local or URL) as data sources and import their triples into the governance store, treating RDF data the same way as relational databases or CSV files.

### How It Works

The mapping engine uses a hybrid AI approach:

1. **Statistical Matcher** (Phase 1): TF-IDF + N-grams for lexical similarity. Catches things like "customer_email" → `schema:email` based on token overlap.

2. **Semantic Matcher** (Phase 2): Transformer embeddings via gRPC to a model service. Understands that "phone_number" and "telephone" mean the same thing even though they share zero tokens.

3. **GNN Matcher** (Phase 3, not yet implemented): Graph neural networks using schema structure. Would understand that "user_id" in an "orders" table probably maps to different ontology terms than "user_id" in a "logins" table.

4. **Symbolic Matcher** (Phase 4, not yet implemented): SPARQL reasoning using ontology axioms. Would enforce constraints like "email addresses must be unique identifiers."

Currently Phase 1+2 are live. Scores are blended (60% statistical, 40% semantic) to get final confidence.

### The Workflow

```bash
# 1. Analyze a data source - figures out which fields map to which ontology terms
POST /api/v1/datasources/{source_id}/analyze-for-mapping
{
  "tables": ["customers", "orders"],
  "user_id": "admin",
  "auto_approve_threshold": 0.95  # Mappings above this are auto-approved
}

# Returns a session with field mappings and confidence scores
# High-confidence mappings (>= 0.95) are auto-approved
# Lower confidence ones need human review

# 2. Review pending mappings (optional, skip if all auto-approved)
POST /api/v1/mapping/sessions/{session_id}/review
{
  "field_mappings": [
    {
      "field_id": "pg_source_customers_phone",
      "action": "approve"  # or "reject" or "modify"
    }
  ],
  "reviewed_by": "analyst_1",
  "finalize": true
}

# 3. Apply approved mappings - stores them as RDF triples in the governance brain
POST /api/v1/mapping/sessions/{session_id}/apply
{}

# This creates gph:MappingSession and gph:FieldMapping triples
# with full provenance (who approved, when, confidence scores, etc.)

# 4. Import data using the mappings - generates entity triples with ontology alignment
POST /api/v1/mapping/sessions/{session_id}/import
{
  "batch_size": 1000,
  "user_id": "admin"
}

# Result: Entities stored as RDF with proper ontology term predicates
# Instead of:  <entity> <cust_email> "john@example.com"
# You get:     <entity> <http://schema.org/email> "john@example.com"
```

The whole session (mappings + approvals) is stored in RocksDB on the coordinator. The final entity data goes to the shards as RDF triples, just like everything else.

### What Gets Stored

**Mapping metadata** (in a dedicated named graph):
```turtle
<gph:mapping/session/abc123> a gph:MappingSession ;
    gph:forDataSource "pg_source" ;
    gph:hasMapping <gph:mapping/field/pg_source_customers_email> .

<gph:mapping/field/pg_source_customers_email> a gph:FieldMapping ;
    gph:sourceTable "customers" ;
    gph:sourceField "customer_email" ;
    gph:mapsToOntologyTerm <http://schema.org/email> ;
    gph:confidence "0.95"^^xsd:double ;
    gph:approvalStatus "autoapproved" .
```

**Entity data** (in a separate graph per data source):
```turtle
<gph:entity/pg_source_customers_0> a gph:Entity ;
    gph:entityId "pg_source_customers_0" ;
    gph:entityType "customers" ;
    prov:wasDerivedFrom <gph:mapping/session/abc123> ;  # Provenance!
    <http://schema.org/email> "alice@example.com" ;     # Ontology term as predicate
    <http://schema.org/name> "Alice Smith" ;
    <http://schema.org/identifier> "1001" .
```

The provenance link means you can query "which mapping session created this entity" or "show me all entities from mapping session X". It's audit-trail-complete.

### Query Endpoints

**Read-only** (all authenticated users):
- `GET /api/v1/mapping/health` - is semantic matcher available?
- `GET /api/v1/mapping/sessions/{session_id}` - get session details
- `GET /api/v1/mapping/fields/{field_id}/candidates` - see alternative mappings

**Write** (Admin, Operator, Service roles):
- `POST /api/v1/datasources/{source_id}/analyze-for-mapping` - start mapping
- `POST /api/v1/mapping/sessions/{session_id}/review` - approve/reject
- `POST /api/v1/mapping/sessions/{session_id}/apply` - store to RDF
- `POST /api/v1/mapping/sessions/{session_id}/import` - import entity data

### Performance

On a laptop (demo data, 3 customers):
- Schema analysis: ~50ms for 3 fields (includes semantic matching via gRPC)
- Candidate generation: ~10ms per field (statistical + semantic blend)
- RDF storage (mappings): ~20ms for 45 triples (1 session + 3 field mappings)
- Data import: ~50ms for 21 triples (3 entities × 7 triples each)

With actual data sources, the bottleneck is querying the source DB for schema + samples, not the matching or RDF storage.

### The Semantic Matcher (Phase 2)

This is a separate gRPC service (`graphica-model-service`) that runs transformer models for embeddings. The coordinator calls it to compute semantic similarity between field names and ontology term labels.

**Start the model service:**
```bash
# Needs the MiniLM model files (170MB)
cd graphica-model-service
export GRAPHICA_MODEL_PATH=../models/minilm
cargo run --release
```

It listens on `localhost:50051` by default. The coordinator will use it if it's available, otherwise falls back to statistical matching only.

The model service loads Sentence-BERT (all-MiniLM-L6-v2) and computes cosine similarity between embeddings. It's fast (~5ms per comparison) and caches results in RocksDB, so repeated queries are instant.

## CSV-to-DB Bulk Loading (Because ETL Shouldn't Be Slow)

When you have multiple CSV files from different sources that need to land in the same database tables, you want:
1. **Fast loading** - not row-by-row INSERTs
2. **Field transformations** - UPPER(TRIM(email)), COALESCE(phone1, phone2), etc.
3. **Multi-source consolidation** - merge data from 3 different CSVs into one table
4. **Lineage tracking** - know which CSV row became which DB row

The unified mapping engine does all of this. You create a "unified session" that defines how multiple CSV sources map to target database tables, then load everything in one shot using PostgreSQL COPY (50-200K rows/sec) instead of INSERT (5-10K rows/sec).

### How It Works

**1. Create a unified session** - define target database schema and map CSV fields to columns:

```bash
POST /api/v1/unified_mapping
{
  "source_session_ids": ["csv_001", "csv_002"],  # Multiple CSVs
  "target_database": {
    "name": "analytics",
    "tables": {
      "customers": {
        "columns": {
          "email": { "data_type": "VARCHAR(255)", "nullable": false },
          "phone": { "data_type": "VARCHAR(20)", "nullable": true },
          "created_at": { "data_type": "TIMESTAMP", "nullable": false }
        },
        "primary_keys": ["email"]
      }
    }
  },
  "created_by": "admin"
}

# Returns:
{
  "session_id": "unified_abc123",
  "field_mappings": [...],   # Auto-generated mappings
  "conflicts": [...]         # Fields that map to same column
}
```

The system auto-detects conflicts (e.g., "email1" from CSV A and "email_address" from CSV B both map to `customers.email`) and lets you resolve them with strategies like `Coalesce`, `UsePrimary`, or `Merge`.

**2. Resolve conflicts** (if any):

```bash
POST /api/v1/unified_mapping/unified_abc123/resolve
{
  "resolutions": {
    "conflict_001": {
      "strategy": "Coalesce"  # Use first non-empty value
    }
  }
}
```

**3. Load to database** - kicks off background job:

```bash
POST /api/v1/unified_mapping/unified_abc123/load
{
  "database_type": "PostgreSQL",
  "connection_config": {
    "host": "localhost",
    "port": 5432,
    "database": "analytics",
    "username": "loader",
    "password": "...",
    "ssl_mode": "require"
  },
  "batch_size": 10000,
  "create_tables": true
}

# Returns:
{
  "load_job_id": "loadjob_xyz789",
  "status": "Queued",
  "message": "Load job queued successfully"
}
```

The loader:
- Creates tables with proper DDL if `create_tables: true`
- Reads CSV data from all source sessions
- Applies field transformations (see below)
- Loads using PostgreSQL COPY FROM STDIN (10-100x faster than INSERT)
- Tracks lineage as RDF triples

**4. Check job status**:

```bash
GET /api/v1/unified_mapping/load/{load_job_id}/status

# Returns:
{
  "job_id": "loadjob_xyz789",
  "status": "Running",
  "progress": {
    "total_rows": 100000,
    "rows_processed": 50000,
    "rows_succeeded": 49950,
    "rows_failed": 50,
    "percentage_complete": 50.0
  }
}
```

### Field Transformations

The transformation engine supports SQL-like expressions for cleaning and transforming data during load:

**String functions:**
- `UPPER({email})`, `LOWER({name})`
- `TRIM({field})`, `LTRIM({field})`, `RTRIM({field})`
- `CONCAT({first_name}, ' ', {last_name})`
- `SUBSTRING({phone}, 1, 3)` - extract area code
- `REPLACE({ssn}, '-', '')` - remove dashes

**Null handling:**
- `COALESCE({email1}, {email2}, {email3})` - first non-null/non-empty
- `IFNULL({middle_name}, '')` - default value
- `NULLIF({status}, 'N/A')` - convert to NULL

**Numeric:**
- `ABS({balance})`, `ROUND({amount}, 2)`
- `FLOOR({price})`, `CEIL({quantity})`

**Date functions:**
- `CURRENT_DATE()` - today
- `DATE_ADD({hire_date}, 90)` - add days
- `DATE_FORMAT({timestamp}, 'YYYY-MM-DD')`

**Conditional:**
- `IF({age} > 18, 'adult', 'minor')`

**Regex:**
- `REGEX_MATCH({email}, '^[a-z]+@.*')` - validation
- `REGEX_REPLACE({phone}, '[^0-9]', '')` - extract digits

Example transformation in unified mapping:

```json
{
  "transformation": "UPPER(TRIM({email}))",
  "target_column": "email"
}
```

This is compiled into an execution plan and cached, so repeated transformations are fast. The engine handles ~100K rows/sec for typical transformations.

### Performance

On real data (tested locally):
- **INSERT mode**: 5-10K rows/sec (multi-row parameterized INSERT)
- **COPY mode**: 50-200K rows/sec (PostgreSQL COPY FROM STDIN)
- **Transformation overhead**: ~5-10% (most time is I/O)
- **Memory usage**: Streaming CSV reader, minimal footprint

COPY mode is the default. It generates properly-escaped CSV data in memory and streams it to PostgreSQL using the binary protocol. Special characters, quotes, nulls all handled correctly.

### What Gets Tracked

The load operation creates lineage RDF triples:

```turtle
<gph:load/loadjob_xyz789> a prov:Activity ;
    prov:used <gph:session/unified_abc123> ;
    prov:startedAtTime "2025-10-17T10:00:00Z"^^xsd:dateTime ;
    prov:endedAtTime "2025-10-17T10:02:15Z"^^xsd:dateTime .

<gph:entity/customers/row_0001> a gph:Entity ;
    prov:wasGeneratedBy <gph:load/loadjob_xyz789> ;
    gph:sourceRow "csv_001:42" ;
    gph:targetTable "customers" .
```

You can query "which load job created this row" or "show me all rows from load job X" using SPARQL.

### API Endpoints

**Unified Mapping Management:**
- `POST /api/v1/unified_mapping` - create unified session
- `GET /api/v1/unified_mapping/{id}` - get session details
- `GET /api/v1/unified_mapping` - list all sessions
- `PUT /api/v1/unified_mapping/{id}` - update mappings
- `DELETE /api/v1/unified_mapping/{id}` - delete session

**Conflict Resolution:**
- `POST /api/v1/unified_mapping/{id}/resolve` - resolve field conflicts

**Database Loading:**
- `POST /api/v1/unified_mapping/{id}/load` - start load job
- `GET /api/v1/unified_mapping/load/{job_id}/status` - check progress

### Supported Databases

Currently implemented:
- **PostgreSQL** - COPY FROM STDIN (fast) + multi-row INSERT (fallback)

Coming soon (placeholders exist):
- **DB2** - LOAD utility + DEL file generation
- **Oracle** - SQL*Loader

### Error Handling

Failed rows go to a dead letter queue (DLQ) with:
- Original CSV row data
- Error message (constraint violation, type mismatch, etc.)
- Timestamp and job ID

DLQ files are CSV format in `./data/dlq/{job_id}/`, so you can review and reprocess them.

### Why This Exists

Because manually writing INSERT statements for 100K rows is insane, and most ETL tools are either slow (row-by-row) or complicated (Airflow DAGs with 47 tasks). This gives you:
- High-performance bulk loading (COPY protocol)
- Transformation engine integrated (no external tools)
- Multi-source consolidation (merge CSVs before load)
- Lineage tracking (W3C PROV triples)

And it's just HTTP endpoints - no DAG definitions, no YAML configs, no separate transformation tools.

## CSV-to-DB2 ETL Pipeline

High-performance CSV ingestion for DB2 with MERGE/UPSERT support. Uses r2d2 connection pooling, async CSV reading, and multi-row batch execution. Generic connection layer (`DB2Connection` trait) so tests use `MockDB2Connection` (no ODBC), production uses real connections.

```
CSV → AsyncCsvReader → LoaderWorker → DB2Loader → DB2ConnectionManager → DB2 (ODBC)
```

Default batch size: 5,000 rows. Connection pool: 10 connections.

### INSERT vs MERGE

**INSERT:** 10-50K rows/sec. Append-only, fails on duplicate PK. Use for initial loads.

**MERGE:** 5-20K rows/sec. Idempotent upsert by primary key. Use for CDC/incremental.

```sql
-- MERGE example (composite PK supported)
MERGE INTO products AS T
USING (VALUES (...)) AS S (product_id, variant_id, ...)
ON T.product_id = S.product_id AND T.variant_id = S.variant_id
WHEN MATCHED THEN UPDATE SET ...
WHEN NOT MATCHED THEN INSERT ...
```

Primary keys from `TargetTableConfig`. UPDATE clause excludes PKs automatically.

### Error Handling

**DLQ:** Failed rows → `./data/dlq/{job_id}/batch_{n}.csv`

**Checkpointing:** Resume from crash. Checkpoint every N batches (default: 10). JSON format: `./data/checkpoints/{job_id}.json`

**Retries:** 3 attempts with exponential backoff (100ms, 200ms, 400ms). Transient errors (deadlock, timeout) retry. Constraint violations → DLQ.

### Usage

```bash
POST /api/v1/loader/jobs
{
  "source_file": "/data/customers.csv",
  "target_table": "CUSTOMERS",
  "dml_mode": "Merge",
  "batch_size": 5000,
  "table_config": {
    "name": "CUSTOMERS",
    "primary_keys": ["CUSTOMER_ID"],
    "columns": {...}
  },
  "db2_config": {
    "host": "db2.example.com",
    "port": 50000,
    "database": "ANALYTICS",
    "username": "etl_user",
    "password": "$DB2_PASSWORD",
    "max_connections": 10
  }
}
```

**API:** `GET /api/v1/loader/jobs/{job_id}` for status, `DELETE` to cancel.

**Pending:** Parameter binding, CSV header parsing, DEL transformation integration.

## Batch Job Orchestration (Because Bulk Imports Need Coordination)

When you have dozens of CSV files that need to import to DB2 with dependencies (customers before orders, products before inventory), you want:
1. **Parallel execution** - multiple files loading simultaneously
2. **Dependency management** - orders wait for customers to complete
3. **Transaction coordination** - all-or-nothing, per-file, or batched
4. **Progress tracking** - real-time status via Server-Sent Events
5. **Retry logic** - automatic retries with exponential backoff
6. **Dead letter queue** - failed rows captured for review

The batch job system orchestrates workflow executions as a DAG with transaction boundaries, resource limits, and comprehensive monitoring.

### Quick Example

```bash
# Create batch job with dependencies
curl -X POST http://localhost:8080/api/v1/batch \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Customer Data Import",
    "workflow_id": "csv_to_db2_workflow",
    "created_by": "admin",
    "config": {
      "max_parallel": 4,
      "transaction_mode": "PerFile",
      "retry_failed": true,
      "max_retries": 3,
      "enable_dlq": true
    },
    "files": [
      {"file_id": "file_001", "file_name": "customers.csv"},
      {"file_id": "file_002", "file_name": "orders.csv", "dependencies": ["file_001"]}
    ]
  }'

# Start execution
curl -X POST http://localhost:8080/api/v1/batch/{job_id}/start

# Monitor progress (Server-Sent Events)
curl -N -H "Accept: text/event-stream" \
  http://localhost:8080/api/v1/batch/{job_id}/progress
```

### Transaction Modes

**PerFile** (default): Each file in separate transaction. Partial success OK.

**AllOrNothing**: All files in single transaction. Any failure rolls back everything.

**Batched**: Files grouped into transaction batches. Balance between isolation and performance.

```json
{
  "transaction_mode": {
    "Batched": {
      "batch_size": 5
    }
  }
}
```

### Features

- **Dependency Resolution**: DAG-based execution with automatic ordering
- **Preflight Validation**: Detects circular dependencies, invalid configs before execution
- **Resource Limits**: Memory, DB connections, file size constraints
- **SSE Progress Streaming**: Real-time updates with `event: progress`, `event: execution_completed`
- **DLQ Integration**: Failed rows captured to CSV for reprocessing
- **Transaction Summary**: Track commits/rollbacks across entire batch

### API Endpoints

- `POST /api/v1/batch` - create batch job
- `POST /api/v1/batch/{id}/start` - start execution
- `GET /api/v1/batch/{id}` - status and progress
- `GET /api/v1/batch/{id}/progress` - SSE stream
- `POST /api/v1/batch/{id}/cancel` - cancel running job
- `GET /api/v1/batch/{id}/transactions` - transaction summary
- `GET /api/v1/batch?status=Running` - list/filter jobs
- `DELETE /api/v1/batch/{id}` - delete job

**Full Documentation:** `docs/BATCH_JOBS.md`

**Example Scripts:**
- Bash: `docs/examples/batch_job_quickstart.sh`
- Python: `docs/examples/batch_job_example.py`

**Integration Tests:** 20 comprehensive tests covering dependencies, transactions, validation, DLQ, progress tracking.

## SHACL-to-DDL Generation (Because Schema Validation Matters)

When you have SHACL shapes defining your data constraints in RDF, you probably want equivalent SQL tables with CHECK constraints, FOREIGN KEYs, and proper types. The SHACL parser (`graphica-coordinator/src/mapping/ddl/shacl/parser.rs`) extracts shapes from the RDF store and generates DDL for PostgreSQL, DB2, MySQL, Oracle, and SQL Server.

### What It Does

Takes SHACL shapes like this:

```turtle
ex:CustomerShape a sh:NodeShape ;
    sh:targetClass ex:Customer ;
    sh:property [
        sh:path ex:email ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxLength 255 ;
        sh:pattern "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$" ;
    ] ;
    sh:property [
        sh:path ex:age ;
        sh:minExclusive 0 ;
        sh:maxInclusive 150 ;
    ] ;
    sh:property [
        sh:path ex:status ;
        sh:in ("active" "inactive" "suspended") ;
        sh:defaultValue "active" ;
    ] .
```

And generates SQL DDL like this:

```sql
CREATE TYPE customer_status AS ENUM ('active', 'inactive', 'suspended');

CREATE TABLE CUSTOMER (
    email VARCHAR(255) NOT NULL,
    age INTEGER,
    status customer_status DEFAULT 'active',

    CHECK (LENGTH(email) > 0),
    CHECK (email ~ '^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'),
    CHECK (age > 0),
    CHECK (age <= 150)
);
```

### Supported SHACL Constraints

The parser handles most of the useful SHACL Core Constraint Components:

**Value Constraints:**
- `sh:in` → ENUM or CHECK (column IN (...))
- `sh:hasValue` → DEFAULT or CHECK (column = value)
- `sh:datatype` → SQL type (VARCHAR, INTEGER, etc.)

**Numeric Constraints:**
- `sh:minInclusive`, `sh:maxInclusive` → CHECK (column >= min AND column <= max)
- `sh:minExclusive`, `sh:maxExclusive` → CHECK (column > min AND column < max)

**String Constraints:**
- `sh:minLength`, `sh:maxLength` → VARCHAR(n) + CHECK (LENGTH(...) >= min)
- `sh:pattern` → CHECK (column REGEXP pattern)
- `sh:flags` → Regex flags (case-insensitive, etc.)

**Property Comparison Constraints:**
- `sh:lessThan`, `sh:lessThanOrEquals` → CHECK (start_date < end_date)
- `sh:equals` → CHECK (email = confirm_email)
- `sh:disjoint` → CHECK (work_email != personal_email)

**Cardinality:**
- `sh:minCount` ≥ 1 → NOT NULL
- `sh:maxCount` = 1 + `sh:minCount` ≥ 1 → UNIQUE

**Relationships:**
- `sh:class` → FOREIGN KEY to referenced table

### Validation and Error Handling

The parser includes constraint validation that warns about:
- Conflicting bounds (both `minInclusive` and `minExclusive`)
- Invalid ranges (min >= max)
- Type mismatches (string constraints on integer fields)
- Redundant constraints (sh:hasValue with sh:in)

Errors include full SPARQL query context and shape URIs, so you can actually debug what went wrong.

### Usage

```bash
# Parse SHACL shape and generate DDL
GET /api/v1/ddl/shacl/{shape_uri}

# Or generate for specific dialect
POST /api/v1/ddl/shacl/{shape_uri}/generate
{
  "dialect": "postgresql",
  "include_comments": true
}
```

**Test Coverage:** 22 tests (all passing)
**Full Documentation:** `docs/SHACL_FEATURE_COVERAGE.md`

### Why This Exists

Because maintaining SHACL shapes and SQL DDL manually is a pain. With this, your shapes *are* your schema. Change the SHACL, regenerate the DDL, run migrations. The RDF store and SQL database stay in sync.

It's especially useful for:
- **Data quality**: SHACL constraints translate directly to SQL CHECK constraints
- **Schema evolution**: Version your shapes, track changes as RDF triples
- **Multi-dialect support**: One SHACL shape → DDL for all supported databases

## Building This Thing

It's a Cargo workspace with three crates. Build order matters because graphica-core is a library:

```bash
cargo build --release
# Or if cargo is being stupid:
cd graphica-core && cargo build --release --lib && cd ..
cd graphica-shard && cargo build --release && cd ..
cd graphica-coordinator && cargo build --release && cd ..
```

This spits out:
- `graphica-shard/target/release/graphica-shard` - the actual RDF storage
- `graphica-coordinator/target/release/graphica-coordinator` - HTTP server + query router

## Running Locally

**Single Coordinator (simple):**

Shards now auto-register with the coordinator via gRPC - no manual shard IDs needed!

```bash
# Start coordinator FIRST (shards need to register with it)
RUST_LOG=info \
GRPC_PORT=50051 \
ENABLE_CRYPTOGRAPHIC_AUDIT=true \
./graphica-coordinator/target/release/graphica-coordinator &

# Wait 2 seconds for coordinator to start
sleep 2

# Shard 0 - auto-registers and receives shard_id=0
RUST_LOG=info ./graphica-shard/target/release/graphica-shard \
  --data-path ./data/shard-0 \
  --port 9100 \
  --coordinator-url http://localhost:50051 \
  --heartbeat-interval 30 &

# Shard 1 - auto-registers and receives shard_id=1
RUST_LOG=info ./graphica-shard/target/release/graphica-shard \
  --data-path ./data/shard-1 \
  --port 9101 \
  --coordinator-url http://localhost:50051 \
  --heartbeat-interval 30 &
```

Or just run `./run-local.sh` if you don't want to type all that. Set `ENABLE_CRYPTOGRAPHIC_AUDIT=false` if you're not doing compliance stuff.

**How Auto-Registration Works:**

1. Each shard generates a unique machine ID (UUID) on first startup
2. Machine ID is stored in `{data_path}/.graphica/shard_identity.json`
3. Shard connects to coordinator via gRPC and requests registration
4. Coordinator assigns sequential shard IDs (0, 1, 2, ...) and hash ranges (evenly distributed)
5. Shard saves assigned ID and hash range to identity file
6. On restart, shards reconnect with the same shard ID (persistent identity)
7. Heartbeats every 30 seconds report statistics and health status

**Benefits:**
- No manual shard ID configuration
- Shards survive restarts with same identity
- Dynamic cluster expansion (just start new shards)
- Automatic hash range redistribution

**HA Cluster (3 coordinators with Raft consensus):**

Build with HA support, then run the HA script:

```bash
# Build with Raft consensus
ENABLE_HA=true ./build.sh

# Start infrastructure (Kafka, etc.)
docker compose up -d zookeeper kafka schema-registry

# Run 3 coordinators + 3 shards
./run-local-ha.sh
```

This starts 3 coordinators (ports 8081-8083) with automatic leader election and sub-second failover. Check `HA_LOCAL_TESTING_GUIDE.md` for testing procedures.

## How Data Flows

**Writes:**
1. You POST some JSON to `/api/v1/models` or whatever
2. Coordinator converts it to RDF triples (W3C PROV ontology)
3. For each triple, hash(subject URI) determines which shard gets it
4. Coordinator calls `insert_triple` on target shards via gRPC
5. If audit is enabled, operation gets logged (Ed25519 signature, SHA-256 hash chain)
6. Done

The audit step adds ~1-2ms but it's async so it doesn't block the response. Signatures and hashes go into a separate RocksDB instance, Merkle tree gets updated incrementally for batch verification later.

**Reads (SPARQL):**
1. You POST a SPARQL query
2. Coordinator sends it to ALL active shards in parallel
3. Each shard runs the query on its partition
4. Coordinator merges results and deduplicates (using HashSet on serialized bindings)
5. Returns the combined result set

This is why SPARQL actually works here - the coordinator doesn't need to understand your query semantics. It just scatters to all shards and gathers results. The shards do the real work.

Performance is pretty good: ~1.3ms for simple queries hitting 2 shards locally. Obviously depends on query complexity and network latency in production.

## Cryptographic Audit (If You Care About Compliance)

When enabled, every write operation (insert, update, delete) gets logged to a tamper-proof audit chain. Each entry includes:
- Ed25519 signature (cryptographically signed, can't be forged)
- SHA-256 hash of previous entry (can't reorder or delete entries)
- Transaction ID, timestamp, user ID, operation type
- Merkle tree for O(log n) batch verification

The chain is append-only and stored in RocksDB, so it survives crashes. You can verify the entire chain integrity, generate cryptographic proofs for specific operations, or export everything for compliance audits.

Typical use case: auditor asks "show me proof that user X modified entity Y on date Z". You generate a Merkle proof (few KB), they verify it against the current Merkle root. Done. No need to hand over your entire database.

Performance hit is minimal (~1-2ms per write) because signing and hashing happen async. The coordinator doesn't wait for the audit write to complete before responding to your request.

Disable it with `ENABLE_CRYPTOGRAPHIC_AUDIT=false` if you don't need it. The feature is compiled out entirely when building without `--features cryptographic-audit`, so zero overhead.

## Configuration

The coordinator takes either env vars or CLI args (env vars win if both):

```bash
REST_PORT=8080                      # HTTP API
GRPC_PORT=9090                      # gRPC for shard auto-registration
ROCKSDB_PATH=./data/...             # Coordinator's metadata + shard registry
JWT_SECRET=<base64>                 # For auth tokens (32+ bytes)

# Shard auto-registration (no manual SHARD_URLS needed!)
# Shards connect to GRPC_PORT and register themselves

# Cryptographic Audit (optional, for compliance)
ENABLE_CRYPTOGRAPHIC_AUDIT=true     # Enable audit chain
AUDIT_CHAIN_PATH=./data/audit       # Where to store audit RocksDB
AUDIT_USER_ID=coordinator           # User ID for audit entries
```

Shards use auto-registration:
```bash
--coordinator-url http://localhost:50051  # Coordinator gRPC endpoint
--data-path ./data/shard-0                # Where to put RocksDB files + identity
--port 9100                               # gRPC port
--heartbeat-interval 30                   # Heartbeat frequency in seconds
# Shard ID and hash ranges assigned automatically on first connection
# Identity persisted to {data-path}/.graphica/shard_identity.json
```

## Authentication (Because Production)

On first startup, coordinator generates a one-time setup token (valid 1 hour). This is printed to stdout. You can't create admin accounts without it, which prevents random people from hitting `/auth/setup` and making themselves admin.

```bash
# Look for this in the logs:
grep "Setup token" ./data/coordinator/coordinator.log

# Then create admin user (you get ONE shot at this):
curl -X POST http://localhost:8080/auth/setup \
  -H "Content-Type: application/json" \
  -d '{"setup_token": "<token-from-logs>", "password": "SomethingSecure123!"}'
```

Password needs 12+ chars with upper, lower, digit, special char. Yes, it's annoying. No, I don't care.

After that, normal login:
```bash
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "SomethingSecure123!"}'
```

Returns a JWT. Slap it on requests as `Authorization: Bearer <token>`.

For dev/testing, set `ENABLE_AUTH=false` and the middleware just injects fake admin claims. DO NOT DO THIS IN PRODUCTION.

## Why RDF Though?

Fair question. The data model is:
- Entities (customers, products, whatever)
- Derived attributes (stuff predicted by ML models)
- Models (with versions, training data refs, etc.)
- Lineage (W3C PROV - what generated what from what)

When you ask "which entities did model version 2.1.0 predict on?", that's a graph query. In SQL you'd need joins across lineage tables, model tables, entity tables, and it gets messy fast. In SPARQL:

```sparql
SELECT ?entity WHERE {
  ?attr prov:wasGeneratedBy <gph:model/mdl_123> .
  ?entity gph:hasDerivedAttribute ?attr .
}
```

Yeah, SPARQL syntax is ugly. But the query planner handles the graph traversal. You're not fighting an ORM.

## Scaling

Hash ranges are u64, so you've got 2^64 values to partition. The coordinator divides the space evenly based on the number of registered shards:

```bash
# 2 shards: 0-50%, 50-100%
# 3 shards: 0-33%, 33-67%, 67-100%
# 4 shards: 0-25%, 25-50%, 50-75%, 75-100%
```

To add shards: just start new shard processes with `--coordinator-url` pointing to your coordinator. They register automatically, get assigned the next sequential shard ID, and the coordinator recalculates hash ranges for all shards. No coordinator restart needed. The data doesn't move - new writes just go to the right shard based on hash. You'll have unbalanced shards for a while until writes even out. That's life.

To remove shards: stop the shard process. It'll stop sending heartbeats and the coordinator will mark it inactive after timeout (default: 60s). Queries will skip inactive shards. Data on that shard becomes unreachable until you bring it back or manually rebalance.

## Things That Might Bite You

**Memory:** Each shard loads its full RDF store into memory (well, RocksDB's block cache plus whatever Oxigraph does). Plan accordingly. We use ~4GB per shard with aggressive RocksDB tuning.

**RocksDB LOCK:** If a shard crashes, the LOCK file might stick around. Just `rm ./data/shard-N/LOCK` and restart. Same goes for the audit chain - `rm ./data/audit/LOCK` if the coordinator crashes. It's fine.

**Port conflicts:** Default ports are 8080 (coordinator HTTP), 9090 (coordinator gRPC), 9100+ (shards). Change them if you're running multiple instances.

**ulimit:** RocksDB opens a lot of files. Bump your file descriptor limit or it'll complain. `ulimit -n 65536` should work.

**Audit chain growth:** If you're doing millions of writes, the audit RocksDB will grow. Each entry is ~200 bytes, so 1M operations = ~200MB. Old entries can be archived to cold storage if you need to reclaim space, but you lose the ability to generate proofs for those entries.

## Monitoring

Coordinator exposes `/metrics` (Prometheus format) and `/health`. Shards have gRPC health checks:

```bash
grpcurl -plaintext localhost:9100 graphica.shard.v1.ShardService/Health
```

For debugging, shards support gRPC reflection:
```bash
grpcurl -plaintext localhost:9100 list
```

If audit is enabled, you can verify chain integrity at any time. This checks signatures, hash chain linkage, and Merkle tree consistency:
```bash
# Via logs (look for verification results)
RUST_LOG=graphica_coordinator::bitemporal::audit=debug

# Or programmatically via the audit API endpoints (if you've exposed them)
# The coordinator verifies the chain on startup automatically
```

Logs go to stdout. Control verbosity with `RUST_LOG`:
```bash
RUST_LOG=debug              # Everything (very noisy)
RUST_LOG=info               # Normal
RUST_LOG=graphica_coordinator=debug,graphica_shard=info  # Per-module
RUST_LOG=graphica_coordinator::bitemporal::audit=debug   # Audit chain details
```

## Troubleshooting

**"Coordinator won't start"** - Check GRPC_PORT is available. Shards will connect after coordinator is up.

**"Shards won't register"** - Check shards can reach coordinator gRPC port. Look for "Successfully registered" or "Successfully reconnected" in shard logs. Verify coordinator logs show registration attempts. Check `--coordinator-url` points to right host:port.

**"Shard data looks corrupted"** - Stop shard, `rm -rf ./data/shard-N/*`, restart. You'll lose that shard's data obviously. Shard will re-register and get a new shard ID.

**"Auth isn't working"** - For dev, `ENABLE_AUTH=false`. For production, check you set `JWT_SECRET` (32+ bytes base64). If you screwed up admin setup, nuke `./data/coordinator/rocksdb/users` and restart to get a new setup token.

**"Query returns no results but data exists"** - Check shard status with `/cluster/health`. Shards might not be Active. For dev, coordinator auto-activates them on startup. In production, they need to send heartbeats.

**"Audit chain verification failed"** - This is actually serious. It means someone tampered with the audit log or there's a bug. Check the logs for which entry failed (signature mismatch, broken hash chain, etc.). If you're certain it's not tampering, you can rebuild the Merkle tree from scratch. If it IS tampering, well, that's what the audit chain is for - you caught it.

**"Audit RocksDB won't open"** - Stale LOCK file. `rm ./data/audit/LOCK` and restart. If that doesn't work, rebuild from backup or accept data loss for audit entries (the actual RDF data is fine, it's in the shards).

## Development

Standard Rust stuff:
```bash
cargo test              # Run tests
cargo fmt               # Format code
cargo clippy            # Lint
```

To test the audit chain:
```bash
cd graphica-coordinator
cargo test --features cryptographic-audit
cargo test --test audited_shard_integration_test --features cryptographic-audit
```

Tests are mostly unit tests in `#[cfg(test)]` modules. Some integration tests in `tests/`. The shard tests actually start Oxigraph instances, so they're a bit slow. Audit chain tests verify signatures, hash linkage, and Merkle proofs - they're pretty thorough.

## API Endpoints

Full list at `/openapi.yaml`. Highlights:

**Public (no auth):**
- `GET /health` - health check
- `POST /auth/login` - get JWT
- `POST /auth/setup` - one-time admin creation

**Protected (need JWT):**
- `POST /api/v1/models` - register ML model
- `POST /api/v1/models/{id}/predictions` - record predictions
- `POST /api/v1/sparql` - run SPARQL queries
- `GET /api/v1/entities/{id}` - get entity with derived attributes
- `POST /api/v1/fusion/resolve` - merge duplicate entities

**Field Mapping (CSV/DB import):**
- `POST /api/v1/datasources/{source_id}/analyze-for-mapping` - analyze schema and generate mappings
- `POST /api/v1/mapping/sessions/{session_id}/review` - approve/reject mappings
- `POST /api/v1/mapping/sessions/{session_id}/apply` - store mappings as RDF
- `POST /api/v1/mapping/sessions/{session_id}/import` - import data with ontology alignment

**CSV-to-DB Bulk Loading:**
- `POST /api/v1/unified_mapping` - create unified session (multi-CSV to DB)
- `POST /api/v1/unified_mapping/{id}/resolve` - resolve field conflicts
- `POST /api/v1/unified_mapping/{id}/load` - bulk load to PostgreSQL (COPY mode)
- `GET /api/v1/unified_mapping/load/{job_id}/status` - check load progress

**SHACL-to-DDL:**
- `GET /api/v1/ddl/shacl/{shape_uri}` - generate DDL from SHACL shape
- `POST /api/v1/ddl/shacl/{shape_uri}/generate` - generate for specific dialect

Everything under `/api/v1/*` needs `Authorization: Bearer <token>`.

## Production Checklist

- [ ] Set `JWT_SECRET` env var (use `openssl rand -base64 32`)
- [ ] Use actual strong passwords (12+ chars, not "password123")
- [ ] Put shards on isolated network (coordinator -> shard only)
- [ ] Enable TLS for gRPC if going over public network
- [ ] Actually monitor `/metrics` and `/health`
- [ ] Rotate secrets periodically
- [ ] Back up shard data dirs (or don't, it's all reproducible from source data)
- [ ] Tune RocksDB for your workload (see `storage/rocks_config.rs`)
- [ ] Enable `ENABLE_CRYPTOGRAPHIC_AUDIT=true` if you need compliance (SOX/HIPAA/GDPR)
- [ ] Back up audit chain directory separately (regulators love immutable trails)

## Directory Layout

```
graphica/
├── graphica-core/           # Shared types, protos, RDF utils
├── graphica-coordinator/    # HTTP API, query router (doesn't store data)
├── graphica-shard/          # Oxigraph + RocksDB (actual storage)
├── build.sh                 # Build everything
└── run-local.sh             # Start 2 shards + coordinator
```

## Performance Notes

Some actual numbers from local testing (your mileage will vary):

- Query latency: ~1.3ms for simple SPARQL on 2 shards (localhost gRPC)
- Insert throughput: Limited by SPARQL parsing, not network or storage
- Deduplication overhead: Negligible (HashSet on string serialization)
- Memory per shard: ~4GB with default RocksDB config

The performance bottleneck is usually SPARQL query complexity, not the distribution layer. If queries are slow, it's probably Oxigraph parsing or executing complex graph patterns.

## Frontend

There's a React frontend. Point it at the coordinator with `VITE_API_URL=http://localhost:8080/api/v1`. It doesn't know about shards - that's the whole point.

## What's Missing

This is real code that actually works, but it's not feature-complete:

- No replication (single point of failure per shard)
- No TLS on gRPC (you need to add that)
- No query result caching (scatter-gather every time)
- No SPARQL query optimization (we trust Oxigraph)

These aren't bugs, they're just not implemented yet. PRs welcome if you need them.

## What's Actually Implemented (But Easy to Miss)

**HA coordinator with Raft consensus** - Run 3+ coordinators with automatic leader election and sub-second failover. Feature-gated (`--features raft-consensus`) so single-coordinator mode has zero overhead. Requests work on any coordinator (auto-forwarded to leader). Kill the leader and a new one is elected in < 1 second while requests continue working. Full documentation in `HA_LOCAL_TESTING_GUIDE.md`.

**Automatic shard rebalancing** - When data distribution becomes imbalanced (>20% deviation), the system detects it and creates a migration plan. Uses multi-factor load assessment (80% actual data size, 20% virtual node distribution). Moves ~25% of virtual nodes from overloaded shards to underloaded ones with rate limiting (50MB/s default) and progress tracking. Small migrations (<1GB) auto-approve, large ones need confirmation.

**Cryptographic audit trail** - Ed25519 signatures, SHA-256 hash chain, Merkle tree for batch verification. Every write operation gets logged with full provenance. It's feature-gated (`--features cryptographic-audit`) so you only pay for it if you need it. Persisted to RocksDB so it survives crashes.

**Consistent hashing with 150 virtual nodes** - Deterministic routing with minimal data movement when shards are added/removed. O(log n) lookups via binary search. Hash space coverage is tracked and factors into rebalancing decisions. You don't have to calculate hash ranges manually - the coordinator divides the space evenly and recalculates when topology changes.

**Unified ontology mapping engine** *(2025-10-29)* - Consolidated all ontology mapping logic into a single strategy-based engine (`graphica-coordinator/src/mapping/unified_ontology/`). Six matching strategies run in parallel: pattern detection (email/phone regex), semantic embeddings via graphica-model-service, statistical TF-IDF, lexical edit distance, custom registry terms, and name-based heuristics. Each strategy contributes weighted confidence scores (0.6-1.0 range) that get blended for final ranking. Caches embeddings and ontology terms to avoid redundant computation. Used by DDL generation, field mapping sessions, and CSV import workflows. Replaces three separate implementations that were duplicating the same logic with slightly different approaches. Performance benchmarks in `graphica-coordinator/benches/unified_ontology_bench.rs` show 1-5ms per field with warm cache, 10-50ms cold cache, and 50-200ms for batch mapping 10 fields.


---

That's basically it. Start the processes, they talk gRPC, data flows. If something breaks, read the logs - they're actually useful. If the logs don't help, the source code is right there.
