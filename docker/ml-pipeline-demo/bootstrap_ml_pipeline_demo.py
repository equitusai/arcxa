#!/usr/bin/env python3
"""Bootstrap the ArcXA machine-learning pipeline demo."""

from __future__ import annotations

import json
import mimetypes
import os
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path
from typing import Any, Dict, Optional


COORDINATOR_URL = os.environ.get("COORDINATOR_URL", "http://coordinator:8080").rstrip("/")
ADMIN_USERNAME = os.environ.get("ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("ADMIN_PASSWORD", "GraphicaDemoAdmin123!")
SETUP_TOKEN_PATH = Path(
    os.environ.get("SETUP_TOKEN_PATH", "/coordinator-data/bootstrap/setup-token.txt")
)
SUMMARY_PATH = Path(
    os.environ.get(
        "BOOTSTRAP_SUMMARY_PATH",
        "/coordinator-data/bootstrap/ml-pipeline-demo-bootstrap-summary.json",
    )
)

POSTGRES_DATASOURCE_TITLE = os.environ.get(
    "POSTGRES_DATASOURCE_TITLE", "postgres-ml-feature-demo"
)
POSTGRES_HOST = os.environ.get("POSTGRES_HOST", "postgres")
POSTGRES_PORT = int(os.environ.get("POSTGRES_PORT", "5432"))
POSTGRES_DATABASE = os.environ.get("POSTGRES_DATABASE", "arcxa_ml_demo")
POSTGRES_SCHEMA = os.environ.get("POSTGRES_SCHEMA", "ml_demo")
POSTGRES_USERNAME = os.environ.get("POSTGRES_USERNAME", "arcxa_demo")
POSTGRES_PASSWORD = os.environ.get("POSTGRES_PASSWORD", "arcxa_demo")

PARQUET_DATASET_NAME = os.environ.get("PARQUET_DATASET_NAME", "product_usage_signals")
PARQUET_DATASET_FILE = Path(
    os.environ.get("PARQUET_DATASET_FILE", "/demo-data/product_usage.parquet")
)

CUSTOMER_WORKFLOW_ID = "ml-demo-customer-master-curation"
SUPPORT_WORKFLOW_ID = "ml-demo-support-signal-curation"
USAGE_WORKFLOW_ID = "ml-demo-product-usage-curation"
FEATURE_WORKFLOW_ID = "ml-demo-feature-assembly"


def log(message: str) -> None:
    print(f"[ml-pipeline-demo-bootstrap] {message}", flush=True)


def request_json(
    method: str,
    path: str,
    payload: Optional[Dict[str, Any]] = None,
    token: Optional[str] = None,
    expected: tuple[int, ...] = (200,),
    timeout_seconds: int = 60,
    headers: Optional[Dict[str, str]] = None,
) -> Any:
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    request_headers = {"Content-Type": "application/json"}
    if headers:
        request_headers.update(headers)
    if token:
        request_headers["Authorization"] = f"Bearer {token}"

    request = urllib.request.Request(
        f"{COORDINATOR_URL}{path}",
        data=body,
        method=method,
        headers=request_headers,
    )

    try:
        with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
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
    except urllib.error.URLError as exc:
        raise RuntimeError(f"{method} {path} failed: {exc}") from exc

    if status not in expected:
        raise RuntimeError(
            f"{method} {path} returned unexpected status {status}: "
            f"{raw.decode('utf-8', errors='replace')}"
        )

    if not raw:
        return None

    text = raw.decode("utf-8")
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text


def request_multipart(
    method: str,
    path: str,
    *,
    token: Optional[str],
    file_field: str,
    file_path: Path,
    metadata_field: str,
    metadata_payload: Dict[str, Any],
    expected: tuple[int, ...] = (200,),
    timeout_seconds: int = 180,
) -> Dict[str, Any]:
    boundary = f"----arcxa-demo-{uuid.uuid4().hex}"
    metadata_bytes = json.dumps(metadata_payload).encode("utf-8")
    file_bytes = file_path.read_bytes()
    filename = file_path.name
    content_type = mimetypes.guess_type(filename)[0] or "application/octet-stream"

    body = bytearray()

    def add_part(headers: list[str], payload: bytes) -> None:
        body.extend(f"--{boundary}\r\n".encode("utf-8"))
        for header in headers:
            body.extend(f"{header}\r\n".encode("utf-8"))
        body.extend(b"\r\n")
        body.extend(payload)
        body.extend(b"\r\n")

    add_part(
        [
            f'Content-Disposition: form-data; name="{file_field}"; filename="{filename}"',
            f"Content-Type: {content_type}",
        ],
        file_bytes,
    )
    add_part(
        [
            f'Content-Disposition: form-data; name="{metadata_field}"',
            "Content-Type: application/json",
        ],
        metadata_bytes,
    )
    body.extend(f"--{boundary}--\r\n".encode("utf-8"))

    headers = {"Content-Type": f"multipart/form-data; boundary={boundary}"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    request = urllib.request.Request(
        f"{COORDINATOR_URL}{path}",
        data=bytes(body),
        method=method,
        headers=headers,
    )

    try:
        with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
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

    return json.loads(raw.decode("utf-8"))


def wait_for_health(timeout_seconds: int = 300) -> None:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            request_json("GET", "/health")
            log("Coordinator is healthy")
            return
        except Exception:
            time.sleep(2)
    raise RuntimeError(f"Coordinator at {COORDINATOR_URL} did not become healthy in time")


def try_login() -> Optional[str]:
    try:
        response = request_json(
            "POST",
            "/auth/login",
            {"username": ADMIN_USERNAME, "password": ADMIN_PASSWORD},
            expected=(200, 401, 400),
        )
    except RuntimeError as exc:
        log(f"Login probe failed: {exc}")
        return None

    if isinstance(response, dict) and response.get("token"):
        return response["token"]
    return None


def wait_for_setup_token(timeout_seconds: int = 90) -> str:
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        if SETUP_TOKEN_PATH.exists():
            token = SETUP_TOKEN_PATH.read_text(encoding="utf-8").strip()
            if token:
                return token
        time.sleep(1)
    raise RuntimeError(f"Setup token file '{SETUP_TOKEN_PATH}' was not populated in time")


def ensure_admin_login() -> str:
    token = try_login()
    if token:
        log(f"Admin login succeeded for '{ADMIN_USERNAME}'")
        return token

    setup_token = wait_for_setup_token()
    log("Creating the initial admin user through /auth/setup")
    request_json(
        "POST",
        "/auth/setup",
        {"setup_token": setup_token, "password": ADMIN_PASSWORD},
        expected=(200, 409),
    )

    token = try_login()
    if not token:
        raise RuntimeError("Admin login still failed after setup")
    log(f"Admin login succeeded for '{ADMIN_USERNAME}' after setup")
    return token


def datasource_id_of(datasource: Dict[str, Any]) -> str:
    datasource_id = datasource.get("id") or datasource.get("@id")
    if not datasource_id:
        raise KeyError("Datasource response did not include 'id' or '@id'")
    return datasource_id


def find_existing_datasource(token: str, title: str) -> Optional[Dict[str, Any]]:
    response = request_json("GET", "/api/v1/datasources", token=token)
    sources = response.get("sources", []) if isinstance(response, dict) else []
    for source in sources:
        if source.get("title") == title:
            return source
    return None


def get_datasource(token: str, datasource_id: str) -> Dict[str, Any]:
    return request_json("GET", f"/api/v1/datasources/{datasource_id}", token=token)


def delete_datasource(token: str, datasource_id: str) -> None:
    request_json(
        "DELETE",
        f"/api/v1/datasources/{datasource_id}",
        token=token,
        expected=(200, 204),
    )


def test_datasource(token: str, datasource_id: str) -> Dict[str, Any]:
    result = request_json(
        "POST",
        "/api/v1/datasources/test",
        {"sourceId": datasource_id},
        token=token,
    )
    if not result.get("success"):
        raise RuntimeError(
            f"Datasource test failed for {datasource_id}: {result.get('error') or result}"
        )
    return result


def query_datasource(
    token: str,
    datasource_id: str,
    query: str,
    *,
    limit: int = 20,
    timeout_seconds: int = 60,
) -> Dict[str, Any]:
    return request_json(
        "POST",
        f"/api/v1/datasources/{datasource_id}/query",
        {"sourceId": datasource_id, "query": query, "limit": limit},
        token=token,
        timeout_seconds=timeout_seconds,
    )


def find_dataset_by_name(token: str, name: str) -> Optional[Dict[str, Any]]:
    response = request_json(
        "GET",
        "/api/v1/datasets?page=0&page_size=100",
        token=token,
        expected=(200,),
    )
    datasets = response.get("datasets", []) if isinstance(response, dict) else []
    for dataset in datasets:
        if dataset.get("name") == name:
            return dataset
    return None


def build_postgres_datasource_payload() -> Dict[str, Any]:
    return {
        "title": POSTGRES_DATASOURCE_TITLE,
        "sourceType": "PostgreSQL",
        "connection": {
            "secretRef": f"inline://{POSTGRES_DATASOURCE_TITLE}",
            "config": {
                "type": "PostgreSQL",
                "host": POSTGRES_HOST,
                "port": POSTGRES_PORT,
                "database": POSTGRES_DATABASE,
                "schema": POSTGRES_SCHEMA,
            },
            "encryptionEnabled": False,
            "credentials": {
                "username": POSTGRES_USERNAME,
                "password": POSTGRES_PASSWORD,
            },
        },
        "tags": ["demo", "ml", "postgresql", "feature-engineering"],
        "metadata": {
            "owner": "ml-pipeline-demo-compose",
            "fixture": "ml-pipeline-demo",
        },
    }


def ensure_datasource(token: str) -> tuple[Dict[str, Any], Dict[str, Any], bool]:
    datasource = find_existing_datasource(token, POSTGRES_DATASOURCE_TITLE)
    if datasource is not None:
        datasource_id = datasource_id_of(datasource)
        try:
            test_result = test_datasource(token, datasource_id)
            current_datasource = get_datasource(token, datasource_id)
            log(f"Reused existing datasource '{POSTGRES_DATASOURCE_TITLE}' ({datasource_id})")
            return current_datasource, test_result, False
        except Exception as exc:
            log(
                f"Existing datasource '{POSTGRES_DATASOURCE_TITLE}' failed validation, recreating it: {exc}"
            )
            delete_datasource(token, datasource_id)

    datasource = request_json(
        "POST", "/api/v1/datasources", build_postgres_datasource_payload(), token=token
    )
    datasource_id = datasource_id_of(datasource)
    test_result = test_datasource(token, datasource_id)
    current_datasource = get_datasource(token, datasource_id)
    log(f"Created datasource '{POSTGRES_DATASOURCE_TITLE}' ({datasource_id})")
    return current_datasource, test_result, True


def ensure_parquet_dataset(token: str) -> tuple[Dict[str, Any], bool]:
    dataset = find_dataset_by_name(token, PARQUET_DATASET_NAME)
    if dataset:
        log(f"Reused existing dataset '{PARQUET_DATASET_NAME}' ({dataset['id']})")
        return dataset, False

    metadata = {
        "name": PARQUET_DATASET_NAME,
        "description": "Thirty-day product usage signals for the ArcXA ML pipeline demo.",
        "tags": ["demo", "ml", "parquet", "feature-engineering"],
    }
    response = request_multipart(
        "POST",
        "/api/v1/datasets/import",
        token=token,
        file_field="file",
        file_path=PARQUET_DATASET_FILE,
        metadata_field="metadata",
        metadata_payload=metadata,
        expected=(200,),
        timeout_seconds=180,
    )
    dataset = find_dataset_by_name(token, PARQUET_DATASET_NAME) or {
        "id": response["dataset_id"],
        "name": response["name"],
        "record_count": response.get("record_count"),
    }
    log(f"Imported dataset '{PARQUET_DATASET_NAME}' ({response['dataset_id']})")
    return dataset, True


def build_customer_master_workflow(datasource_id: str) -> Dict[str, Any]:
    return {
        "id": CUSTOMER_WORKFLOW_ID,
        "name": "ML Demo Customer Master Curation",
        "description": (
            "Extract noisy customer master data from PostgreSQL, clean it, repair sparse "
            "categorical fields, validate it, align it to the customer ontology, and "
            "materialize a curated customer feature base."
        ),
        "tags": ["demo", "ml", "postgresql", "ontology", "lineage"],
        "definition": {
            "steps": [
                {
                    "id": "extract_crm_customers",
                    "step_type": "db_extract",
                    "config": {
                        "datasource_id": datasource_id,
                        "query": (
                            "SELECT customer_id, customer_email, full_name, country_code, "
                            "segment, plan_tier, monthly_revenue_usd, account_status, "
                            "marketing_source, signup_date, last_contract_renewal "
                            "FROM ml_demo.crm_customers ORDER BY customer_id"
                        ),
                        "schema_table": "crm_customers",
                        "include_schema": True,
                    },
                },
                {
                    "id": "normalize_customer_master_fields",
                    "step_type": "field_transformer",
                    "depends_on": ["extract_crm_customers"],
                    "config": {
                        "transformations": [
                            {
                                "field": "customer_email",
                                "operations": [{"type": "TRIM"}, {"type": "LOWER"}],
                            },
                            {
                                "field": "full_name",
                                "operations": [{"type": "TRIM"}],
                            },
                            {
                                "field": "country_code",
                                "operations": [{"type": "TRIM"}, {"type": "UPPER"}],
                            },
                            {
                                "field": "segment",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "UNKNOWN_SEGMENT"},
                                    {"type": "TRIM"},
                                    {"type": "UPPER"},
                                ],
                            },
                            {
                                "field": "plan_tier",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "STANDARD"},
                                    {"type": "TRIM"},
                                    {"type": "UPPER"},
                                ],
                            },
                            {
                                "field": "account_status",
                                "operations": [{"type": "TRIM"}, {"type": "UPPER"}],
                            },
                            {
                                "field": "marketing_source",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "unknown"},
                                    {"type": "TRIM"},
                                    {"type": "LOWER"},
                                ],
                            },
                            {
                                "field": "monthly_revenue_usd",
                                "operations": [{"type": "ROUND", "decimals": 2}],
                            },
                        ]
                    },
                },
                {
                    "id": "validate_customer_master",
                    "step_type": "data_validator",
                    "depends_on": ["normalize_customer_master_fields"],
                    "config": {
                        "rules": [
                            {
                                "field": "customer_id",
                                "rule_type": "NOT_NULL",
                                "severity": "error",
                            },
                            {
                                "field": "customer_email",
                                "rule_type": {
                                    "REGEX": {
                                        "pattern": "^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$"
                                    }
                                },
                                "severity": "error",
                            },
                            {
                                "field": "monthly_revenue_usd",
                                "rule_type": {"RANGE": {"min": 0, "max": 100000}},
                                "severity": "error",
                            },
                        ],
                        "fail_on_error": True,
                    },
                },
                {
                    "id": "deduplicate_customer_master",
                    "step_type": "deduplicator",
                    "depends_on": ["validate_customer_master"],
                    "config": {
                        "method": "exact",
                        "key_fields": ["customer_email"],
                        "keep": "first",
                    },
                },
                {
                    "id": "load_customer_master_curated",
                    "step_type": "db_loader",
                    "depends_on": ["deduplicate_customer_master"],
                    "config": {
                        "datasource_id": datasource_id,
                        "table_name": "ml_demo.customer_master_curated",
                        "mode": "replace",
                        "batch_size": 1000,
                        "create_table": False,
                    },
                },
                {
                    "id": "align_customer_master_to_ontology",
                    "step_type": "semantic_mapper",
                    "depends_on": ["load_customer_master_curated"],
                    "config": {
                        "target_ontology": [
                            "http://arcxa.dev/demo/ml#CustomerMasterRecord"
                        ],
                        "mapping_mode": "hybrid",
                        "preserve_original_fields": True,
                        "source_id": datasource_id,
                        "table_name": "ml_demo.customer_master_curated",
                    },
                },
            ],
            "fusion_threshold": 0.8,
            "fallback": "manual_review",
        },
    }


