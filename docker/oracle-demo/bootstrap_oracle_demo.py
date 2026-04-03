#!/usr/bin/env python3
"""Bootstrap an authenticated Oracle + DB2 demo with a visible ETL workflow."""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
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
        "/coordinator-data/bootstrap/oracle-demo-bootstrap-summary.json",
    )
)

ORACLE_DATASOURCE_TITLE = os.environ.get(
    "ORACLE_DATASOURCE_TITLE", "oracle-demo-freepdb1"
)
ORACLE_DATASOURCE_SECRET_REF = os.environ.get(
    "ORACLE_DATASOURCE_SECRET_REF",
    "vault://datasources/oracle-demo-freepdb1/credentials",
)
ORACLE_HOST = os.environ.get("ORACLE_HOST", "oracle")
ORACLE_PORT = int(os.environ.get("ORACLE_PORT", "1521"))
ORACLE_SERVICE_NAME = os.environ.get("ORACLE_SERVICE_NAME", "FREEPDB1")
ORACLE_SCHEMA = os.environ.get("ORACLE_SCHEMA", "ARCXA_DEMO")
ORACLE_USERNAME = os.environ.get("ORACLE_USERNAME", "ARCXA_DEMO")
ORACLE_PASSWORD = os.environ.get("ORACLE_PASSWORD", "arcxa_demo")
ORACLE_ODBC_DRIVER = os.environ.get("ORACLE_ODBC_DRIVER", "Oracle in OraClient19Home1")
INFER_TABLE_NAME = os.environ.get("INFER_TABLE_NAME", "CUSTOMER_FEED")

DB2_DATASOURCE_TITLE = os.environ.get("DB2_DATASOURCE_TITLE", "db2-demo-graphica")
DB2_DATASOURCE_SECRET_REF = os.environ.get(
    "DB2_DATASOURCE_SECRET_REF", "vault://datasources/db2-demo-graphica/credentials"
)
DB2_HOST = os.environ.get("DB2_HOST", "db2-server")
DB2_PORT = int(os.environ.get("DB2_PORT", "50000"))
DB2_DATABASE = os.environ.get("DB2_DATABASE", "GRAPHICA")
DB2_SCHEMA = os.environ.get("DB2_SCHEMA", "DB2INST1")
DB2_USERNAME = os.environ.get("DB2_USERNAME", "db2inst1")
DB2_PASSWORD = os.environ.get("DB2_PASSWORD", "graphica-db2-pass")
DB2_TARGET_TABLE = os.environ.get("DB2_TARGET_TABLE", "CUSTOMER_FEED_CURATED")

WORKFLOW_ID = os.environ.get("WORKFLOW_ID", "oracle-demo-customer-feed-to-db2")
WORKFLOW_NAME = os.environ.get("WORKFLOW_NAME", "Oracle Demo Customer Feed to DB2")


def db2_target_table_name(table_name: str) -> str:
    return table_name.split(".")[-1].strip()


DB2_TARGET_TABLE_NAME = db2_target_table_name(DB2_TARGET_TABLE)
DB2_TARGET_TABLE_QUALIFIED = f"{DB2_SCHEMA}.{DB2_TARGET_TABLE_NAME}"


def log(message: str) -> None:
    print(f"[oracle-demo-bootstrap] {message}", flush=True)


