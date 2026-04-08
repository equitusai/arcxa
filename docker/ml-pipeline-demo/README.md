# ArcXA ML Pipeline Demo

This demo shows ArcXA as a trustworthy feature-engineering system for machine learning workflows.

It uses three source types:

- PostgreSQL CRM data
- CSV support signal data
- Parquet product usage data

ArcXA then:

- cleans and validates each source
- aligns each source to the same customer ontology
- deduplicates noisy inputs
- materializes curated feature tables
- assembles a final training-feature dataset
- records row-level lineage so you can inspect where a value came from

The fixture is intentionally realistic rather than tiny:

- `3,490` PostgreSQL CRM rows that collapse to `3,200` curated customers after duplicate merges
- `3,976` CSV support rows with sparse status, priority, and CSAT fields
- `2,922` Parquet usage rows with sparse numerical activity fields
- `3,200` final training-feature rows, including `1,920` ML-usable samples and `1,280` samples flagged as unusable because at least one upstream signal family is missing

Because the fixture is deterministic, we know exactly:

- how many duplicates must merge
- how many nulls must be repaired to default values
- which final rows should be usable for model training
- what exact feature values should exist for several sample customers
- which lineage row key should resolve to a real feature vector

## Bring up the demo

```bash
docker compose -f docker/ml-pipeline-demo/docker-compose.yml up -d --build
```

If you rebuild or recreate only the coordinator later, rerun the bootstrap service so the
demo workflows are registered again:

```bash
docker compose -f docker/ml-pipeline-demo/docker-compose.yml up bootstrap
```

If you want to regenerate the synthetic fixture locally before bringing the demo up:

```bash
python3 docker/ml-pipeline-demo/generate_synthetic_ml_demo_data.py
```

## URLs

- Frontend: `http://localhost:13020`
- Coordinator API: `http://localhost:18928`
- PostgreSQL host port: `localhost:15444`

## Login

- Username: `admin`
- Password: `GraphicaDemoAdmin123!`

## Preconfigured datasource

- PostgreSQL datasource: `postgres-ml-feature-demo`
- PostgreSQL schema: `ml_demo`

## Preconfigured workflows

- `ml-demo-customer-master-curation`
- `ml-demo-support-signal-curation`
- `ml-demo-product-usage-curation`
- `ml-demo-feature-assembly`

## Run the full demo and validate it

```bash
python3 scripts/ci/run_arcxa_mcp_ml_pipeline_demo_validation.py
```

That script will:

- confirm the stack is healthy
- find the bootstrapped PostgreSQL datasource and Parquet dataset
- run the four workflows in the correct order
- verify exact source, curated, and final-table counts
- verify the known null-repair and duplicate-merge expectations
- verify exact final feature values for several known sample customers
- verify row-search, row-lineage, and row-journey behavior
- print a summary artifact with example row keys to inspect in the frontend

The validation artifact is written to:

- `artifacts/arcxa-mcp-ml-pipeline-demo-validation/summary.json`

The current validated example row key is built from:

- `customer0011@demo-ml.example`

The current validated end-to-end expectations are:

- curated customer rows: `3,200`
- support feature rows: `2,560`
- usage feature rows: `2,400`
- final feature rows: `3,200`
- ML-usable rows: `1,920`
- ML-unusable rows: `1,280`

## Demo script to follow in the frontend

If you want to walk through the demo manually in ArcXA instead of running the validation
script, use this order:

1. Open `http://localhost:13020` and log in with `admin` / `GraphicaDemoAdmin123!`.
2. Go to `Workflows`.
3. Run these workflows in sequence:
   - `ml-demo-customer-master-curation`
   - `ml-demo-support-signal-curation`
   - `ml-demo-product-usage-curation`
   - `ml-demo-feature-assembly`

Workflow input notes:

- `ml-demo-customer-master-curation`
  - Use workflow source steps.
- `ml-demo-support-signal-curation`
  - Use workflow source steps.
- `ml-demo-product-usage-curation`
  - Use dataset input.
  - Select the imported dataset `product_usage_signals`.
- `ml-demo-feature-assembly`
  - Use workflow source steps.

What each workflow does:

- `ml-demo-customer-master-curation`
  - Reads PostgreSQL CRM rows, cleans and validates them, deduplicates noisy customer records,
    repairs sparse categorical fields, and writes a curated customer master table.
- `ml-demo-support-signal-curation`
  - Reads CSV support tickets, repairs sparse support fields, validates ticket quality,
    aggregates support features, and writes curated support features.
- `ml-demo-product-usage-curation`
  - Reads Parquet product usage signals, repairs sparse numeric usage fields, validates them,
    deduplicates usage events, aggregates engagement features, and writes curated usage features.
- `ml-demo-feature-assembly`
  - Joins all curated feature sources into one training-feature table, computes ML eligibility
    flags, and records row lineage.

## What the final feature dataset contains

The final dataset lands in:

- `ml_demo.customer_training_features`

It includes features such as:

- `monthly_revenue_usd`
- `ticket_count_90d`
- `avg_csat_90d`
- `avg_active_days_30d`
- `total_product_events_30d`
- `feature_adoption_score`
- `support_signal_available`
- `usage_signal_available`
- `ml_sample_usable`
- `churn_label`

Two especially useful demo claims to verify:

- every customer from the curated customer master lands in the final feature table
- customers missing either support or usage coverage are still retained, but flagged with `ml_sample_usable = 0`

## Lineage in the frontend

After you run the validation script, open:

- `http://localhost:13020/lineage`

Use the row-journey search to inspect a customer feature row. The page now supports
typeahead backed by the lineage row-search API, so you can search by values like:

- `customer0011@demo-ml.example`
- `customer_training_features`

The validation script writes the exact example row key to its summary artifact, and the
currently validated row is:

- `postgres:ml_demo.customer_training_features:customer_email=customer0011@demo-ml.example`

## Bootstrap summary

The bootstrap container writes its summary to:

- `/app/data/bootstrap/ml-pipeline-demo-bootstrap-summary.json`