def build_support_workflow(datasource_id: str) -> Dict[str, Any]:
    return {
        "id": SUPPORT_WORKFLOW_ID,
        "name": "ML Demo Support Signal Curation",
        "description": (
            "Read support tickets from CSV, standardize them, repair sparse ticket fields, "
            "validate the signal quality, align them to the customer-support ontology, "
            "and aggregate support features."
        ),
        "tags": ["demo", "ml", "csv", "ontology", "lineage"],
        "definition": {
            "steps": [
                {
                    "id": "extract_support_csv",
                    "step_type": "csv_source",
                    "config": {
                        "file_path": "/demo-data/support_tickets.csv",
                        "delimiter": ",",
                        "has_header": True,
                    },
                },
                {
                    "id": "normalize_support_fields",
                    "step_type": "field_transformer",
                    "depends_on": ["extract_support_csv"],
                    "config": {
                        "transformations": [
                            {
                                "field": "customer_email",
                                "operations": [{"type": "TRIM"}, {"type": "LOWER"}],
                            },
                            {
                                "field": "ticket_status",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "OPEN"},
                                    {"type": "TRIM"},
                                    {"type": "UPPER"},
                                ],
                            },
                            {
                                "field": "priority",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "MEDIUM"},
                                    {"type": "TRIM"},
                                    {"type": "UPPER"},
                                ],
                            },
                            {
                                "field": "csat_score",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "3.5"},
                                    {"type": "ROUND", "decimals": 2},
                                ],
                            },
                        ]
                    },
                },
                {
                    "id": "validate_support_rows",
                    "step_type": "data_validator",
                    "depends_on": ["normalize_support_fields"],
                    "config": {
                        "rules": [
                            {
                                "field": "ticket_id",
                                "rule_type": "NOT_NULL",
                                "severity": "error",
                            },
                            {
                                "field": "customer_email",
                                "rule_type": {
                                    "REGEX": {
                                        "pattern": "^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$"
                                    }
                                },
                                "severity": "error",
                            },
                            {
                                "field": "csat_score",
                                "rule_type": {"RANGE": {"min": 0, "max": 5}},
                                "severity": "error",
                            },
                        ],
                        "fail_on_error": True,
                    },
                },
                {
                    "id": "deduplicate_support_rows",
                    "step_type": "deduplicator",
                    "depends_on": ["validate_support_rows"],
                    "config": {
                        "method": "exact",
                        "key_fields": ["ticket_id"],
                        "keep": "first",
                    },
                },
                {
                    "id": "aggregate_support_features",
                    "step_type": "aggregator",
                    "depends_on": ["deduplicate_support_rows"],
                    "config": {
                        "group_by": ["customer_email"],
                        "aggregations": [
                            {
                                "field": "csat_score",
                                "function": "COUNT",
                                "alias": "ticket_count_90d",
                            },
                            {
                                "field": "csat_score",
                                "function": "AVG",
                                "alias": "avg_csat_90d",
                            },
                        ],
                    },
                },
                {
                    "id": "load_support_features",
                    "step_type": "db_loader",
                    "depends_on": ["aggregate_support_features"],
                    "config": {
                        "datasource_id": datasource_id,
                        "table_name": "ml_demo.customer_support_features",
                        "mode": "replace",
                        "batch_size": 1000,
                        "create_table": False,
                    },
                },
                {
                    "id": "align_support_signals_to_ontology",
                    "step_type": "semantic_mapper",
                    "depends_on": ["load_support_features"],
                    "config": {
                        "target_ontology": [
                            "http://arcxa.dev/demo/ml#CustomerSupportSignal"
                        ],
                        "mapping_mode": "hybrid",
                        "preserve_original_fields": True,
                        "source_id": datasource_id,
                        "table_name": "ml_demo.customer_support_features",
                    },
                },
            ],
            "fusion_threshold": 0.8,
            "fallback": "manual_review",
        },
    }


