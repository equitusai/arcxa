#!/usr/bin/env python3
"""Validate the ArcXA ML pipeline demo through the live MCP interface."""

from __future__ import annotations

import argparse
import csv
import json
import os
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Dict, Optional

import pyarrow.parquet as pq

from run_arcxa_mcp_acceptance import (
    call_tool,
    ensure,
    extract_entries,
    normalize_status,
    wait_for_workflow_execution_record,
)
from run_arcxa_mcp_smoke import McpSession, REPO_ROOT


DEFAULT_ARTIFACT_DIR = REPO_ROOT / "artifacts" / "arcxa-mcp-ml-pipeline-demo-validation"
DEFAULT_BASE_URL = "http://localhost:18928"
DEFAULT_USERNAME = "admin"
DEFAULT_PASSWORD = "GraphicaDemoAdmin123!"
DEFAULT_DATASOURCE_TITLE = "postgres-ml-feature-demo"
DEFAULT_DATASET_NAME = "product_usage_signals"
CUSTOMER_WORKFLOW_ID = "ml-demo-customer-master-curation"
SUPPORT_WORKFLOW_ID = "ml-demo-support-signal-curation"
USAGE_WORKFLOW_ID = "ml-demo-product-usage-curation"
FEATURE_WORKFLOW_ID = "ml-demo-feature-assembly"
WORKFLOW_IDS = [
    CUSTOMER_WORKFLOW_ID,
    SUPPORT_WORKFLOW_ID,
    USAGE_WORKFLOW_ID,
    FEATURE_WORKFLOW_ID,
]
FINAL_TARGET_TABLE = "ml_demo.customer_training_features"
BOOTSTRAP_SUMMARY_PATH = "/app/data/bootstrap/ml-pipeline-demo-bootstrap-summary.json"
COORDINATOR_CONTAINER_NAME = "arcxa-ml-demo-coordinator"
EXPECTED_METRICS_PATH = (
    REPO_ROOT / "docker" / "ml-pipeline-demo" / "data" / "expected_metrics.json"
)
SUPPORT_CSV_PATH = REPO_ROOT / "docker" / "ml-pipeline-demo" / "data" / "support_tickets.csv"
USAGE_PARQUET_PATH = (
    REPO_ROOT / "docker" / "ml-pipeline-demo" / "data" / "product_usage.parquet"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--username", default=DEFAULT_USERNAME)
    parser.add_argument("--password", default=DEFAULT_PASSWORD)
    parser.add_argument("--datasource-title", default=DEFAULT_DATASOURCE_TITLE)
    parser.add_argument("--dataset-name", default=DEFAULT_DATASET_NAME)
    parser.add_argument(
        "--artifacts-dir",
        default=str(DEFAULT_ARTIFACT_DIR),
        help="Directory for validation artifacts.",
    )
    return parser.parse_args()


def http_json(
    base_url: str,
    method: str,
    path: str,
    *,
    token: Optional[str] = None,
    payload: Optional[Dict[str, Any]] = None,
    expected: tuple[int, ...] = (200,),
) -> Dict[str, Any]:
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        f"{base_url.rstrip('/')}{path}",
        data=body,
        method=method,
        headers=headers,
    )
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            status = response.getcode()
            raw = response.read()
    except urllib.error.HTTPError as exc:
        status = exc.code
        raw = exc.read()
        if status not in expected:
            raise RuntimeError(
                f"{method} {path} failed with status {status}: "
                f"{raw.decode('utf-8', errors='replace')}"
            ) from exc
    if status not in expected:
        raise RuntimeError(
            f"{method} {path} returned unexpected status {status}: "
            f"{raw.decode('utf-8', errors='replace')}"
        )
    return json.loads(raw.decode("utf-8")) if raw else {}


def login(base_url: str, username: str, password: str) -> str:
    response = http_json(
        base_url,
        "POST",
        "/auth/login",
        payload={"username": username, "password": password},
        expected=(200,),
    )
    token = response.get("token")
    ensure(isinstance(token, str) and token, f"Login did not return a token: {response}")
    return token


