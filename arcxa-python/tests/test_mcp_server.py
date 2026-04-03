from graphica.errors import GraphicaError
from graphica.auth import BasicAuth, TokenAuth
from graphica.mcp import server as mcp_server_module
from graphica.mcp.server import ArcxaMcpServer, build_client_from_env


class FakeDatasourcesAPI:
    def list(self, **kwargs):
        return {"sources": [], "filters": kwargs}

    def get(self, datasource_id):
        return {"id": datasource_id}

    def create(self, datasource):
        return {"created": datasource}

    def update(self, datasource_id, updates):
        return {"datasource_id": datasource_id, "updated": updates}

    def delete(self, datasource_id):
        return None

    def test_connection(self, **kwargs):
        return {"tested": kwargs}

    def infer_schema(self, **kwargs):
        return {"schema": kwargs}

    def query(self, **kwargs):
        return {"query": kwargs}


class FakeDatasetsAPI:
    def list(self, **kwargs):
        return {"datasets": [], "filters": kwargs}

    def get(self, dataset_id):
        return {"dataset_id": dataset_id}

    def import_file(self, **kwargs):
        return {"imported_file": kwargs}

    def import_from_datasource(self, **kwargs):
        return {"imported_datasource": kwargs}

    def batch_import_from_datasource(self, **kwargs):
        return {"batch_import": kwargs}

    def get_import_status(self, import_id):
        return {"import_id": import_id, "status": "processing"}

    def list_imports(self, **kwargs):
        return {"imports": [], "filters": kwargs}


class FakeWorkflowsAPI:
    def list(self, **kwargs):
        return {"workflows": [], "filters": kwargs}

    def get(self, workflow_id):
        return {"workflow_id": workflow_id}

    def validate(self, workflow):
        return {"valid": True, "workflow": workflow}

    def create(self, workflow):
        return {"created": workflow}

    def delete(self, workflow_id):
        return None

    def execute(self, **kwargs):
        return {"execution": kwargs}

    def list_executions(self, **kwargs):
        return {"executions": [], "filters": kwargs}

    def get_execution(self, execution_id):
        return {"execution_id": execution_id}

    def get_execution_progress(self, execution_id):
        return {"execution_id": execution_id, "status": "running"}

    def list_execution_progress(self, workflow_id):
        return [{"workflow_id": workflow_id, "status": "completed"}]

    def list_active_executions(self):
        return [{"execution_id": "exec_active_1", "status": "running"}]


class FakeMappingAPI:
    def analyze_datasource_for_mapping(self, source_id, **kwargs):
        return {"source_id": source_id, "analysis": kwargs}

    def analyze_dataset_for_mapping(self, dataset_id, **kwargs):
        return {"dataset_id": dataset_id, "analysis": kwargs}

    def get_source_session(self, session_id):
        return {"session_id": session_id, "kind": "source"}

    def review_source_session(self, session_id, **kwargs):
        return {"session_id": session_id, "review": kwargs}

    def apply_source_session(self, session_id, **kwargs):
        return {"session_id": session_id, "apply": kwargs}

    def import_source_session(self, session_id, **kwargs):
        return {"session_id": session_id, "import": kwargs}

    def list_sessions(self, **kwargs):
        return {"sessions": [], "filters": kwargs}

    def suggest(self, datasets):
        return {"datasets": datasets, "suggestions": []}

    def create_session(self, **kwargs):
        return {"session": kwargs}

    def get_session(self, session_id):
        return {"session_id": session_id}

    def update_session(self, session_id, **kwargs):
        return {"session_id": session_id, "updated": kwargs}

    def resolve_conflicts(self, **kwargs):
        return {"resolved": kwargs}

    def load_to_database(self, **kwargs):
        return {"load": kwargs}

    def get_load_job_status(self, job_id):
        return {"job_id": job_id, "status": "completed"}

    def statistics(self):
        return {"total_sessions": 3}