def build_usage_workflow(datasource_id: str) -> Dict[str, Any]:
    return {
        "id": USAGE_WORKFLOW_ID,
        "name": "ML Demo Product Usage Curation",
        "description": (
            "Load product usage signals from Parquet, validate and normalize them, "
            "repair sparse numeric usage fields, align them to the customer-usage "
            "ontology, and aggregate them into ML features."
        ),
        "tags": ["demo", "ml", "parquet", "ontology", "lineage"],
        "definition": {
            "steps": [
                {
                    "id": "normalize_usage_fields",
                    "step_type": "field_transformer",
                    "config": {
                        "transformations": [
                            {
                                "field": "customer_email",
                                "operations": [{"type": "TRIM"}, {"type": "LOWER"}],
                            },
                            {
                                "field": "feature_adoption_score",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "0"},
                                    {"type": "ROUND", "decimals": 2},
                                ],
                            },
                            {
                                "field": "active_days_30d",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "0"},
                                    {"type": "ROUND", "decimals": 0},
                                ],
                            },
                            {
                                "field": "product_events_30d",
                                "operations": [
                                    {"type": "IF_NULL", "default_value": "0"},
                                    {"type": "ROUND", "decimals": 0},
                                ],
                            },
                        ]
                    },
                },
                {
                    "id": "validate_usage_rows",
                    "step_type": "data_validator",
                    "depends_on": ["normalize_usage_fields"],
                    "config": {
                        "rules": [
                            {
                                "field": "usage_record_id",
                                "rule_type": "NOT_NULL",
                                "severity": "error",
                            },
                            {
                                "field": "customer_email",
                                "rule_type": {
                                    "REGEX": {
                                        "pattern": "^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$"
                                    }
                                },
                                "severity": "error",
                            },
                            {
                                "field": "active_days_30d",
                                "rule_type": {"RANGE": {"min": 0, "max": 31}},
                                "severity": "error",
                            },
                            {
                                "field": "feature_adoption_score",
                                "rule_type": {"RANGE": {"min": 0, "max": 1}},
                                "severity": "error",
                            },
                        ],
                        "fail_on_error": True,
                    },
                },
                {
                    "id": "deduplicate_usage_rows",
                    "step_type": "deduplicator",
                    "depends_on": ["validate_usage_rows"],
                    "config": {
                        "method": "exact",
                        "key_fields": ["usage_record_id"],
                        "keep": "first",
                    },
                },
                {
                    "id": "aggregate_usage_features",
                    "step_type": "aggregator",
                    "depends_on": ["deduplicate_usage_rows"],
                    "config": {
                        "group_by": ["customer_email"],
                        "aggregations": [
                            {
                                "field": "active_days_30d",
                                "function": "AVG",
                                "alias": "avg_active_days_30d",
                            },
                            {
                                "field": "product_events_30d",
                                "function": "SUM",
                                "alias": "total_product_events_30d",
                            },
                            {
                                "field": "feature_adoption_score",
                                "function": "MAX",
                                "alias": "feature_adoption_score",
                            },
                        ],
                    },
                },
                {
                    "id": "load_usage_features",
                    "step_type": "db_loader",
                    "depends_on": ["aggregate_usage_features"],
                    "config": {
                        "datasource_id": datasource_id,
                        "table_name": "ml_demo.customer_usage_features",
                        "mode": "replace",
                        "batch_size": 1000,
                        "create_table": False,
                    },
                },
                {
                    "id": "align_usage_signals_to_ontology",
                    "step_type": "semantic_mapper",
                    "depends_on": ["load_usage_features"],
                    "config": {
                        "target_ontology": [
                            "http://arcxa.dev/demo/ml#CustomerProductUsageSignal"
                        ],
                        "mapping_mode": "hybrid",
                        "preserve_original_fields": True,
                        "source_id": datasource_id,
                        "table_name": "ml_demo.customer_usage_features",
                    },
                },
            ],
            "fusion_threshold": 0.8,
            "fallback": "manual_review",
        },
    }