def request_json(
    method: str,
    path: str,
    payload: Optional[Dict[str, Any]] = None,
    token: Optional[str] = None,
    expected: tuple[int, ...] = (200,),
    timeout_seconds: int = 60,
) -> Any:
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    request = urllib.request.Request(
        f"{COORDINATOR_URL}{path}",
        data=body,
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


def secret_ref_to_store_path(secret_ref: str) -> str:
    if "://" in secret_ref:
        _, path = secret_ref.split("://", 1)
        return path.lstrip("/")
    return secret_ref.lstrip("/")


def put_secret(
    token: str,
    *,
    secret_ref: str,
    username: str,
    password: str,
    description: str,
) -> None:
    store_path = secret_ref_to_store_path(secret_ref)
    encoded_path = urllib.parse.quote(store_path, safe="")
    payload = {
        "value": {"username": username, "password": password},
        "description": description,
        "store": "default",
    }
    request_json(
        "PUT",
        f"/api/v1/secrets/{encoded_path}",
        payload,
        token=token,
        expected=(200, 201),
    )


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


def infer_schema(token: str, datasource_id: str, table_name: str) -> Dict[str, Any]:
    return request_json(
        "POST",
        f"/api/v1/datasources/{datasource_id}/schema/infer",
        {"sourceId": datasource_id, "tableName": table_name, "sampleSize": 1000},
        token=token,
        timeout_seconds=180,
    )


def try_infer_schema(
    token: str,
    datasource_id: str,
    table_name: str,
) -> Optional[Dict[str, Any]]:
    try:
        return infer_schema(token, datasource_id, table_name)
    except Exception as exc:
        log(f"Schema inference skipped for {datasource_id} / {table_name}: {exc}")
        return None


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


def execute_datasource_statement(
    token: str,
    datasource_id: str,
    query: str,
    *,
    acceptable_error_substrings: tuple[str, ...] = (),
) -> Dict[str, Any]:
    response = request_json(
        "POST",
        f"/api/v1/datasources/{datasource_id}/query",
        {"sourceId": datasource_id, "query": query, "limit": 1},
        token=token,
        expected=(200, 500),
    )

    if isinstance(response, dict):
        error_text = response.get("error") or response.get("message")
        if error_text:
            if any(fragment in error_text for fragment in acceptable_error_substrings):
                return response
            raise RuntimeError(
                f"Datasource statement failed for {datasource_id}: {error_text}"
            )

    return response


def execute_ddl(
    token: str,
    *,
    db_type: str,
    host: str,
    port: int,
    database: str,
    username: str,
    password: str,
    ddl_statements: list[str],
    options: Optional[Dict[str, str]] = None,
) -> Dict[str, Any]:
    payload = {
        "ddl_statements": ddl_statements,
        "database_config": {
            "db_type": db_type,
            "host": host,
            "port": port,
            "database": database,
            "username": username,
            "password": password,
            "options": options or {},
        },
        "transactional": True,
        "continue_on_error": False,
    }
    return request_json(
        "POST",
        "/api/v1/ddl/execute",
        payload,
        token=token,
        timeout_seconds=180,
    )


def build_oracle_datasource_payload() -> Dict[str, Any]:
    return {
        "title": ORACLE_DATASOURCE_TITLE,
        "sourceType": "Oracle",
        "connection": {
            "secretRef": ORACLE_DATASOURCE_SECRET_REF,
            "config": {
                "type": "Oracle",
                "host": ORACLE_HOST,
                "port": ORACLE_PORT,
                "serviceName": ORACLE_SERVICE_NAME,
                "schema": ORACLE_SCHEMA,
            },
            "encryptionEnabled": False,
        },
        "tags": ["demo", "oracle", "docker-compose"],
        "metadata": {
            "owner": "oracle-demo-compose",
            "fixture": "oracle-demo",
            "odbc_driver": ORACLE_ODBC_DRIVER,
        },
    }


def build_db2_datasource_payload() -> Dict[str, Any]:
    return {
        "title": DB2_DATASOURCE_TITLE,
        "sourceType": "DB2",
        "connection": {
            "secretRef": DB2_DATASOURCE_SECRET_REF,
            "config": {
                "type": "DB2",
                "host": DB2_HOST,
                "port": DB2_PORT,
                "database": DB2_DATABASE,
                "schema": DB2_SCHEMA,
            },
            "encryptionEnabled": False,
        },
        "tags": ["demo", "db2", "docker-compose"],
        "metadata": {
            "owner": "oracle-demo-compose",
            "fixture": "oracle-demo",
        },
    }


def ensure_datasource(
    token: str,
    *,
    title: str,
    secret_ref: str,
    username: str,
    password: str,
    create_payload: Dict[str, Any],
) -> tuple[Dict[str, Any], Dict[str, Any], bool]:
    put_secret(
        token,
        secret_ref=secret_ref,
        username=username,
        password=password,
        description=f"Credentials for {title}",
    )

    datasource = find_existing_datasource(token, title)
    if datasource is not None:
        datasource_id = datasource_id_of(datasource)
        try:
            test_result = test_datasource(token, datasource_id)
            current_datasource = get_datasource(token, datasource_id)
            log(f"Reused existing datasource '{title}' ({datasource_id})")
            return current_datasource, test_result, False
        except Exception as exc:
            log(f"Existing datasource '{title}' failed validation, recreating it: {exc}")
            delete_datasource(token, datasource_id)

    datasource = request_json("POST", "/api/v1/datasources", create_payload, token=token)
    datasource_id = datasource_id_of(datasource)
    test_result = test_datasource(token, datasource_id)
    current_datasource = get_datasource(token, datasource_id)
    log(f"Created datasource '{title}' ({datasource_id})")
    return current_datasource, test_result, True


def build_workflow_request(oracle_datasource_id: str, db2_datasource_id: str) -> Dict[str, Any]:
    return {
        "id": WORKFLOW_ID,
        "name": WORKFLOW_NAME,
        "description": (
            "Extract CUSTOMER_FEED from Oracle, normalize customer fields, "
            "deduplicate on normalized email, and load the curated result into DB2."
        ),
        "tags": ["demo", "oracle", "db2", "frontend", "lineage"],
        "definition": {
            "steps": [
                {
                    "id": "extract_customer_feed",
                    "step_type": "db_extract",
                    "config": {
                        "datasource_id": oracle_datasource_id,
                        "query": (
                            "SELECT "
                            "STAGE_ROW_ID, CUSTOMER_CODE, FULL_NAME, EMAIL, SEGMENT, STATUS, "
                            "COUNTRY_CODE, LIFETIME_VALUE, UPDATED_AT "
                            "FROM ARCXA_DEMO.CUSTOMER_FEED "
                            "ORDER BY UPDATED_AT, STAGE_ROW_ID"
                        ),
                        "schema_table": INFER_TABLE_NAME,
                        "batch_size": 1000,
                        "include_schema": True,
                    },
                },
                {
                    "id": "normalize_customer_feed",
                    "step_type": "field_transformer",
                    "depends_on": ["extract_customer_feed"],
                    "config": {
                        "transformations": [
                            {
                                "field": "CUSTOMER_CODE",
                                "operations": [{"type": "TRIM"}, {"type": "UPPER"}],
                            },
                            {
                                "field": "FULL_NAME",
                                "operations": [{"type": "TRIM"}],
                            },
                            {
                                "field": "EMAIL",
                                "operations": [{"type": "TRIM"}, {"type": "LOWER"}],
                            },
                            {
                                "field": "SEGMENT",
                                "operations": [{"type": "TRIM"}, {"type": "UPPER"}],
                            },
                            {
                                "field": "STATUS",
                                "operations": [{"type": "TRIM"}, {"type": "UPPER"}],
                            },
                            {
                                "field": "COUNTRY_CODE",
                                "operations": [{"type": "TRIM"}, {"type": "UPPER"}],
                            },
                            {
                                "field": "LIFETIME_VALUE",
                                "operations": [{"type": "ROUND", "decimals": 2}],
                            },
                        ]
                    },
                },
                {
                    "id": "validate_customer_feed",
                    "step_type": "data_validator",
                    "depends_on": ["normalize_customer_feed"],
                    "config": {
                        "rules": [
                            {
                                "field": "CUSTOMER_CODE",
                                "rule_type": "NOT_NULL",
                                "severity": "error",
                            },
                            {
                                "field": "EMAIL",
                                "rule_type": {
                                    "REGEX": {
                                        "pattern": "^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$"
                                    }
                                },
                                "severity": "error",
                            },
                        ],
                        "fail_on_error": True,
                    },
                },
                {
                    "id": "deduplicate_customer_feed",
                    "step_type": "deduplicator",
                    "depends_on": ["validate_customer_feed"],
                    "config": {
                        "method": "exact",
                        "key_fields": ["EMAIL"],
                        "keep": "first",
                    },
                },
                {
                    "id": "load_customer_feed_curated",
                    "step_type": "db_loader",
                    "depends_on": ["deduplicate_customer_feed"],
                    "config": {
                        "datasource_id": db2_datasource_id,
                        "table_name": DB2_TARGET_TABLE_NAME,
                        "mode": "replace",
                        "batch_size": 1000,
                        "create_table": False,
                    },
                },
            ],
            "fusion_threshold": 0.75,
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


def ensure_workflow(
    token: str,
    *,
    oracle_datasource_id: str,
    db2_datasource_id: str,
) -> Dict[str, Any]:
    workflow_request = build_workflow_request(oracle_datasource_id, db2_datasource_id)
    validation = validate_workflow_definition(token, workflow_request["definition"])
    if not validation.get("valid"):
        raise RuntimeError(f"Workflow validation failed: {validation}")

    existing = request_json(
        "GET",
        f"/api/v1/workflows/{WORKFLOW_ID}/details",
        token=token,
        expected=(200, 404),
    )
    if isinstance(existing, dict) and existing.get("workflow_id") == WORKFLOW_ID:
        request_json(
            "PUT",
            f"/api/v1/workflows/{WORKFLOW_ID}",
            workflow_request,
            token=token,
            expected=(200,),
            timeout_seconds=180,
        )
        log(f"Updated workflow '{WORKFLOW_NAME}' ({WORKFLOW_ID})")
    else:
        request_json(
            "POST",
            "/api/v1/workflows",
            workflow_request,
            token=token,
            expected=(200, 201),
            timeout_seconds=180,
        )
        log(f"Created workflow '{WORKFLOW_NAME}' ({WORKFLOW_ID})")

    return request_json(
        "GET",
        f"/api/v1/workflows/{WORKFLOW_ID}/details",
        token=token,
        timeout_seconds=180,
    )


def write_summary(summary: Dict[str, Any]) -> None:
    SUMMARY_PATH.parent.mkdir(parents=True, exist_ok=True)
    SUMMARY_PATH.write_text(json.dumps(summary, indent=2), encoding="utf-8")


def table_columns(schema: Dict[str, Any]) -> list[Dict[str, Any]]:
    first_table = schema.get("tables", [{}])[0]
    return [
        {"name": column.get("name"), "type": column.get("dataType")}
        for column in first_table.get("columns", [])
    ]


def db2_table_exists(token: str, datasource_id: str, qualified_table_name: str) -> bool:
    schema_name, table_name = qualified_table_name.split(".", 1)
    result = query_datasource(
        token,
        datasource_id,
        (
            "SELECT COUNT(*) AS TABLE_COUNT "
            "FROM SYSCAT.TABLES "
            f"WHERE TABSCHEMA = '{schema_name.upper()}' "
            f"AND TABNAME = '{table_name.upper()}'"
        ),
        limit=1,
        timeout_seconds=120,
    )
    rows = result.get("rows", [])
    if not rows:
        return False
    first_row = rows[0]
    raw_count = first_row.get("TABLE_COUNT", first_row.get("table_count", 0))
    return int(raw_count or 0) > 0


def db2_table_columns(
    token: str,
    datasource_id: str,
    qualified_table_name: str,
) -> list[Dict[str, Any]]:
    schema_name, table_name = qualified_table_name.split(".", 1)
    result = query_datasource(
        token,
        datasource_id,
        (
            "SELECT COLNAME, TYPENAME, LENGTH, SCALE, NULLS "
            "FROM SYSCAT.COLUMNS "
            f"WHERE TABSCHEMA = '{schema_name.upper()}' "
            f"AND TABNAME = '{table_name.upper()}' "
            "ORDER BY COLNO"
        ),
        limit=200,
        timeout_seconds=120,
    )
    columns: list[Dict[str, Any]] = []
    for row in result.get("rows", []):
        column_name = row.get("COLNAME") or row.get("colname")
        data_type = row.get("TYPENAME") or row.get("typename")
        if column_name and data_type:
            columns.append({"name": column_name, "type": data_type})
    return columns


def ensure_db2_target_table(token: str, datasource_id: str) -> list[Dict[str, Any]]:
    ddl = f"""
CREATE TABLE {DB2_TARGET_TABLE_QUALIFIED} (
    STAGE_ROW_ID VARCHAR(20) NOT NULL PRIMARY KEY,
    CUSTOMER_CODE VARCHAR(20) NOT NULL,
    FULL_NAME VARCHAR(100) NOT NULL,
    EMAIL VARCHAR(120) NOT NULL,
    SEGMENT VARCHAR(20),
    STATUS VARCHAR(20),
    COUNTRY_CODE VARCHAR(10),
    LIFETIME_VALUE DECIMAL(12,2),
    UPDATED_AT TIMESTAMP NOT NULL
)
""".strip()

    if db2_table_exists(token, datasource_id, DB2_TARGET_TABLE_QUALIFIED):
        return db2_table_columns(token, datasource_id, DB2_TARGET_TABLE_QUALIFIED)

    log(f"Provisioning DB2 target table '{DB2_TARGET_TABLE_QUALIFIED}'")
    execute_ddl(
        token,
        db_type="db2",
        host=DB2_HOST,
        port=DB2_PORT,
        database=DB2_DATABASE,
        username=DB2_USERNAME,
        password=DB2_PASSWORD,
        ddl_statements=[ddl],
    )
    if not db2_table_exists(token, datasource_id, DB2_TARGET_TABLE_QUALIFIED):
        raise RuntimeError(
            f"DB2 target table '{DB2_TARGET_TABLE_QUALIFIED}' is still not discoverable after creation"
        )
    return db2_table_columns(token, datasource_id, DB2_TARGET_TABLE_QUALIFIED)


def main() -> int:
    wait_for_health()
    token = ensure_admin_login()

    oracle_datasource, oracle_test, _ = ensure_datasource(
        token,
        title=ORACLE_DATASOURCE_TITLE,
        secret_ref=ORACLE_DATASOURCE_SECRET_REF,
        username=ORACLE_USERNAME,
        password=ORACLE_PASSWORD,
        create_payload=build_oracle_datasource_payload(),
    )
    oracle_datasource_id = datasource_id_of(oracle_datasource)
    oracle_schema = try_infer_schema(token, oracle_datasource_id, INFER_TABLE_NAME)
    oracle_preview = query_datasource(
        token,
        oracle_datasource_id,
        (
            "SELECT STAGE_ROW_ID, CUSTOMER_CODE, FULL_NAME, EMAIL, STATUS "
            "FROM ARCXA_DEMO.CUSTOMER_FEED ORDER BY STAGE_ROW_ID"
        ),
        limit=10,
        timeout_seconds=180,
    )

    db2_datasource, db2_test, _ = ensure_datasource(
        token,
        title=DB2_DATASOURCE_TITLE,
        secret_ref=DB2_DATASOURCE_SECRET_REF,
        username=DB2_USERNAME,
        password=DB2_PASSWORD,
        create_payload=build_db2_datasource_payload(),
    )
    db2_datasource_id = datasource_id_of(db2_datasource)
    db2_target_columns = ensure_db2_target_table(token, db2_datasource_id)
    workflow = ensure_workflow(
        token,
        oracle_datasource_id=oracle_datasource_id,
        db2_datasource_id=db2_datasource_id,
    )

    oracle_first_table = (oracle_schema or {}).get("tables", [{}])[0]
    summary = {
        "coordinator_url": COORDINATOR_URL,
        "admin_username": ADMIN_USERNAME,
        "oracle_datasource": {
            "id": oracle_datasource_id,
            "title": ORACLE_DATASOURCE_TITLE,
            "status": oracle_datasource.get("status"),
            "tested_at": oracle_test.get("testedAt"),
            "connection_metadata": oracle_test.get("metadata"),
            "schema_name": (oracle_schema or {}).get("name"),
            "inferred_table": oracle_first_table.get("name"),
            "column_count": len(oracle_first_table.get("columns", [])),
            "columns": table_columns(oracle_schema or {"tables": []}),
            "source_row_count": oracle_preview.get("row_count"),
        },
        "db2_datasource": {
            "id": db2_datasource_id,
            "title": DB2_DATASOURCE_TITLE,
            "status": db2_datasource.get("status"),
            "tested_at": db2_test.get("testedAt"),
            "connection_metadata": db2_test.get("metadata"),
            "target_table": DB2_TARGET_TABLE_NAME,
            "target_table_qualified": DB2_TARGET_TABLE_QUALIFIED,
            "target_column_count": len(db2_target_columns),
            "target_columns": db2_target_columns,
        },
        "workflow": {
            "workflow_id": workflow.get("workflow_id"),
            "name": workflow.get("name"),
            "description": workflow.get("description"),
            "tags": workflow.get("tags"),
            "version": workflow.get("version"),
            "execution_count": workflow.get("execution_count"),
            "definition_step_count": len(
                workflow.get("definition", {}).get("steps", [])
            ),
            "target_table": DB2_TARGET_TABLE_NAME,
            "target_table_qualified": DB2_TARGET_TABLE_QUALIFIED,
        },
    }
    write_summary(summary)

    log("Oracle datasource is ready")
    log("DB2 datasource is ready")
    log(f"Workflow '{WORKFLOW_NAME}' is ready in the coordinator and visible to the frontend")
    log(f"Admin login: username='{ADMIN_USERNAME}' password='{ADMIN_PASSWORD}'")
    log(f"Bootstrap summary written to {SUMMARY_PATH}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # pragma: no cover - integration entrypoint
        log(f"Bootstrap failed: {exc}")
        raise