class FakeLineageAPI:
    def get_row(self, row_key):
        return {"row_key": row_key, "events": []}

    def get_run(self, run_id):
        return {"run_id": run_id}

    def get_row_journey(self, row_key):
        return {"row_key": row_key}


class FakeClient:
    def __init__(self):
        self.base_url = "http://localhost:8080"
        self.datasources = FakeDatasourcesAPI()
        self.datasets = FakeDatasetsAPI()
        self.workflows = FakeWorkflowsAPI()
        self.mapping = FakeMappingAPI()
        self.lineage = FakeLineageAPI()

    def health(self):
        return {"status": "ok"}


def test_tools_list_includes_arcxa_workflow_and_datasource_tools():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}
    )

    tool_names = {tool["name"] for tool in response["result"]["tools"]}
    assert "arcxa_list_datasources" in tool_names
    assert "arcxa_list_datasets" in tool_names
    assert "arcxa_create_workflow" in tool_names
    assert "arcxa_plan_data_integration" in tool_names
    assert "arcxa_build_etl_workflow_definition" in tool_names
    assert "arcxa_analyze_datasource_for_mapping" in tool_names
    assert "arcxa_analyze_dataset_for_mapping" in tool_names
    assert "arcxa_create_unified_mapping_session" in tool_names
    assert "arcxa_update_unified_mapping_session" in tool_names