def build_feature_workflow(datasource_id: str) -> Dict[str, Any]:
    return {
        "id": FEATURE_WORKFLOW_ID,
        "name": "ML Demo Feature Assembly",
        "description": (
            "Join curated customer, support, and product-usage signals into a model-ready "
            "feature table with ML eligibility flags, a churn label, and full row-level lineage."
        ),
        "tags": ["demo", "ml", "features", "postgresql", "lineage"],
        "definition": {
            "steps": [
                {
                    "id": "assemble_feature_rows",
                    "step_type": "db_extract",
                    "config": {
                        "datasource_id": datasource_id,
                        "query": (
                            "SELECT "
                            "c.customer_id, c.customer_email, c.full_name, c.country_code, "
                            "c.segment, c.plan_tier, c.monthly_revenue_usd, "
                            "COALESCE(s.ticket_count_90d, 0) AS ticket_count_90d, "
                            "COALESCE(s.avg_csat_90d, 0.0) AS avg_csat_90d, "
                            "COALESCE(u.avg_active_days_30d, 0) AS avg_active_days_30d, "
                            "COALESCE(u.total_product_events_30d, 0) AS total_product_events_30d, "
                            "COALESCE(u.feature_adoption_score, 0) AS feature_adoption_score, "
                            "CASE WHEN s.customer_email IS NULL THEN 0 ELSE 1 END AS support_signal_available, "
                            "CASE WHEN u.customer_email IS NULL THEN 0 ELSE 1 END AS usage_signal_available, "
                            "CASE WHEN s.customer_email IS NOT NULL AND u.customer_email IS NOT NULL "
                            "THEN 1 ELSE 0 END AS ml_sample_usable, "
                            "CASE "
                            "WHEN c.account_status = 'AT_RISK' THEN 1 "
                            "WHEN COALESCE(u.avg_active_days_30d, 0) < 8 THEN 1 "
                            "WHEN COALESCE(s.ticket_count_90d, 0) >= 3 "
                            "  AND COALESCE(s.avg_csat_90d, 0.0) < 4 THEN 1 "
                            "ELSE 0 END AS churn_label "
                            "FROM ml_demo.customer_master_curated c "
                            "LEFT JOIN ml_demo.customer_support_features s "
                            "  ON s.customer_email = c.customer_email "
                            "LEFT JOIN ml_demo.customer_usage_features u "
                            "  ON u.customer_email = c.customer_email "
                            "ORDER BY c.customer_id"
                        ),
                        "schema_table": "ml_demo.customer_training_features",
                        "include_schema": True,
                    },
                },
                {
                    "id": "round_feature_metrics",
                    "step_type": "field_transformer",
                    "depends_on": ["assemble_feature_rows"],
                    "config": {
                        "transformations": [
                            {
                                "field": "monthly_revenue_usd",
                                "operations": [{"type": "ROUND", "decimals": 2}],
                            },
                            {
                                "field": "avg_csat_90d",
                                "operations": [{"type": "ROUND", "decimals": 2}],
                            },
                            {
                                "field": "avg_active_days_30d",
                                "operations": [{"type": "ROUND", "decimals": 2}],
                            },
                            {
                                "field": "feature_adoption_score",
                                "operations": [{"type": "ROUND", "decimals": 2}],
                            },
                        ]
                    },
                },
                {
                    "id": "validate_training_features",
                    "step_type": "data_validator",
                    "depends_on": ["round_feature_metrics"],
                    "config": {
                        "rules": [
                            {
                                "field": "customer_email",
                                "rule_type": {
                                    "REGEX": {
                                        "pattern": "^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$"
                                    }
                                },
                                "severity": "error",
                            },
                            {
                                "field": "monthly_revenue_usd",
                                "rule_type": {"RANGE": {"min": 0, "max": 100000}},
                                "severity": "error",
                            },
                            {
                                "field": "support_signal_available",
                                "rule_type": {"RANGE": {"min": 0, "max": 1}},
                                "severity": "error",
                            },
                            {
                                "field": "usage_signal_available",
                                "rule_type": {"RANGE": {"min": 0, "max": 1}},
                                "severity": "error",
                            },
                            {
                                "field": "ml_sample_usable",
                                "rule_type": {"RANGE": {"min": 0, "max": 1}},
                                "severity": "error",
                            },
                            {
                                "field": "churn_label",
                                "rule_type": {"RANGE": {"min": 0, "max": 1}},
                                "severity": "error",
                            },
                        ],
                        "fail_on_error": True,
                    },
                },
                {
                    "id": "load_training_features",
                    "step_type": "db_loader",
                    "depends_on": ["validate_training_features"],
                    "config": {
                        "datasource_id": datasource_id,
                        "table_name": "ml_demo.customer_training_features",
                        "mode": "replace",
                        "batch_size": 1000,
                        "create_table": False,
                    },
                },
                {
                    "id": "align_training_features_to_ontology",
                    "step_type": "semantic_mapper",
                    "depends_on": ["load_training_features"],
                    "config": {
                        "target_ontology": [
                            "http://arcxa.dev/demo/ml#CustomerTrainingFeatureVector"
                        ],
                        "mapping_mode": "hybrid",
                        "preserve_original_fields": True,
                        "source_id": datasource_id,
                        "table_name": "ml_demo.customer_training_features",
                    },
                },
            ],
            "fusion_threshold": 0.8,
            "fallback": "manual_review",
        },
    }