def find_source_by_title(payload: Dict[str, Any], title: str) -> Dict[str, Any]:
    sources = payload.get("sources", payload)
    for source in sources:
        if source.get("title") == title:
            return source
    raise RuntimeError(f"Datasource '{title}' was not found in the coordinator catalog")


def find_dataset_by_name(base_url: str, token: str, name: str) -> Dict[str, Any]:
    payload = http_json(
        base_url,
        "GET",
        "/api/v1/datasets?page=0&page_size=100",
        token=token,
    )
    for dataset in payload.get("datasets", []):
        if dataset.get("name") == name:
            return dataset
    raise RuntimeError(f"Dataset '{name}' was not found in the coordinator catalog")


def read_bootstrap_summary_from_container() -> Optional[Dict[str, Any]]:
    try:
        result = subprocess.run(
            [
                "docker",
                "exec",
                COORDINATOR_CONTAINER_NAME,
                "cat",
                BOOTSTRAP_SUMMARY_PATH,
            ],
            capture_output=True,
            text=True,
            check=True,
            timeout=10,
        )
    except (FileNotFoundError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        return None

    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    return payload if isinstance(payload, dict) else None


def resolve_demo_dataset_id(base_url: str, token: str, dataset_name: str) -> str:
    try:
        dataset = find_dataset_by_name(base_url, token, dataset_name)
    except RuntimeError:
        dataset = None

    if dataset:
        dataset_id = dataset.get("id")
        ensure(dataset_id, f"Dataset id missing from dataset summary: {dataset}")
        return str(dataset_id)

    bootstrap_summary = read_bootstrap_summary_from_container()
    ensure(
        bootstrap_summary is not None,
        f"Dataset '{dataset_name}' was not found in the coordinator catalog and no bootstrap summary was available",
    )
    ensure(
        bootstrap_summary.get("parquet_dataset_name") == dataset_name,
        f"Bootstrap summary does not match requested dataset '{dataset_name}': {bootstrap_summary}",
    )
    dataset_id = bootstrap_summary.get("parquet_dataset_id")
    ensure(
        isinstance(dataset_id, str) and dataset_id,
        f"Bootstrap summary did not contain a parquet dataset id: {bootstrap_summary}",
    )
    return dataset_id


def search_row_keys(base_url: str, token: str, query: str, limit: int = 10) -> Dict[str, Any]:
    encoded_query = urllib.parse.quote(query, safe="")
    return http_json(
        base_url,
        "GET",
        f"/api/v1/lineage/rows/search?q={encoded_query}&limit={limit}",
        token=token,
    )


def load_expected_metrics() -> Dict[str, Any]:
    return json.loads(EXPECTED_METRICS_PATH.read_text(encoding="utf-8"))


def count_support_csv_rows() -> int:
    with SUPPORT_CSV_PATH.open("r", encoding="utf-8", newline="") as handle:
        return sum(1 for _ in csv.DictReader(handle))


def count_support_csv_nulls() -> Dict[str, int]:
    counts = {
        "support_rows_with_status_default": 0,
        "support_rows_with_priority_default": 0,
        "support_rows_with_csat_default": 0,
    }
    with SUPPORT_CSV_PATH.open("r", encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            if not (row.get("ticket_status") or "").strip():
                counts["support_rows_with_status_default"] += 1
            if not (row.get("priority") or "").strip():
                counts["support_rows_with_priority_default"] += 1
            if not (row.get("csat_score") or "").strip():
                counts["support_rows_with_csat_default"] += 1
    return counts


def count_usage_parquet_rows() -> int:
    return pq.read_table(USAGE_PARQUET_PATH).num_rows


def count_usage_parquet_nulls() -> Dict[str, int]:
    table = pq.read_table(USAGE_PARQUET_PATH)
    counts = {
        "usage_rows_with_active_days_default": 0,
        "usage_rows_with_product_events_default": 0,
        "usage_rows_with_feature_adoption_default": 0,
    }
    for row in table.to_pylist():
        if row.get("active_days_30d") is None:
            counts["usage_rows_with_active_days_default"] += 1
        if row.get("product_events_30d") is None:
            counts["usage_rows_with_product_events_default"] += 1
        if row.get("feature_adoption_score") is None:
            counts["usage_rows_with_feature_adoption_default"] += 1
    return counts


def query_datasource(
    session: McpSession,
    next_id: Any,
    datasource_id: str,
    query: str,
    *,
    limit: int = 100,
) -> Dict[str, Any]:
    return call_tool(
        session,
        next_id(),
        "arcxa_query_datasource",
        {
            "datasource_id": datasource_id,
            "query": query,
            "limit": limit,
        },
    )


def scalar_from_row(row: Dict[str, Any], *keys: str) -> Any:
    for key in keys:
        if key in row:
            return row[key]
    raise KeyError(f"None of the keys {keys} were present in row: {row}")


def assert_number_matches(actual: Any, expected: Any, *, label: str) -> None:
    if isinstance(expected, int):
        ensure(int(float(actual)) == expected, f"{label} mismatch: expected {expected}, got {actual}")
        return
    if isinstance(expected, float):
        ensure(
            abs(float(actual) - expected) < 0.01,
            f"{label} mismatch: expected {expected}, got {actual}",
        )
        return
    ensure(str(actual) == str(expected), f"{label} mismatch: expected {expected}, got {actual}")


def query_single_row(
    session: McpSession,
    next_id: Any,
    datasource_id: str,
    query: str,
    *,
    limit: int = 1,
) -> Dict[str, Any]:
    payload = query_datasource(session, next_id, datasource_id, query, limit=limit)
    rows = payload.get("rows", [])
    ensure(rows, f"Datasource query returned no rows for query: {query!r}. Payload: {payload}")
    return rows[0]


def normalize_row_keys(row: Dict[str, Any]) -> Dict[str, Any]:
    normalized: Dict[str, Any] = {}
    for key, value in row.items():
        normalized[key] = value
        normalized[key.lower()] = value
    return normalized


def assert_metric_group(
    actual_row: Dict[str, Any],
    expected: Dict[str, Any],
    *,
    labels: list[str],
) -> None:
    normalized = normalize_row_keys(actual_row)
    for label in labels:
        assert_number_matches(
            normalized[label],
            expected[label],
            label=label,
        )


def run_workflow(
    session: McpSession,
    next_id: Any,
    workflow_id: str,
    workflow_input: Optional[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    start = call_tool(
        session,
        next_id(),
        "arcxa_execute_workflow",
        {
            "workflow_id": workflow_id,
            "input": workflow_input or {},
            "async_mode": True,
        },
    )
    execution_id = start.get("execution_id")
    if not execution_id:
        history = wait_for_workflow_execution_record(session, next_id, workflow_id)
        entries = extract_entries(history, "executions")
        ensure(entries, f"No execution record appeared for workflow {workflow_id}: {history}")
        execution_id = entries[0].get("execution_id") or entries[0].get("id")

    ensure(execution_id, f"Could not determine execution_id for workflow {workflow_id}: {start}")

    waited = call_tool(
        session,
        next_id(),
        "arcxa_wait_for_execution",
        {
            "execution_id": execution_id,
            "timeout_seconds": 120,
            "poll_interval_seconds": 0.5,
        },
    )
    ensure(waited.get("timed_out") is False, f"Execution timed out: {waited}")
    ensure(
        normalize_status(waited.get("status")) == "completed",
        f"Workflow execution did not complete successfully: {waited}",
    )
    execution = call_tool(
        session,
        next_id(),
        "arcxa_get_workflow_execution",
        {"execution_id": execution_id},
    )
    return {
        "execution_id": execution_id,
        "start": start,
        "wait": waited,
        "execution": execution,
    }


def get_first_row_value(row: Dict[str, Any], *keys: str) -> Optional[str]:
    for key in keys:
        value = row.get(key)
        if value is not None:
            return str(value)
    return None


def first_present(event: Dict[str, Any], keys: tuple[str, ...]) -> Optional[str]:
    for key in keys:
        value = event.get(key)
        if isinstance(value, str) and value:
            return value
    return None


def derive_row_key_from_run_lineage(run_lineage: Dict[str, Any]) -> Optional[str]:
    for event in run_lineage.get("events", []):
        row_key = first_present(
            event,
            ("output_row_id", "target_row_id", "source_row_id", "input_row_id", "row_id"),
        )
        if row_key:
            return row_key
    return None


def pick_target_row_key(
    search_payload: Dict[str, Any],
    *,
    preferred_source_id: str,
) -> Optional[str]:
    matches = search_payload.get("matches", [])
    for match in matches:
        if match.get("source_id") == preferred_source_id and isinstance(
            match.get("row_key"), str
        ):
            return match["row_key"]

    for match in matches:
        row_key = match.get("row_key")
        if isinstance(row_key, str) and preferred_source_id in row_key:
            return row_key

    return None


def resolve_example_row_key(
    *,
    base_url: str,
    token: str,
    run_lineage: Dict[str, Any],
    preferred_source_id: str,
    search_queries: list[str],
) -> tuple[str, list[Dict[str, Any]]]:
    row_key = derive_row_key_from_run_lineage(run_lineage)
    search_attempts: list[Dict[str, Any]] = []

    if row_key:
        return row_key, search_attempts

    for query in search_queries:
        search_payload = search_row_keys(base_url, token, query, limit=10)
        search_attempts.append(search_payload)
        row_key = pick_target_row_key(
            search_payload,
            preferred_source_id=preferred_source_id,
        )
        if row_key:
            return row_key, search_attempts

    raise RuntimeError(
        "Could not derive a row key from run lineage or row search. "
        f"Run lineage: {run_lineage}. Search attempts: {search_attempts}"
    )


def run_validation(args: argparse.Namespace) -> Dict[str, Any]:
    artifact_dir = Path(args.artifacts_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)

    os.environ["ARCXA_USERNAME"] = args.username
    os.environ["ARCXA_PASSWORD"] = args.password

    summary: Dict[str, Any] = {}
    token = login(args.base_url, args.username, args.password)
    request_id = 1

    def next_id() -> int:
        nonlocal request_id
        value = request_id
        request_id += 1
        return value

    session = McpSession(args.base_url)
    try:
        initialize = session.send(
            {
                "jsonrpc": "2.0",
                "id": next_id(),
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "arcxa-mcp-ml-pipeline-demo-validation",
                        "version": "1.0",
                    },
                },
            }
        )
        summary["initialize"] = initialize["result"]

        health = call_tool(session, next_id(), "arcxa_health_check", {})
        summary["health_status"] = health.get("status")
        ensure(health.get("status") == "alive", f"Coordinator health failed: {health}")

        datasources = call_tool(
            session,
            next_id(),
            "arcxa_list_datasources",
            {"page": 0, "page_size": 50},
        )
        source = find_source_by_title(datasources, args.datasource_title)
        datasource_id = source.get("@id") or source.get("id")
        ensure(datasource_id, f"Datasource is missing an id: {source}")
        summary["postgres_datasource_id"] = datasource_id

        dataset_id = resolve_demo_dataset_id(args.base_url, token, args.dataset_name)
        summary["parquet_dataset_id"] = dataset_id

        workflow_runs = {}
        workflow_runs[CUSTOMER_WORKFLOW_ID] = run_workflow(
            session, next_id, CUSTOMER_WORKFLOW_ID
        )
        workflow_runs[SUPPORT_WORKFLOW_ID] = run_workflow(
            session, next_id, SUPPORT_WORKFLOW_ID
        )
        workflow_runs[USAGE_WORKFLOW_ID] = run_workflow(
            session,
            next_id,
            USAGE_WORKFLOW_ID,
            {"type": "dataset", "dataset_id": dataset_id},
        )
        workflow_runs[FEATURE_WORKFLOW_ID] = run_workflow(
            session, next_id, FEATURE_WORKFLOW_ID
        )
        summary["workflow_executions"] = {
            workflow_id: run["execution_id"] for workflow_id, run in workflow_runs.items()
        }

        expected_metrics = load_expected_metrics()
        summary["expected_metrics"] = {
            "customer_source_row_count": expected_metrics["customer_source_row_count"],
            "support_source_row_count": expected_metrics["support_source_row_count"],
            "usage_source_row_count": expected_metrics["usage_source_row_count"],
            "curated_customer_count": expected_metrics["curated_customer_count"],
            "customer_source_rows_with_segment_null": expected_metrics[
                "customer_source_rows_with_segment_null"
            ],
            "customer_source_rows_with_plan_tier_null": expected_metrics[
                "customer_source_rows_with_plan_tier_null"
            ],
            "customer_source_rows_with_marketing_source_null": expected_metrics[
                "customer_source_rows_with_marketing_source_null"
            ],
            "curated_customer_rows_with_segment_default": expected_metrics[
                "curated_customer_rows_with_segment_default"
            ],
            "curated_customer_rows_with_plan_tier_default": expected_metrics[
                "curated_customer_rows_with_plan_tier_default"
            ],
            "curated_customer_rows_with_marketing_source_default": expected_metrics[
                "curated_customer_rows_with_marketing_source_default"
            ],
            "support_feature_customer_count": expected_metrics["support_feature_customer_count"],
            "usage_feature_customer_count": expected_metrics["usage_feature_customer_count"],
            "final_feature_row_count": expected_metrics["final_feature_row_count"],
            "ml_sample_usable_count": expected_metrics["ml_sample_usable_count"],
            "ml_sample_unusable_count": expected_metrics["ml_sample_unusable_count"],
            "lineage_example_email": expected_metrics["lineage_example_email"],
        }

        support_csv_row_count = count_support_csv_rows()
        support_csv_null_counts = count_support_csv_nulls()
        usage_parquet_row_count = count_usage_parquet_rows()
        usage_parquet_null_counts = count_usage_parquet_nulls()
        summary["local_fixture_counts"] = {
            "support_source_row_count": support_csv_row_count,
            "usage_source_row_count": usage_parquet_row_count,
            **support_csv_null_counts,
            **usage_parquet_null_counts,
        }
        assert_number_matches(
            support_csv_row_count,
            expected_metrics["support_source_row_count"],
            label="support_source_row_count",
        )
        assert_number_matches(
            usage_parquet_row_count,
            expected_metrics["usage_source_row_count"],
            label="usage_source_row_count",
        )
        for label, actual in support_csv_null_counts.items():
            assert_number_matches(actual, expected_metrics[label], label=label)
        for label, actual in usage_parquet_null_counts.items():
            assert_number_matches(actual, expected_metrics[label], label=label)

        customer_source_stats = query_single_row(
            session,
            next_id,
            datasource_id,
            """
            SELECT
                COUNT(*) AS customer_source_row_count,
                SUM(CASE WHEN segment IS NULL THEN 1 ELSE 0 END) AS customer_source_rows_with_segment_null,
                SUM(CASE WHEN plan_tier IS NULL THEN 1 ELSE 0 END) AS customer_source_rows_with_plan_tier_null,
                SUM(CASE WHEN marketing_source IS NULL THEN 1 ELSE 0 END) AS customer_source_rows_with_marketing_source_null
            FROM ml_demo.crm_customers
            """,
        )
        summary["customer_source_stats"] = customer_source_stats
        assert_metric_group(
            customer_source_stats,
            expected_metrics,
            labels=[
                "customer_source_row_count",
                "customer_source_rows_with_segment_null",
                "customer_source_rows_with_plan_tier_null",
                "customer_source_rows_with_marketing_source_null",
            ],
        )

        curated_customer_stats = query_single_row(
            session,
            next_id,
            datasource_id,
            """
            SELECT
                COUNT(*) AS curated_customer_count,
                SUM(CASE WHEN segment = 'UNKNOWN_SEGMENT' THEN 1 ELSE 0 END) AS curated_customer_rows_with_segment_default,
                SUM(CASE WHEN plan_tier = 'STANDARD' THEN 1 ELSE 0 END) AS curated_customer_rows_with_plan_tier_default,
                SUM(CASE WHEN marketing_source = 'unknown' THEN 1 ELSE 0 END) AS curated_customer_rows_with_marketing_source_default,
                SUM(CASE WHEN segment IS NULL THEN 1 ELSE 0 END) AS segment_nulls,
                SUM(CASE WHEN plan_tier IS NULL THEN 1 ELSE 0 END) AS plan_tier_nulls,
                SUM(CASE WHEN marketing_source IS NULL THEN 1 ELSE 0 END) AS marketing_source_nulls
            FROM ml_demo.customer_master_curated
            """,
        )
        summary["curated_customer_stats"] = curated_customer_stats
        assert_metric_group(
            curated_customer_stats,
            expected_metrics,
            labels=[
                "curated_customer_count",
                "curated_customer_rows_with_segment_default",
                "curated_customer_rows_with_plan_tier_default",
                "curated_customer_rows_with_marketing_source_default",
            ],
        )
        for label in ("segment_nulls", "plan_tier_nulls", "marketing_source_nulls"):
            assert_number_matches(curated_customer_stats[label], 0, label=label)

        support_feature_stats = query_single_row(
            session,
            next_id,
            datasource_id,
            """
            SELECT
                COUNT(*) AS support_feature_customer_count,
                SUM(CASE WHEN customer_email IS NULL THEN 1 ELSE 0 END) AS support_customer_email_nulls,
                SUM(CASE WHEN ticket_count_90d IS NULL THEN 1 ELSE 0 END) AS support_ticket_count_nulls,
                SUM(CASE WHEN avg_csat_90d IS NULL THEN 1 ELSE 0 END) AS support_avg_csat_nulls
            FROM ml_demo.customer_support_features
            """,
        )
        summary["support_feature_stats"] = support_feature_stats
        assert_metric_group(
            support_feature_stats,
            expected_metrics,
            labels=["support_feature_customer_count"],
        )
        for label in (
            "support_customer_email_nulls",
            "support_ticket_count_nulls",
            "support_avg_csat_nulls",
        ):
            assert_number_matches(support_feature_stats[label], 0, label=label)

        usage_feature_stats = query_single_row(
            session,
            next_id,
            datasource_id,
            """
            SELECT
                COUNT(*) AS usage_feature_customer_count,
                SUM(CASE WHEN customer_email IS NULL THEN 1 ELSE 0 END) AS usage_customer_email_nulls,
                SUM(CASE WHEN avg_active_days_30d IS NULL THEN 1 ELSE 0 END) AS usage_active_days_nulls,
                SUM(CASE WHEN total_product_events_30d IS NULL THEN 1 ELSE 0 END) AS usage_product_events_nulls,
                SUM(CASE WHEN feature_adoption_score IS NULL THEN 1 ELSE 0 END) AS usage_feature_adoption_nulls
            FROM ml_demo.customer_usage_features
            """,
        )
        summary["usage_feature_stats"] = usage_feature_stats
        assert_metric_group(
            usage_feature_stats,
            expected_metrics,
            labels=["usage_feature_customer_count"],
        )
        for label in (
            "usage_customer_email_nulls",
            "usage_active_days_nulls",
            "usage_product_events_nulls",
            "usage_feature_adoption_nulls",
        ):
            assert_number_matches(usage_feature_stats[label], 0, label=label)

        final_feature_stats = query_single_row(
            session,
            next_id,
            datasource_id,
            f"""
            SELECT
                COUNT(*) AS final_feature_row_count,
                SUM(support_signal_available) AS support_signal_available_count,
                SUM(usage_signal_available) AS usage_signal_available_count,
                SUM(ml_sample_usable) AS ml_sample_usable_count,
                SUM(CASE WHEN ml_sample_usable = 0 THEN 1 ELSE 0 END) AS ml_sample_unusable_count,
                SUM(CASE WHEN support_signal_available = 1 AND usage_signal_available = 1 AND ml_sample_usable = 0 THEN 1 ELSE 0 END) AS unexpectedly_unusable_rows,
                SUM(CASE WHEN (support_signal_available = 0 OR usage_signal_available = 0) AND ml_sample_usable = 1 THEN 1 ELSE 0 END) AS unexpectedly_usable_rows,
                SUM(CASE WHEN churn_label IS NULL THEN 1 ELSE 0 END) AS churn_label_nulls
            FROM {FINAL_TARGET_TABLE}
            """,
        )
        summary["final_feature_stats"] = final_feature_stats
        assert_metric_group(
            final_feature_stats,
            expected_metrics,
            labels=[
                "final_feature_row_count",
                "support_signal_available_count",
                "usage_signal_available_count",
                "ml_sample_usable_count",
                "ml_sample_unusable_count",
            ],
        )
        for label in ("unexpectedly_unusable_rows", "unexpectedly_usable_rows", "churn_label_nulls"):
            assert_number_matches(final_feature_stats[label], 0, label=label)

        sample_emails = sorted(expected_metrics["sample_rows"].keys())
        quoted_emails = ", ".join(f"'{email}'" for email in sample_emails)
        sample_rows_payload = query_datasource(
            session,
            next_id,
            datasource_id,
            f"""
            SELECT
                customer_email,
                customer_id,
                full_name,
                country_code,
                segment,
                plan_tier,
                monthly_revenue_usd,
                ticket_count_90d,
                avg_csat_90d,
                avg_active_days_30d,
                total_product_events_30d,
                feature_adoption_score,
                support_signal_available,
                usage_signal_available,
                ml_sample_usable,
                churn_label
            FROM {FINAL_TARGET_TABLE}
            WHERE customer_email IN ({quoted_emails})
            ORDER BY customer_email
            """,
            limit=len(sample_emails),
        )
        sample_rows = sample_rows_payload.get("rows", [])
        ensure(
            len(sample_rows) == len(sample_emails),
            f"Expected {len(sample_emails)} sample feature rows, got {len(sample_rows)}: {sample_rows_payload}",
        )
        rows_by_customer = {
            get_first_row_value(row, "customer_email", "CUSTOMER_EMAIL"): normalize_row_keys(row)
            for row in sample_rows
        }
        summary["sample_rows"] = rows_by_customer
        for email, expected_row in expected_metrics["sample_rows"].items():
            actual_row = rows_by_customer[email]
            for field, expected_value in expected_row.items():
                assert_number_matches(
                    actual_row[field],
                    expected_value,
                    label=f"{email}.{field}",
                )

        final_execution_id = workflow_runs[FEATURE_WORKFLOW_ID]["execution_id"]
        run_lineage = call_tool(
            session,
            next_id(),
            "arcxa_get_run_lineage",
            {"run_id": final_execution_id},
        )
        summary["final_run_lineage_total_records"] = run_lineage.get("total_records")
        lineage_events = run_lineage.get("events", [])
        ensure(
            (run_lineage.get("total_records") or 0) > 0 and lineage_events,
            f"Run lineage did not return events: {run_lineage}",
        )

        row_key, row_search_attempts = resolve_example_row_key(
            base_url=args.base_url,
            token=token,
            run_lineage=run_lineage,
            preferred_source_id=FINAL_TARGET_TABLE,
            search_queries=[
                expected_metrics["lineage_example_email"],
                FINAL_TARGET_TABLE,
                FINAL_TARGET_TABLE.split(".")[-1],
            ],
        )
        summary["example_row_key"] = row_key
        ensure(
            expected_metrics["lineage_example_email"] in row_key,
            f"Resolved row key did not include the expected lineage example email: {row_key}",
        )
        if row_search_attempts:
            summary["row_search_attempts"] = row_search_attempts

        row_lineage = call_tool(
            session,
            next_id(),
            "arcxa_get_row_lineage",
            {"row_key": row_key},
        )
        summary["row_lineage_event_count"] = row_lineage.get("total_count")
        ensure(
            (row_lineage.get("total_count") or 0) > 0,
            f"Row lineage did not return events for {row_key}: {row_lineage}",
        )

        journey = call_tool(
            session,
            next_id(),
            "arcxa_get_row_journey",
            {"row_key": row_key},
        )
        journey_steps = journey.get("steps", [])
        summary["row_journey_step_count"] = len(journey_steps)
        ensure(journey_steps, f"Row journey returned no steps for {row_key}: {journey}")

        search_by_email = search_row_keys(
            args.base_url,
            token,
            expected_metrics["lineage_example_email"],
            limit=5,
        )
        search_matches = search_by_email.get("matches", [])
        ensure(search_matches, f"Row search did not return matches: {search_by_email}")
        summary["row_search_matches"] = search_matches
        ensure(
            any(match.get("row_key") == row_key for match in search_matches),
            f"Row search results did not contain the resolved row key {row_key}: {search_by_email}",
        )

        summary_path = artifact_dir / "summary.json"
        summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
        return summary
    finally:
        session.close()


def main() -> None:
    args = parse_args()
    summary = run_validation(args)
    print(json.dumps(summary, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