def test_build_etl_workflow_definition_tool_returns_structured_workflow():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "arcxa_build_etl_workflow_definition",
                "arguments": {
                    "name": "oracle-to-db2",
                    "sources": [
                        {
                            "datasource_id": "urn:graphica:datasource:oracle",
                            "table_name": "CUSTOMERS",
                        }
                    ],
                    "field_transformations": [
                        {
                            "field": "customer_name",
                            "operations": [{"type": "TRIM"}],
                        }
                    ],
                    "target": {
                        "kind": "db_loader",
                        "datasource_id": "urn:graphica:datasource:db2",
                        "table_name": "CUSTOMER_DIM",
                    },
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    workflow = response["result"]["structuredContent"]["workflow"]
    assert workflow["name"] == "oracle-to-db2"
    assert workflow["definition"]["steps"][0]["step_type"] == "db_extract"
    assert workflow["definition"]["steps"][-1]["step_type"] == "db_loader"


def test_recommend_execution_surface_prefers_unified_mapping_for_multi_source_join():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "arcxa_recommend_execution_surface",
                "arguments": {
                    "sources": [
                        {"datasource_id": "parquet-source"},
                        {"datasource_id": "oracle-source"},
                    ],
                    "join": {"left_key": ["id"], "right_key": ["id"]},
                    "target": {"kind": "db_loader"},
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    recommendation = response["result"]["structuredContent"]
    assert recommendation["recommended_surface"] == "unified_mapping"
    assert recommendation["safe_for_direct_workflow_execution"] is False


def test_plan_data_integration_returns_multi_source_mapping_plan():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "arcxa_plan_data_integration",
                "arguments": {
                    "name": "oracle-parquet-db2",
                    "sources": [
                        {
                            "kind": "parquet_file",
                            "file_path": "/tmp/customers.parquet",
                            "dataset_name": "customers-file",
                        },
                        {
                            "kind": "oracle_table",
                            "datasource_id": "urn:graphica:datasource:oracle",
                            "table_name": "CUSTOMERS",
                            "dataset_name": "customers-oracle",
                        },
                    ],
                    "join": {"left_key": ["CUSTOMER_ID"], "right_key": ["CUSTOMER_ID"]},
                    "target": {"kind": "d_b2", "connection_config": {"database": "WAREHOUSE"}},
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    content = response["result"]["structuredContent"]
    assert content["recommendation"]["recommended_surface"] == "unified_mapping"
    assert content["unified_mapping_plan"]["dataset_preparation"][0]["tool"] == "arcxa_import_dataset_file"
    assert {
        "arcxa_analyze_datasource_for_mapping",
        "arcxa_analyze_dataset_for_mapping",
    }.issubset(
        {
            step.get("tool")
            for step in content["unified_mapping_plan"]["source_session_preparation"]
        }
    )
    assert (
        content["unified_mapping_plan"]["execution_readiness"]["can_execute_end_to_end_via_mcp"]
        is True
    )
    assert content["unified_mapping_plan"]["mapping_execution"][-1]["tool"] == "arcxa_load_unified_mapping_session"


def test_analyze_dataset_for_mapping_tool_calls_mapping_api():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 4_05,
            "method": "tools/call",
            "params": {
                "name": "arcxa_analyze_dataset_for_mapping",
                "arguments": {
                    "dataset_id": "ds_import_123",
                    "tables": ["customers_file"],
                    "sample_size": 25,
                    "user_id": "agent",
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    content = response["result"]["structuredContent"]
    assert content["dataset_id"] == "ds_import_123"
    assert content["analysis"]["tables"] == ["customers_file"]
    assert content["analysis"]["sample_size"] == 25


def test_analyze_datasource_for_mapping_tool_calls_mapping_api():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 4_1,
            "method": "tools/call",
            "params": {
                "name": "arcxa_analyze_datasource_for_mapping",
                "arguments": {
                    "datasource_id": "urn:graphica:datasource:oracle",
                    "tables": ["CUSTOMERS"],
                    "sample_size": 50,
                    "user_id": "agent",
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    content = response["result"]["structuredContent"]
    assert content["source_id"] == "urn:graphica:datasource:oracle"
    assert content["analysis"]["tables"] == ["CUSTOMERS"]
    assert content["analysis"]["sample_size"] == 50


def test_load_unified_mapping_session_tool_allows_internal_targets_without_connection_config():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 4_2,
            "method": "tools/call",
            "params": {
                "name": "arcxa_load_unified_mapping_session",
                "arguments": {
                    "session_id": "unified_123",
                    "database_type": "postgre_sql",
                    "create_tables": False,
                    "validate_data": True,
                    "batch_size": 250,
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    content = response["result"]["structuredContent"]
    assert content["load"]["session_id"] == "unified_123"
    assert content["load"]["connection_config"] is None
    assert content["load"]["create_tables"] is False


def test_update_unified_mapping_session_tool_calls_mapping_api():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 4_25,
            "method": "tools/call",
            "params": {
                "name": "arcxa_update_unified_mapping_session",
                "arguments": {
                    "session_id": "unified_123",
                    "field_mappings": [
                        {
                            "id": "mapping_manual_001",
                            "source_fields": [
                                {
                                    "session_id": "session_oracle",
                                    "datasource_id": "urn:graphica:datasource:oracle",
                                    "table_name": "CUSTOMER_FEED",
                                    "field_name": "FULL_NAME",
                                    "source_data_type": "VARCHAR2",
                                }
                            ],
                            "ontology_term_uri": "http://schema.org/name",
                            "target_column": {
                                "table_name": "CUSTOMER_DIM",
                                "column_name": "FULL_NAME",
                                "data_type": "VARCHAR(100)",
                            },
                            "conflict_resolution": "no_conflict",
                            "transformation": None,
                            "confidence": 1.0,
                        }
                    ],
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    content = response["result"]["structuredContent"]
    assert content["session_id"] == "unified_123"
    assert content["updated"]["field_mappings"][0]["target_column"]["column_name"] == "FULL_NAME"


def test_prompts_list_and_get_include_oracle_parquet_db2_flow():
    server = ArcxaMcpServer(FakeClient())

    listed = server.handle_request(
        {"jsonrpc": "2.0", "id": 5, "method": "prompts/list", "params": {}}
    )
    prompt_names = {prompt["name"] for prompt in listed["result"]["prompts"]}
    assert "design_oracle_parquet_to_db2" in prompt_names

    prompt = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "prompts/get",
            "params": {
                "name": "design_oracle_parquet_to_db2",
                "arguments": {
                    "oracle_datasource_id": "urn:graphica:datasource:oracle",
                    "oracle_table": "CUSTOMERS",
                    "parquet_file_path": "/tmp/customers.parquet",
                    "db2_datasource_id": "urn:graphica:datasource:db2",
                    "target_table": "CUSTOMER_DIM",
                },
            },
        }
    )

    message = prompt["result"]["messages"][0]["content"]["text"]
    assert "arcxa_analyze_datasource_for_mapping" in message
    assert "arcxa_analyze_dataset_for_mapping" in message
    assert "managed dataset" in message
    assert "arcxa_update_unified_mapping_session" in message
    assert "arcxa_load_unified_mapping_session" in message


def test_create_workflow_from_spec_validates_raw_definition():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "arcxa_create_workflow_from_spec",
                "arguments": {
                    "spec": {
                        "name": "single-source-export",
                        "sources": [
                            {
                                "datasource_id": "urn:graphica:datasource:postgres",
                                "table_name": "public.customers",
                            }
                        ],
                        "target": {
                            "kind": "csv_exporter",
                            "output_path": "/tmp/customers.csv",
                        },
                    },
                    "create": False,
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    validation_input = response["result"]["structuredContent"]["validation"]["workflow"]
    assert "steps" in validation_input
    assert "workflow" not in validation_input


def test_build_etl_workflow_definition_normalizes_fail_fallback():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {
                "name": "arcxa_build_etl_workflow_definition",
                "arguments": {
                    "name": "single-source-export",
                    "sources": [
                        {
                            "datasource_id": "urn:graphica:datasource:postgres",
                            "table_name": "public.customers",
                        }
                    ],
                    "target": {
                        "kind": "csv_exporter",
                        "output_path": "/tmp/customers.csv",
                    },
                    "fallback": "fail",
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    workflow = response["result"]["structuredContent"]["workflow"]
    assert workflow["definition"]["fallback"] == "reject_fusion"


def test_delete_tools_return_structured_confirmation():
    server = ArcxaMcpServer(FakeClient())

    delete_datasource = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "arcxa_delete_datasource",
                "arguments": {"datasource_id": "urn:graphica:datasource:test"},
            },
        }
    )
    delete_workflow = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {
                "name": "arcxa_delete_workflow",
                "arguments": {"workflow_id": "wf_test"},
            },
        }
    )

    assert delete_datasource["result"]["isError"] is False
    assert (
        delete_datasource["result"]["structuredContent"]["datasource_id"]
        == "urn:graphica:datasource:test"
    )
    assert delete_workflow["result"]["structuredContent"] == {
        "deleted": True,
        "workflow_id": "wf_test",
    }


def test_wait_for_execution_returns_terminal_execution_snapshot():
    class FakeTerminalWorkflowsAPI(FakeWorkflowsAPI):
        def __init__(self):
            self.calls = 0

        def get_execution_progress(self, execution_id):
            self.calls += 1
            if self.calls == 1:
                return {"execution_id": execution_id, "status": "running"}
            return {"execution_id": execution_id, "status": "completed"}

        def get_execution(self, execution_id):
            if self.calls == 1:
                return {"execution_id": execution_id, "status": "running"}
            return {"execution_id": execution_id, "status": "completed", "output": {"rows": 3}}

    client = FakeClient()
    client.workflows = FakeTerminalWorkflowsAPI()
    server = ArcxaMcpServer(client)

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {
                "name": "arcxa_wait_for_execution",
                "arguments": {
                    "execution_id": "exec_123",
                    "timeout_seconds": 1,
                    "poll_interval_seconds": 0.001,
                },
            },
        }
    )

    assert response["result"]["isError"] is False
    structured = response["result"]["structuredContent"]
    assert structured["status"] == "completed"
    assert structured["terminal"] is True
    assert structured["timed_out"] is False
    assert structured["execution"]["output"] == {"rows": 3}


def test_progress_tools_return_stable_wrapped_shapes():
    server = ArcxaMcpServer(FakeClient())

    progress_response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 16,
            "method": "tools/call",
            "params": {
                "name": "arcxa_get_execution_progress",
                "arguments": {"execution_id": "exec_123"},
            },
        }
    )
    progress = progress_response["result"]["structuredContent"]
    assert progress["available"] is True
    assert progress["execution_id"] == "exec_123"
    assert progress["status"] == "running"

    list_progress_response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 17,
            "method": "tools/call",
            "params": {
                "name": "arcxa_list_execution_progress",
                "arguments": {"workflow_id": "wf_123"},
            },
        }
    )
    list_progress = list_progress_response["result"]["structuredContent"]
    assert list_progress["available"] is True
    assert list_progress["workflow_id"] == "wf_123"
    assert len(list_progress["entries"]) == 1

    active_response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 18,
            "method": "tools/call",
            "params": {
                "name": "arcxa_list_active_executions",
                "arguments": {},
            },
        }
    )
    active = active_response["result"]["structuredContent"]
    assert active["available"] is True
    assert active["executions"][0]["execution_id"] == "exec_active_1"