def validate_workflow_definition(token: str, definition: Dict[str, Any]) -> Dict[str, Any]:
    return request_json(
        "POST",
        "/api/v1/workflows/validate",
        definition,
        token=token,
        timeout_seconds=300,
    )


def ensure_workflow(token: str, workflow_request: Dict[str, Any]) -> Dict[str, Any]:
    workflow_id = workflow_request["id"]
    validation = validate_workflow_definition(token, workflow_request["definition"])
    if not validation.get("valid"):
        raise RuntimeError(f"Workflow validation failed for {workflow_id}: {validation}")

    existing = request_json(
        "GET",
        f"/api/v1/workflows/{workflow_id}/details",
        token=token,
        expected=(200, 404),
        timeout_seconds=180,
    )
    if isinstance(existing, dict) and existing.get("workflow_id") == workflow_id:
        request_json(
            "PUT",
            f"/api/v1/workflows/{workflow_id}",
            workflow_request,
            token=token,
            expected=(200,),
            timeout_seconds=180,
        )
        log(f"Updated workflow '{workflow_request['name']}' ({workflow_id})")
    else:
        request_json(
            "POST",
            "/api/v1/workflows",
            workflow_request,
            token=token,
            expected=(200, 201),
            timeout_seconds=180,
        )
        log(f"Created workflow '{workflow_request['name']}' ({workflow_id})")

    return request_json(
        "GET",
        f"/api/v1/workflows/{workflow_id}/details",
        token=token,
        timeout_seconds=180,
    )