def test_progress_tools_degrade_cleanly_when_tracking_is_unavailable():
    class FakeUnavailableProgressWorkflowsAPI(FakeWorkflowsAPI):
        def get_execution_progress(self, execution_id):
            raise GraphicaError("Progress tracking not available")

        def list_execution_progress(self, workflow_id):
            raise GraphicaError("Progress tracking not available")

        def list_active_executions(self):
            raise GraphicaError("Progress tracking not available")

    client = FakeClient()
    client.workflows = FakeUnavailableProgressWorkflowsAPI()
    server = ArcxaMcpServer(client)

    progress_response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 19,
            "method": "tools/call",
            "params": {
                "name": "arcxa_get_execution_progress",
                "arguments": {"execution_id": "exec_123"},
            },
        }
    )
    progress = progress_response["result"]["structuredContent"]
    assert progress["available"] is False
    assert progress["status"] == "unavailable"
    assert progress["execution_id"] == "exec_123"

    list_progress_response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 20,
            "method": "tools/call",
            "params": {
                "name": "arcxa_list_execution_progress",
                "arguments": {"workflow_id": "wf_123"},
            },
        }
    )
    list_progress = list_progress_response["result"]["structuredContent"]
    assert list_progress["available"] is False
    assert list_progress["workflow_id"] == "wf_123"
    assert list_progress["entries"] == []

    active_response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 21,
            "method": "tools/call",
            "params": {
                "name": "arcxa_list_active_executions",
                "arguments": {},
            },
        }
    )
    active = active_response["result"]["structuredContent"]
    assert active["available"] is False
    assert active["executions"] == []


def test_run_lineage_falls_back_to_execution_surface_when_sparql_is_unsupported():
    class FakeUnsupportedRunLineageAPI(FakeLineageAPI):
        def get_run(self, run_id):
            raise GraphicaError(
                'SPARQL query failed: Unsupported SPARQL query: expected one triple pattern'
            )

    client = FakeClient()
    client.lineage = FakeUnsupportedRunLineageAPI()
    server = ArcxaMcpServer(client)

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 22,
            "method": "tools/call",
            "params": {
                "name": "arcxa_get_run_lineage",
                "arguments": {"run_id": "exec_123"},
            },
        }
    )

    lineage = response["result"]["structuredContent"]
    assert lineage["available"] is False
    assert lineage["fallback"] == "workflow_execution"
    assert lineage["execution"]["execution_id"] == "exec_123"


def test_run_lineage_falls_back_to_unified_load_job_when_run_is_not_found():
    class FakeMissingRunLineageAPI(FakeLineageAPI):
        def get_run(self, run_id):
            raise GraphicaError("NotFound")

    client = FakeClient()
    client.lineage = FakeMissingRunLineageAPI()
    server = ArcxaMcpServer(client)

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 22,
            "method": "tools/call",
            "params": {
                "name": "arcxa_get_run_lineage",
                "arguments": {"run_id": "unified_load_loadjob_123"},
            },
        }
    )

    lineage = response["result"]["structuredContent"]
    assert lineage["available"] is False
    assert lineage["fallback"] == "unified_mapping_load_job"
    assert lineage["load_job_id"] == "loadjob_123"
    assert lineage["load_job"]["job_id"] == "loadjob_123"