def write_summary(summary: Dict[str, Any]) -> None:
    SUMMARY_PATH.parent.mkdir(parents=True, exist_ok=True)
    SUMMARY_PATH.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    log(f"Wrote bootstrap summary to {SUMMARY_PATH}")


def main() -> None:
    wait_for_health()
    token = ensure_admin_login()

    datasource, datasource_test, datasource_created = ensure_datasource(token)
    datasource_id = datasource_id_of(datasource)
    raw_count = query_datasource(
        token,
        datasource_id,
        "SELECT COUNT(*) AS customer_count FROM ml_demo.crm_customers",
        limit=1,
    )

    dataset, dataset_created = ensure_parquet_dataset(token)
    dataset_id = dataset.get("id") or dataset.get("dataset_id")
    if not dataset_id:
        raise RuntimeError(f"Dataset id missing from dataset summary: {dataset}")

    workflows = {
        CUSTOMER_WORKFLOW_ID: ensure_workflow(
            token, build_customer_master_workflow(datasource_id)
        ),
        SUPPORT_WORKFLOW_ID: ensure_workflow(token, build_support_workflow(datasource_id)),
        USAGE_WORKFLOW_ID: ensure_workflow(token, build_usage_workflow(datasource_id)),
        FEATURE_WORKFLOW_ID: ensure_workflow(token, build_feature_workflow(datasource_id)),
    }

    summary = {
        "coordinator_url": COORDINATOR_URL,
        "admin_username": ADMIN_USERNAME,
        "postgres_datasource_id": datasource_id,
        "postgres_datasource_title": POSTGRES_DATASOURCE_TITLE,
        "postgres_datasource_created": datasource_created,
        "postgres_test_success": datasource_test.get("success"),
        "postgres_raw_customer_count": raw_count.get("rows", [{}])[0].get("customer_count"),
        "parquet_dataset_id": dataset_id,
        "parquet_dataset_name": PARQUET_DATASET_NAME,
        "parquet_dataset_created": dataset_created,
        "workflow_ids": list(workflows.keys()),
        "workflow_names": {
            workflow_id: workflow.get("name") for workflow_id, workflow in workflows.items()
        },
        "source_files": {
            "csv_support_tickets": "/demo-data/support_tickets.csv",
            "parquet_product_usage": str(PARQUET_DATASET_FILE),
        },
    }
    write_summary(summary)


if __name__ == "__main__":
    main()