def test_run_lineage_falls_back_to_execution_surface_when_backend_reports_no_lineage():
    class FakeMissingRunLineageAPI(FakeLineageAPI):
        def get_run(self, run_id):
            raise GraphicaError(f"No lineage found for run: {run_id}")

    client = FakeClient()
    client.lineage = FakeMissingRunLineageAPI()
    server = ArcxaMcpServer(client)

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 23,
            "method": "tools/call",
            "params": {
                "name": "arcxa_get_run_lineage",
                "arguments": {"run_id": "exec_123"},
            },
        }
    )

    lineage = response["result"]["structuredContent"]
    assert lineage["available"] is False
    assert lineage["fallback"] == "workflow_execution"
    assert lineage["execution"]["execution_id"] == "exec_123"


def test_get_row_lineage_tool_returns_structured_row_events():
    server = ArcxaMcpServer(FakeClient())

    response = server.handle_request(
        {
            "jsonrpc": "2.0",
            "id": 23,
            "method": "tools/call",
            "params": {
                "name": "arcxa_get_row_lineage",
                "arguments": {"row_key": "postgres:public.customers:id=1"},
            },
        }
    )

    assert response["result"]["isError"] is False
    assert response["result"]["structuredContent"]["row_key"] == "postgres:public.customers:id=1"


def test_build_client_from_env_uses_token_login_for_username_password(monkeypatch):
    calls = {}

    class FakeResponse:
        status_code = 200
        ok = True

        @staticmethod
        def json():
            return {"token": "jwt-token"}

    def fake_post(url, json, timeout):
        calls["url"] = url
        calls["json"] = json
        calls["timeout"] = timeout
        return FakeResponse()

    monkeypatch.setattr(mcp_server_module.requests, "post", fake_post)

    client = build_client_from_env(
        {
            "ARCXA_BASE_URL": "http://localhost:18898",
            "ARCXA_USERNAME": "admin",
            "ARCXA_PASSWORD": "secret",
            "ARCXA_TIMEOUT": "17",
        }
    )

    assert isinstance(client.auth, TokenAuth)
    assert client.auth.token == "jwt-token"
    assert calls["url"] == "http://localhost:18898/auth/login"
    assert calls["json"] == {"username": "admin", "password": "secret"}
    assert calls["timeout"] == 17


def test_build_client_from_env_strips_api_v1_for_login(monkeypatch):
    calls = {}

    class FakeResponse:
        status_code = 200
        ok = True

        @staticmethod
        def json():
            return {"token": "jwt-token"}

    def fake_post(url, json, timeout):
        calls["url"] = url
        return FakeResponse()

    monkeypatch.setattr(mcp_server_module.requests, "post", fake_post)

    client = build_client_from_env(
        {
            "ARCXA_BASE_URL": "http://localhost:18898/api/v1",
            "ARCXA_USERNAME": "admin",
            "ARCXA_PASSWORD": "secret",
        }
    )

    assert isinstance(client.auth, TokenAuth)
    assert calls["url"] == "http://localhost:18898/auth/login"


def test_build_client_from_env_falls_back_to_basic_auth_for_legacy_servers(monkeypatch):
    class FakeResponse:
        status_code = 405
        ok = False
        text = ""

        @staticmethod
        def json():
            return {}

    monkeypatch.setattr(mcp_server_module.requests, "post", lambda *args, **kwargs: FakeResponse())

    client = build_client_from_env(
        {
            "ARCXA_BASE_URL": "http://legacy.example.com",
            "ARCXA_USERNAME": "admin",
            "ARCXA_PASSWORD": "secret",
        }
    )

    assert isinstance(client.auth, BasicAuth)
