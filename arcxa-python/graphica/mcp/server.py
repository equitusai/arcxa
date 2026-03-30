"""ARCXA MCP server implementation."""

from __future__ import annotations

import json
import logging
import sys
import time
from dataclasses import dataclass
from typing import Any, Callable, Dict, Iterable, Optional

from graphica import BasicAuth, Client, TokenAuth
from graphica.errors import GraphicaError
from graphica.mcp.workflow_builder import (
    build_etl_workflow_definition,
    build_unified_mapping_plan,
    plan_data_integration,
    recommend_execution_surface,
)

logger = logging.getLogger(__name__)

JSON = Dict[str, Any]
ToolHandler = Callable[[Dict[str, Any]], Any]
PromptHandler = Callable[[Dict[str, Any]], Dict[str, Any]]


def _normalize_database_type(raw: str) -> str:
    """Normalize friendly database aliases to the live API enum values."""
    value = (raw or "").strip().lower()
    aliases = {
        "postgresql": "postgre_s_q_l",
        "postgres": "postgre_s_q_l",
        "postgre_sql": "postgre_s_q_l",
        "postgre_s_q_l": "postgre_s_q_l",
        "db2": "d_b2",
        "d_b2": "d_b2",
        "oracle": "oracle",
        "databricks": "databricks",
    }
    if value not in aliases:
        raise ValueError(
            "Unsupported database_type. Use one of: postgresql, db2, oracle, databricks"
        )
    return aliases[value]


@dataclass
class ToolDefinition:
    name: str
    description: str
    input_schema: Dict[str, Any]
    handler: ToolHandler

    def to_mcp(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        }


@dataclass
class PromptDefinition:
    name: str
    description: str
    arguments: Iterable[Dict[str, Any]]
    handler: PromptHandler

    def to_mcp(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "arguments": list(self.arguments),
        }


class ArcxaMcpServer:
    """Minimal MCP server for ARCXA operational surfaces."""

    def __init__(self, client: Client):
        self.client = client
        self.tools = self._build_tools()
        self.prompts = self._build_prompts()

    def _build_tools(self) -> Dict[str, ToolDefinition]:
        return {
            "arcxa_health_check": ToolDefinition(
                name="arcxa_health_check",
                description="Check coordinator health and connectivity.",
                input_schema={"type": "object", "properties": {}, "additionalProperties": False},
                handler=lambda _: self.client.health(),
            ),
            "arcxa_list_datasources": ToolDefinition(
                name="arcxa_list_datasources",
                description="List registered datasources with optional operational filters.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "source_type": {"type": "string"},
                        "status": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "page": {"type": "integer", "minimum": 0},
                        "page_size": {"type": "integer", "minimum": 1},
                    },
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.list(
                    source_type=args.get("source_type"),
                    status=args.get("status"),
                    tags=args.get("tags"),
                    page=args.get("page", 0),
                    page_size=args.get("page_size", 50),
                ),
            ),
            "arcxa_get_datasource": ToolDefinition(
                name="arcxa_get_datasource",
                description="Get a datasource by ID, including readiness and capabilities.",
                input_schema={
                    "type": "object",
                    "properties": {"datasource_id": {"type": "string"}},
                    "required": ["datasource_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.get(args["datasource_id"]),
            ),
            "arcxa_register_datasource": ToolDefinition(
                name="arcxa_register_datasource",
                description="Register a datasource from a complete datasource request payload.",
                input_schema={
                    "type": "object",
                    "properties": {"datasource": {"type": "object"}},
                    "required": ["datasource"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.create(args["datasource"]),
            ),
            "arcxa_update_datasource": ToolDefinition(
                name="arcxa_update_datasource",
                description="Update an existing datasource with a partial update payload.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "datasource_id": {"type": "string"},
                        "updates": {"type": "object"},
                    },
                    "required": ["datasource_id", "updates"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.update(
                    args["datasource_id"], args["updates"]
                ),
            ),
            "arcxa_delete_datasource": ToolDefinition(
                name="arcxa_delete_datasource",
                description="Delete a datasource by ID.",
                input_schema={
                    "type": "object",
                    "properties": {"datasource_id": {"type": "string"}},
                    "required": ["datasource_id"],
                    "additionalProperties": False,
                },
                handler=self._delete_datasource,
            ),
            "arcxa_test_datasource_connection": ToolDefinition(
                name="arcxa_test_datasource_connection",
                description="Test an existing datasource connection or an inline connection payload.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "source_id": {"type": "string"},
                        "source_type": {"type": "string"},
                        "connection": {"type": "object"},
                    },
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.test_connection(
                    source_id=args.get("source_id"),
                    source_type=args.get("source_type"),
                    connection=args.get("connection"),
                ),
            ),
            "arcxa_infer_datasource_schema": ToolDefinition(
                name="arcxa_infer_datasource_schema",
                description="Infer schema for a datasource table or query target.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "datasource_id": {"type": "string"},
                        "table_name": {"type": ["string", "null"]},
                        "sample_size": {"type": "integer", "minimum": 1},
                        "enhanced": {"type": "boolean"},
                    },
                    "required": ["datasource_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.infer_schema(
                    datasource_id=args["datasource_id"],
                    table_name=args.get("table_name"),
                    sample_size=args.get("sample_size", 100),
                    enhanced=args.get("enhanced", False),
                ),
            ),
            "arcxa_query_datasource": ToolDefinition(
                name="arcxa_query_datasource",
                description="Execute a query against a ready datasource.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "datasource_id": {"type": "string"},
                        "query": {"type": "string"},
                        "parameters": {"type": "object"},
                        "limit": {"type": "integer", "minimum": 1},
                    },
                    "required": ["datasource_id", "query"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasources.query(
                    datasource_id=args["datasource_id"],
                    query=args["query"],
                    parameters=args.get("parameters"),
                    limit=args.get("limit"),
                ),
            ),
            "arcxa_list_datasets": ToolDefinition(
                name="arcxa_list_datasets",
                description="List datasets available for profiling, mapping, or workflow input.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "dataset_type": {"type": "string"},
                        "dataset_scope": {"type": "string"},
                        "page": {"type": "integer", "minimum": 0},
                        "page_size": {"type": "integer", "minimum": 1},
                    },
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.list(
                    dataset_type=args.get("dataset_type"),
                    dataset_scope=args.get("dataset_scope"),
                    page=args.get("page", 0),
                    page_size=args.get("page_size", 50),
                ),
            ),
            "arcxa_get_dataset": ToolDefinition(
                name="arcxa_get_dataset",
                description="Get a dataset by ID, including schema and lineage metadata.",
                input_schema={
                    "type": "object",
                    "properties": {"dataset_id": {"type": "string"}},
                    "required": ["dataset_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.get(args["dataset_id"]),
            ),
            "arcxa_import_dataset_file": ToolDefinition(
                name="arcxa_import_dataset_file",
                description="Import a local parquet/csv/json dataset file into ARCXA.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "file_path": {"type": "string"},
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "schema": {"type": "object"},
                    },
                    "required": ["file_path"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.import_file(
                    file_path=args["file_path"],
                    name=args.get("name"),
                    description=args.get("description"),
                    tags=args.get("tags"),
                    schema=args.get("schema"),
                ),
            ),
            "arcxa_import_dataset_from_datasource": ToolDefinition(
                name="arcxa_import_dataset_from_datasource",
                description="Materialize a datasource table into a managed dataset.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "source_id": {"type": "string"},
                        "table": {"type": "string"},
                        "schema": {"type": "string"},
                        "name": {"type": "string"},
                        "columns": {"type": "array", "items": {"type": "string"}},
                        "limit": {"type": "integer", "minimum": 1},
                        "description": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "profile": {"type": "boolean"},
                        "async_mode": {"type": "boolean"},
                        "incremental": {"type": "object"},
                    },
                    "required": ["source_id", "table"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.import_from_datasource(
                    source_id=args["source_id"],
                    table=args["table"],
                    schema=args.get("schema"),
                    name=args.get("name"),
                    columns=args.get("columns"),
                    limit=args.get("limit"),
                    description=args.get("description"),
                    tags=args.get("tags"),
                    profile=args.get("profile", False),
                    async_mode=args.get("async_mode", False),
                    incremental=args.get("incremental"),
                ),
            ),
            "arcxa_batch_import_datasource_tables": ToolDefinition(
                name="arcxa_batch_import_datasource_tables",
                description="Queue a batch import for multiple tables from one datasource.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "source_id": {"type": "string"},
                        "tables": {
                            "type": "array",
                            "items": {"type": "object"},
                            "minItems": 1,
                        },
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "profile": {"type": "boolean"},
                    },
                    "required": ["source_id", "tables"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.batch_import_from_datasource(
                    source_id=args["source_id"],
                    tables=args["tables"],
                    tags=args.get("tags"),
                    profile=args.get("profile", False),
                ),
            ),
            "arcxa_get_dataset_import_status": ToolDefinition(
                name="arcxa_get_dataset_import_status",
                description="Get one dataset import job status.",
                input_schema={
                    "type": "object",
                    "properties": {"import_id": {"type": "string"}},
                    "required": ["import_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.get_import_status(
                    args["import_id"]
                ),
            ),
            "arcxa_wait_for_dataset_import": ToolDefinition(
                name="arcxa_wait_for_dataset_import",
                description="Poll a dataset import job until it reaches a terminal state or times out.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "import_id": {"type": "string"},
                        "timeout_seconds": {"type": "integer", "minimum": 1},
                        "poll_interval_seconds": {"type": "number", "exclusiveMinimum": 0},
                    },
                    "required": ["import_id"],
                    "additionalProperties": False,
                },
                handler=self._wait_for_dataset_import,
            ),
            "arcxa_list_dataset_imports": ToolDefinition(
                name="arcxa_list_dataset_imports",
                description="List dataset import jobs.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "page": {"type": "integer", "minimum": 0},
                        "page_size": {"type": "integer", "minimum": 1},
                    },
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.datasets.list_imports(
                    status=args.get("status"),
                    page=args.get("page", 0),
                    page_size=args.get("page_size", 50),
                ),
            ),
            "arcxa_list_workflows": ToolDefinition(
                name="arcxa_list_workflows",
                description="List workflows registered in ARCXA.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "minimum": 1},
                        "offset": {"type": "integer", "minimum": 0},
                    },
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.list(
                    limit=args.get("limit", 50),
                    offset=args.get("offset", 0),
                ),
            ),
            "arcxa_get_workflow": ToolDefinition(
                name="arcxa_get_workflow",
                description="Fetch one workflow definition and metadata.",
                input_schema={
                    "type": "object",
                    "properties": {"workflow_id": {"type": "string"}},
                    "required": ["workflow_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.get(args["workflow_id"]),
            ),
            "arcxa_validate_workflow": ToolDefinition(
                name="arcxa_validate_workflow",
                description="Validate a workflow definition without creating it.",
                input_schema={
                    "type": "object",
                    "properties": {"workflow": {"type": "object"}},
                    "required": ["workflow"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.validate(args["workflow"]),
            ),
            "arcxa_create_workflow": ToolDefinition(
                name="arcxa_create_workflow",
                description="Register a workflow from a complete workflow request payload.",
                input_schema={
                    "type": "object",
                    "properties": {"workflow": {"type": "object"}},
                    "required": ["workflow"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.create(args["workflow"]),
            ),
            "arcxa_delete_workflow": ToolDefinition(
                name="arcxa_delete_workflow",
                description="Delete a workflow by ID.",
                input_schema={
                    "type": "object",
                    "properties": {"workflow_id": {"type": "string"}},
                    "required": ["workflow_id"],
                    "additionalProperties": False,
                },
                handler=self._delete_workflow,
            ),
            "arcxa_create_workflow_from_spec": ToolDefinition(
                name="arcxa_create_workflow_from_spec",
                description=(
                    "Build a workflow request from a structured spec, validate it, and optionally "
                    "create it in ARCXA."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "spec": {"type": "object"},
                        "create": {"type": "boolean"},
                    },
                    "required": ["spec"],
                    "additionalProperties": False,
                },
                handler=self._create_workflow_from_spec,
            ),
            "arcxa_execute_workflow": ToolDefinition(
                name="arcxa_execute_workflow",
                description="Execute a workflow synchronously or asynchronously.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "workflow_id": {"type": "string"},
                        "input": {"type": "object"},
                        "async_mode": {"type": "boolean"},
                    },
                    "required": ["workflow_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.execute(
                    workflow_id=args["workflow_id"],
                    inputs=args.get("input"),
                    async_mode=args.get("async_mode", False),
                ),
            ),
            "arcxa_list_workflow_executions": ToolDefinition(
                name="arcxa_list_workflow_executions",
                description="List execution history for one workflow.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "workflow_id": {"type": "string"},
                        "status": {"type": "string"},
                        "limit": {"type": "integer", "minimum": 1},
                        "offset": {"type": "integer", "minimum": 0},
                    },
                    "required": ["workflow_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.list_executions(
                    workflow_id=args["workflow_id"],
                    status=args.get("status"),
                    limit=args.get("limit", 50),
                    offset=args.get("offset", 0),
                ),
            ),
            "arcxa_get_workflow_execution": ToolDefinition(
                name="arcxa_get_workflow_execution",
                description="Get one execution record with runtime metrics and step results.",
                input_schema={
                    "type": "object",
                    "properties": {"execution_id": {"type": "string"}},
                    "required": ["execution_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.workflows.get_execution(args["execution_id"]),
            ),
            "arcxa_get_execution_progress": ToolDefinition(
                name="arcxa_get_execution_progress",
                description="Get live progress for a workflow execution.",
                input_schema={
                    "type": "object",
                    "properties": {"execution_id": {"type": "string"}},
                    "required": ["execution_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self._get_execution_progress_surface(args["execution_id"]),
            ),
            "arcxa_list_execution_progress": ToolDefinition(
                name="arcxa_list_execution_progress",
                description="List execution progress snapshots for one workflow.",
                input_schema={
                    "type": "object",
                    "properties": {"workflow_id": {"type": "string"}},
                    "required": ["workflow_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self._list_execution_progress_surface(args["workflow_id"]),
            ),
            "arcxa_list_active_executions": ToolDefinition(
                name="arcxa_list_active_executions",
                description="List active workflow executions across all workflows.",
                input_schema={
                    "type": "object",
                    "properties": {},
                    "additionalProperties": False,
                },
                handler=lambda _: self._list_active_executions_surface(),
            ),
            "arcxa_wait_for_execution": ToolDefinition(
                name="arcxa_wait_for_execution",
                description="Poll workflow execution state until completion or timeout.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "execution_id": {"type": "string"},
                        "timeout_seconds": {"type": "integer", "minimum": 1},
                        "poll_interval_seconds": {"type": "number", "exclusiveMinimum": 0},
                    },
                    "required": ["execution_id"],
                    "additionalProperties": False,
                },
                handler=self._wait_for_execution,
            ),
            "arcxa_get_run_lineage": ToolDefinition(
                name="arcxa_get_run_lineage",
                description="Get workflow lineage for an execution/run ID.",
                input_schema={
                    "type": "object",
                    "properties": {"run_id": {"type": "string"}},
                    "required": ["run_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self._get_run_lineage_surface(args["run_id"]),
            ),
            "arcxa_get_row_lineage": ToolDefinition(
                name="arcxa_get_row_lineage",
                description="Get lineage events for one specific row key.",
                input_schema={
                    "type": "object",
                    "properties": {"row_key": {"type": "string"}},
                    "required": ["row_key"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.lineage.get_row(args["row_key"]),
            ),
            "arcxa_get_row_journey": ToolDefinition(
                name="arcxa_get_row_journey",
                description="Get end-to-end row lineage journey for one row key.",
                input_schema={
                    "type": "object",
                    "properties": {"row_key": {"type": "string"}},
                    "required": ["row_key"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.lineage.get_row_journey(args["row_key"]),
            ),
            "arcxa_list_unified_mapping_sessions": ToolDefinition(
                name="arcxa_list_unified_mapping_sessions",
                description="List unified mapping sessions and their readiness state.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"},
                        "created_by": {"type": "string"},
                        "offset": {"type": "integer", "minimum": 0},
                        "limit": {"type": "integer", "minimum": 1},
                    },
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.list_sessions(
                    status=args.get("status"),
                    created_by=args.get("created_by"),
                    offset=args.get("offset", 0),
                    limit=args.get("limit", 50),
                ),
            ),
            "arcxa_analyze_datasource_for_mapping": ToolDefinition(
                name="arcxa_analyze_datasource_for_mapping",
                description=(
                    "Analyze a datasource and create a source mapping session that can feed "
                    "unified mapping or governance import flows."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "datasource_id": {"type": "string"},
                        "tables": {"type": "array", "items": {"type": "string"}},
                        "sample_size": {"type": "integer", "minimum": 1},
                        "auto_approve_threshold": {"type": "number"},
                        "min_confidence": {"type": "number"},
                        "max_candidates": {"type": "integer", "minimum": 1},
                        "ontology_namespaces": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "user_id": {"type": "string"},
                    },
                    "required": ["datasource_id", "user_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.analyze_datasource_for_mapping(
                    args["datasource_id"],
                    user_id=args["user_id"],
                    tables=args.get("tables"),
                    sample_size=args.get("sample_size"),
                    auto_approve_threshold=args.get("auto_approve_threshold"),
                    min_confidence=args.get("min_confidence"),
                    max_candidates=args.get("max_candidates"),
                    ontology_namespaces=args.get("ontology_namespaces"),
                ),
            ),
            "arcxa_analyze_dataset_for_mapping": ToolDefinition(
                name="arcxa_analyze_dataset_for_mapping",
                description=(
                    "Analyze a managed dataset and create a source mapping session for "
                    "unified mapping or governance import flows."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "dataset_id": {"type": "string"},
                        "tables": {"type": "array", "items": {"type": "string"}},
                        "sample_size": {"type": "integer", "minimum": 1},
                        "auto_approve_threshold": {"type": "number"},
                        "min_confidence": {"type": "number"},
                        "max_candidates": {"type": "integer", "minimum": 1},
                        "ontology_namespaces": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                        "user_id": {"type": "string"},
                    },
                    "required": ["dataset_id", "user_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.analyze_dataset_for_mapping(
                    args["dataset_id"],
                    user_id=args["user_id"],
                    tables=args.get("tables"),
                    sample_size=args.get("sample_size"),
                    auto_approve_threshold=args.get("auto_approve_threshold"),
                    min_confidence=args.get("min_confidence"),
                    max_candidates=args.get("max_candidates"),
                    ontology_namespaces=args.get("ontology_namespaces"),
                ),
            ),
            "arcxa_get_source_mapping_session": ToolDefinition(
                name="arcxa_get_source_mapping_session",
                description="Get a source mapping session by ID.",
                input_schema={
                    "type": "object",
                    "properties": {"session_id": {"type": "string"}},
                    "required": ["session_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.get_source_session(
                    args["session_id"]
                ),
            ),
            "arcxa_review_source_mapping_session": ToolDefinition(
                name="arcxa_review_source_mapping_session",
                description="Review or finalize field mappings for a source mapping session.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "field_mappings": {
                            "type": "array",
                            "items": {"type": "object"},
                        },
                        "reviewed_by": {"type": "string"},
                        "finalize": {"type": "boolean"},
                    },
                    "required": ["session_id", "field_mappings", "reviewed_by"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.review_source_session(
                    args["session_id"],
                    field_mappings=args["field_mappings"],
                    reviewed_by=args["reviewed_by"],
                    finalize=args.get("finalize", False),
                ),
            ),
            "arcxa_apply_source_mapping_session": ToolDefinition(
                name="arcxa_apply_source_mapping_session",
                description=(
                    "Apply an approved source mapping session into the governance store and "
                    "mark it ready for import."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "create_default_import": {"type": "boolean"},
                    },
                    "required": ["session_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.apply_source_session(
                    args["session_id"],
                    create_default_import=args.get("create_default_import", False),
                ),
            ),
            "arcxa_import_source_mapping_session": ToolDefinition(
                name="arcxa_import_source_mapping_session",
                description="Import data through an applied source mapping session.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "user_id": {"type": "string"},
                        "batch_size": {"type": "integer", "minimum": 1},
                        "target_graph": {"type": "string"},
                        "tables": {"type": "array", "items": {"type": "string"}},
                        "limit": {"type": "integer", "minimum": 1},
                    },
                    "required": ["session_id", "user_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.import_source_session(
                    args["session_id"],
                    user_id=args["user_id"],
                    batch_size=args.get("batch_size", 1000),
                    target_graph=args.get("target_graph"),
                    tables=args.get("tables"),
                    limit=args.get("limit"),
                ),
            ),
            "arcxa_suggest_field_mappings": ToolDefinition(
                name="arcxa_suggest_field_mappings",
                description="Suggest field mappings across multiple datasets or dataset-like inputs.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "datasets": {
                            "type": "array",
                            "items": {"type": "object"},
                            "minItems": 2,
                        }
                    },
                    "required": ["datasets"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.suggest(args["datasets"]),
            ),
            "arcxa_create_unified_mapping_session": ToolDefinition(
                name="arcxa_create_unified_mapping_session",
                description="Create a unified mapping session for multi-source consolidation.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "source_session_ids": {
                            "type": "array",
                            "items": {"type": "string"},
                            "minItems": 1,
                        },
                        "target_database": {"type": "object"},
                        "created_by": {"type": "string"},
                    },
                    "required": ["source_session_ids", "target_database", "created_by"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.create_session(
                    source_session_ids=args["source_session_ids"],
                    target_database=args["target_database"],
                    created_by=args["created_by"],
                ),
            ),
            "arcxa_get_unified_mapping_session": ToolDefinition(
                name="arcxa_get_unified_mapping_session",
                description="Get a unified mapping session by ID.",
                input_schema={
                    "type": "object",
                    "properties": {"session_id": {"type": "string"}},
                    "required": ["session_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.get_session(args["session_id"]),
            ),
            "arcxa_update_unified_mapping_session": ToolDefinition(
                name="arcxa_update_unified_mapping_session",
                description=(
                    "Replace or refine unified mapping session field mappings and/or "
                    "target database configuration before load."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "target_database": {"type": "object"},
                        "field_mappings": {
                            "type": "array",
                            "items": {"type": "object"},
                        },
                    },
                    "required": ["session_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.update_session(
                    session_id=args["session_id"],
                    target_database=args.get("target_database"),
                    field_mappings=args.get("field_mappings"),
                ),
            ),
            "arcxa_resolve_mapping_conflicts": ToolDefinition(
                name="arcxa_resolve_mapping_conflicts",
                description="Resolve unified mapping conflicts for a session.",
                input_schema={
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "resolutions": {"type": "object"},
                    },
                    "required": ["session_id", "resolutions"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.resolve_conflicts(
                    session_id=args["session_id"],
                    resolutions=args["resolutions"],
                ),
            ),
            "arcxa_load_unified_mapping_session": ToolDefinition(
                name="arcxa_load_unified_mapping_session",
                description=(
                    "Load a unified mapping session into a target database. "
                    "Connection config is only required for external backends such as DB2."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "database_type": {"type": "string"},
                        "connection_config": {"type": "object"},
                        "create_tables": {"type": "boolean"},
                        "validate_data": {"type": "boolean"},
                        "batch_size": {"type": "integer", "minimum": 1},
                    },
                    "required": ["session_id", "database_type"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.load_to_database(
                    session_id=args["session_id"],
                    database_type=_normalize_database_type(args["database_type"]),
                    connection_config=args.get("connection_config"),
                    create_tables=args.get("create_tables", True),
                    validate_data=args.get("validate_data", True),
                    batch_size=args.get("batch_size", 1000),
                ),
            ),
            "arcxa_get_mapping_load_job": ToolDefinition(
                name="arcxa_get_mapping_load_job",
                description="Get status for a unified mapping load job.",
                input_schema={
                    "type": "object",
                    "properties": {"job_id": {"type": "string"}},
                    "required": ["job_id"],
                    "additionalProperties": False,
                },
                handler=lambda args: self.client.mapping.get_load_job_status(args["job_id"]),
            ),
            "arcxa_get_mapping_statistics": ToolDefinition(
                name="arcxa_get_mapping_statistics",
                description="Get global unified mapping statistics.",
                input_schema={"type": "object", "properties": {}, "additionalProperties": False},
                handler=lambda _: self.client.mapping.statistics(),
            ),
            "arcxa_recommend_execution_surface": ToolDefinition(
                name="arcxa_recommend_execution_surface",
                description=(
                    "Recommend whether a data movement plan should use direct workflows or "
                    "the unified mapping/session surface."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "sources": {"type": "array", "items": {"type": "object"}},
                        "join": {"type": "object"},
                        "target": {"type": "object"},
                    },
                    "required": ["sources"],
                    "additionalProperties": False,
                },
                handler=recommend_execution_surface,
            ),
            "arcxa_build_etl_workflow_definition": ToolDefinition(
                name="arcxa_build_etl_workflow_definition",
                description=(
                    "Build a structured workflow registration payload for single-source ETL "
                    "workflows with optional transform, validation, deduplication, aggregation, "
                    "semantic mapping, and target loading/export."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "workflow_id": {"type": "string"},
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "sources": {"type": "array", "items": {"type": "object"}, "minItems": 1},
                        "field_transformations": {
                            "type": "array",
                            "items": {"type": "object"},
                        },
                        "validation_rules": {"type": "array", "items": {"type": "object"}},
                        "fail_on_error": {"type": "boolean"},
                        "deduplicator": {"type": "object"},
                        "aggregator": {"type": "object"},
                        "semantic_mapper": {"type": "object"},
                        "target": {"type": "object"},
                        "fusion_threshold": {"type": "number"},
                        "fallback": {"type": "string"},
                    },
                    "required": ["name", "sources"],
                    "additionalProperties": False,
                },
                handler=build_etl_workflow_definition,
            ),
            "arcxa_build_unified_mapping_plan": ToolDefinition(
                name="arcxa_build_unified_mapping_plan",
                description=(
                    "Build a structured multi-source integration plan for dataset materialization, "
                    "mapping, conflict resolution, and target loading."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "sources": {"type": "array", "items": {"type": "object"}, "minItems": 2},
                        "join": {"type": "object"},
                        "target": {"type": "object"},
                        "created_by": {"type": "string"},
                    },
                    "required": ["name", "sources"],
                    "additionalProperties": True,
                },
                handler=build_unified_mapping_plan,
            ),
            "arcxa_plan_data_integration": ToolDefinition(
                name="arcxa_plan_data_integration",
                description=(
                    "Build a full agent-facing plan for ETL or multi-source integration, "
                    "including whether to use workflows or unified mapping."
                ),
                input_schema={
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "sources": {"type": "array", "items": {"type": "object"}, "minItems": 1},
                        "join": {"type": "object"},
                        "target": {"type": "object"},
                        "created_by": {"type": "string"},
                    },
                    "required": ["name", "sources"],
                    "additionalProperties": True,
                },
                handler=plan_data_integration,
            ),
        }

    def _build_prompts(self) -> Dict[str, PromptDefinition]:
        return {
            "design_oracle_parquet_to_db2": PromptDefinition(
                name="design_oracle_parquet_to_db2",
                description="Guide an agent through a safe Oracle + parquet -> DB2 integration.",
                arguments=[
                    {"name": "oracle_datasource_id", "required": True},
                    {"name": "oracle_table", "required": True},
                    {"name": "parquet_file_path", "required": True},
                    {"name": "db2_datasource_id", "required": True},
                    {"name": "target_table", "required": True},
                    {"name": "created_by", "required": False},
                ],
                handler=self._prompt_design_oracle_parquet_to_db2,
            ),
            "author_single_source_etl_workflow": PromptDefinition(
                name="author_single_source_etl_workflow",
                description="Guide an agent through authoring a safe single-source ETL workflow.",
                arguments=[
                    {"name": "source_datasource_id", "required": True},
                    {"name": "source_table", "required": True},
                    {"name": "target_datasource_id", "required": True},
                    {"name": "target_table", "required": True},
                ],
                handler=self._prompt_author_single_source_etl_workflow,
            ),
            "triage_workflow_execution": PromptDefinition(
                name="triage_workflow_execution",
                description="Guide an agent through inspecting execution, progress, and lineage.",
                arguments=[{"name": "execution_id", "required": True}],
                handler=self._prompt_triage_workflow_execution,
            ),
        }

    def handle_request(self, request: JSON) -> Optional[JSON]:
        method = request.get("method")
        request_id = request.get("id")
        params = request.get("params", {}) or {}

        if method == "initialize":
            protocol_version = params.get("protocolVersion", "2024-11-05")
            return self._success(
                request_id,
                {
                    "protocolVersion": protocol_version,
                    "capabilities": {"tools": {}, "prompts": {}},
                    "serverInfo": {"name": "arcxa-mcp", "version": "0.2.0"},
                },
            )

        if method in {"notifications/initialized", "initialized"}:
            return None

        if method == "ping":
            return self._success(request_id, {})

        if method == "tools/list":
            return self._success(
                request_id,
                {"tools": [tool.to_mcp() for tool in self.tools.values()]},
            )

        if method == "resources/list":
            return self._success(request_id, {"resources": []})

        if method == "prompts/list":
            return self._success(
                request_id,
                {"prompts": [prompt.to_mcp() for prompt in self.prompts.values()]},
            )

        if method == "prompts/get":
            return self._handle_prompt_get(request_id, params)

        if method == "tools/call":
            return self._handle_tool_call(request_id, params)

        return self._error(request_id, -32601, f"Method not found: {method}")

    def _handle_tool_call(self, request_id: Any, params: JSON) -> JSON:
        tool_name = params.get("name")
        arguments = params.get("arguments", {}) or {}
        tool = self.tools.get(tool_name)
        if tool is None:
            return self._success(
                request_id,
                self._tool_error_result(f"Unknown tool: {tool_name}"),
            )

        try:
            result = tool.handler(arguments)
            return self._success(request_id, self._tool_success_result(result))
        except (GraphicaError, ValueError, KeyError) as error:
            logger.warning("Tool %s failed: %s", tool_name, error)
            return self._success(request_id, self._tool_error_result(str(error)))
        except Exception as error:  # pragma: no cover
            logger.exception("Unhandled tool failure for %s", tool_name)
            return self._success(
                request_id,
                self._tool_error_result(f"Unhandled server error: {error}"),
            )

    def _handle_prompt_get(self, request_id: Any, params: JSON) -> JSON:
        prompt_name = params.get("name")
        arguments = params.get("arguments", {}) or {}
        prompt = self.prompts.get(prompt_name)
        if prompt is None:
            return self._error(request_id, -32602, f"Unknown prompt: {prompt_name}")
        return self._success(request_id, prompt.handler(arguments))

    def _create_workflow_from_spec(self, args: JSON) -> JSON:
        spec = args["spec"]
        create = args.get("create", False)

        workflow_request = build_etl_workflow_definition(spec)
        register_request = workflow_request["workflow"]
        validation = self.client.workflows.validate(register_request["definition"])
        response: JSON = {
            "workflow": register_request,
            "validation": validation,
            "created": None,
        }

        if create and self._validation_allows_creation(validation):
            response["created"] = self.client.workflows.create(register_request)

        return response

    def _delete_datasource(self, args: JSON) -> JSON:
        datasource_id = args["datasource_id"]
        self.client.datasources.delete(datasource_id)
        return {"deleted": True, "datasource_id": datasource_id}

    def _delete_workflow(self, args: JSON) -> JSON:
        workflow_id = args["workflow_id"]
        self.client.workflows.delete(workflow_id)
        return {"deleted": True, "workflow_id": workflow_id}

    def _is_progress_tracking_unavailable(self, error: GraphicaError) -> bool:
        return "progress tracking not available" in str(error).lower()

    def _get_execution_progress_surface(self, execution_id: str) -> JSON:
        try:
            progress = self.client.workflows.get_execution_progress(execution_id)
        except GraphicaError as error:
            if not self._is_progress_tracking_unavailable(error):
                raise
            return {
                "available": False,
                "execution_id": execution_id,
                "status": "unavailable",
                "reason": str(error),
            }

        if isinstance(progress, dict):
            return {
                "available": True,
                "execution_id": progress.get("execution_id", execution_id),
                **progress,
            }

        return {
            "available": True,
            "execution_id": execution_id,
            "status": "unknown",
            "progress": progress,
        }

    def _list_execution_progress_surface(self, workflow_id: str) -> JSON:
        try:
            entries = self.client.workflows.list_execution_progress(workflow_id)
        except GraphicaError as error:
            if not self._is_progress_tracking_unavailable(error):
                raise
            return {
                "available": False,
                "workflow_id": workflow_id,
                "entries": [],
                "reason": str(error),
            }

        if isinstance(entries, dict):
            return {
                "available": True,
                "workflow_id": entries.get("workflow_id", workflow_id),
                "entries": entries.get("entries", []),
                **entries,
            }

        return {
            "available": True,
            "workflow_id": workflow_id,
            "entries": list(entries),
        }

    def _list_active_executions_surface(self) -> JSON:
        try:
            executions = self.client.workflows.list_active_executions()
        except GraphicaError as error:
            if not self._is_progress_tracking_unavailable(error):
                raise
            return {
                "available": False,
                "executions": [],
                "reason": str(error),
            }

        if isinstance(executions, dict):
            return {
                "available": True,
                "executions": executions.get("executions", []),
                **executions,
            }

        return {
            "available": True,
            "executions": list(executions),
        }

    def _get_run_lineage_surface(self, run_id: str) -> JSON:
        try:
            lineage = self.client.lineage.get_run(run_id)
        except GraphicaError as error:
            lowered = str(error).lower()
            if "sparql query failed" in lowered or "unsupported sparql query" in lowered:
                unified_load_fallback = self._build_unified_load_run_fallback(run_id, str(error))
                if unified_load_fallback is not None:
                    return unified_load_fallback
                execution_fallback = self._build_workflow_execution_run_fallback(run_id, str(error))
                if execution_fallback is not None:
                    return execution_fallback
            if (
                lowered == "notfound"
                or "not found" in lowered
                or "no lineage found" in lowered
            ):
                unified_load_fallback = self._build_unified_load_run_fallback(run_id, str(error))
                if unified_load_fallback is not None:
                    return unified_load_fallback
                execution_fallback = self._build_workflow_execution_run_fallback(run_id, str(error))
                if execution_fallback is not None:
                    return execution_fallback
                return {
                    "available": False,
                    "run_id": run_id,
                    "reason": str(error),
                    "fallback": "unavailable",
                }
            raise

        if isinstance(lineage, dict):
            return {
                "available": True,
                "run_id": lineage.get("run_id", run_id),
                **lineage,
            }

        return {
            "available": True,
            "run_id": run_id,
            "records": lineage,
        }

    def _build_workflow_execution_run_fallback(
        self, run_id: str, reason: str
    ) -> Optional[JSON]:
        try:
            execution = self.client.workflows.get_execution(run_id)
        except GraphicaError:
            return None

        return {
            "available": False,
            "run_id": run_id,
            "reason": reason,
            "fallback": "workflow_execution",
            "execution": execution,
        }

    def _build_unified_load_run_fallback(
        self, run_id: str, reason: str
    ) -> Optional[JSON]:
        prefix = "unified_load_"
        if not run_id.startswith(prefix):
            return None

        load_job_id = run_id[len(prefix) :]
        if not load_job_id:
            return None

        try:
            load_job = self.client.mapping.get_load_job_status(load_job_id)
        except GraphicaError:
            return None

        return {
            "available": False,
            "run_id": run_id,
            "reason": reason,
            "fallback": "unified_mapping_load_job",
            "load_job_id": load_job_id,
            "load_job": load_job,
        }

    def _wait_for_execution(self, args: JSON) -> JSON:
        execution_id = args["execution_id"]
        timeout_seconds = int(args.get("timeout_seconds", 120))
        poll_interval_seconds = float(args.get("poll_interval_seconds", 1.0))
        deadline = time.time() + timeout_seconds

        last_progress: Optional[JSON] = None
        last_execution: Optional[JSON] = None

        while True:
            try:
                last_progress = self.client.workflows.get_execution_progress(execution_id)
            except GraphicaError:
                pass

            try:
                last_execution = self.client.workflows.get_execution(execution_id)
            except GraphicaError:
                if last_execution is None and last_progress is None:
                    raise

            status = self._resolve_status(last_execution, last_progress)
            if self._is_terminal_status(status):
                return {
                    "execution_id": execution_id,
                    "status": status,
                    "terminal": True,
                    "timed_out": False,
                    "progress": last_progress,
                    "execution": last_execution,
                }

            if time.time() >= deadline:
                return {
                    "execution_id": execution_id,
                    "status": status,
                    "terminal": self._is_terminal_status(status),
                    "timed_out": True,
                    "progress": last_progress,
                    "execution": last_execution,
                }

            time.sleep(poll_interval_seconds)

    def _wait_for_dataset_import(self, args: JSON) -> JSON:
        import_id = args["import_id"]
        timeout_seconds = int(args.get("timeout_seconds", 120))
        poll_interval_seconds = float(args.get("poll_interval_seconds", 1.0))
        deadline = time.time() + timeout_seconds
        last_status: Optional[JSON] = None

        while True:
            last_status = self.client.datasets.get_import_status(import_id)
            status = str(last_status.get("status", "")).lower()
            if status in {"imported", "completed_with_errors", "failed"}:
                return {
                    "import_id": import_id,
                    "status": status,
                    "terminal": True,
                    "timed_out": False,
                    "import": last_status,
                }

            if time.time() >= deadline:
                return {
                    "import_id": import_id,
                    "status": status,
                    "terminal": False,
                    "timed_out": True,
                    "import": last_status,
                }

            time.sleep(poll_interval_seconds)

    @staticmethod
    def _validation_allows_creation(validation: JSON) -> bool:
        if validation.get("valid") is False:
            return False
        errors = validation.get("errors")
        return not errors

    @staticmethod
    def _resolve_status(
        execution: Optional[JSON], progress: Optional[JSON]
    ) -> Optional[str]:
        if execution and execution.get("status") is not None:
            return str(execution["status"])
        if progress and progress.get("status") is not None:
            return str(progress["status"])
        return None

    @staticmethod
    def _is_terminal_status(status: Optional[str]) -> bool:
        if status is None:
            return False
        return status.lower() in {"completed", "failed", "stopped", "aborted"}

    def _prompt_design_oracle_parquet_to_db2(self, args: JSON) -> JSON:
        created_by = args.get("created_by", "agent")
        text = (
            "Design a safe ARCXA data integration for one Oracle table and one local parquet file "
            "loading into DB2. First inspect datasource readiness, infer the Oracle schema, and "
            "create a source mapping session for the Oracle source. Import the local parquet file "
            "as a managed dataset, analyze that dataset for mapping, and use unified mapping "
            "rather than a direct workflow join. Build the mapping/load plan, create or inspect "
            "source sessions, create the unified session when "
            "all executable source sessions exist, refine unified field mappings when the target "
            "contract needs explicit control, resolve any remaining conflicts, load into DB2, and "
            "finish by checking load status plus run lineage.\n\n"
            f"Oracle datasource: {args['oracle_datasource_id']}\n"
            f"Oracle table: {args['oracle_table']}\n"
            f"Parquet file: {args['parquet_file_path']}\n"
            f"DB2 datasource: {args['db2_datasource_id']}\n"
            f"Target table: {args['target_table']}\n"
            f"Created by: {created_by}\n\n"
            "Use these tools when appropriate: arcxa_get_datasource, "
            "arcxa_infer_datasource_schema, arcxa_import_dataset_file, "
            "arcxa_import_dataset_from_datasource, arcxa_plan_data_integration, "
            "arcxa_analyze_datasource_for_mapping, arcxa_analyze_dataset_for_mapping, "
            "arcxa_get_source_mapping_session, "
            "arcxa_review_source_mapping_session, arcxa_apply_source_mapping_session, "
            "arcxa_import_source_mapping_session, "
            "arcxa_suggest_field_mappings, arcxa_create_unified_mapping_session, "
            "arcxa_get_unified_mapping_session, arcxa_update_unified_mapping_session, "
            "arcxa_resolve_mapping_conflicts, arcxa_load_unified_mapping_session, "
            "arcxa_get_mapping_load_job, arcxa_get_run_lineage."
        )
        return {
            "description": "Design a multi-source Oracle + parquet -> DB2 integration.",
            "messages": [{"role": "user", "content": {"type": "text", "text": text}}],
        }

    def _prompt_author_single_source_etl_workflow(self, args: JSON) -> JSON:
        text = (
            "Create a safe single-source ETL workflow in ARCXA. Inspect datasource readiness, "
            "infer schema, decide any needed field transformations or validation rules, build a "
            "workflow-safe spec, generate the workflow request, validate it, and only create it "
            "if validation is clean. After creation, be ready to execute and inspect progress.\n\n"
            f"Source datasource: {args['source_datasource_id']}\n"
            f"Source table: {args['source_table']}\n"
            f"Target datasource: {args['target_datasource_id']}\n"
            f"Target table: {args['target_table']}\n\n"
            "Use these tools when appropriate: arcxa_get_datasource, arcxa_infer_datasource_schema, "
            "arcxa_plan_data_integration, arcxa_create_workflow_from_spec, "
            "arcxa_execute_workflow, arcxa_get_execution_progress, arcxa_get_run_lineage."
        )
        return {
            "description": "Author a single-source ETL workflow and validate it before creation.",
            "messages": [{"role": "user", "content": {"type": "text", "text": text}}],
        }

    def _prompt_triage_workflow_execution(self, args: JSON) -> JSON:
        text = (
            "Inspect this ARCXA workflow execution systematically. Fetch the execution record, "
            "check live progress if it is still active, inspect runtime metrics, and then review "
            "run lineage and row journeys for any suspect rows.\n\n"
            f"Execution ID: {args['execution_id']}\n\n"
            "Use these tools when appropriate: arcxa_get_workflow_execution, "
            "arcxa_get_execution_progress, arcxa_get_run_lineage, arcxa_get_row_journey."
        )
        return {
            "description": "Inspect one workflow execution across progress, metrics, and lineage.",
            "messages": [{"role": "user", "content": {"type": "text", "text": text}}],
        }

    @staticmethod
    def _tool_success_result(result: Any) -> JSON:
        safe_result = _json_safe(result)
        return {
            "content": [{"type": "text", "text": json.dumps(safe_result, indent=2)}],
            "structuredContent": safe_result,
            "isError": False,
        }

    @staticmethod
    def _tool_error_result(message: str) -> JSON:
        return {
            "content": [{"type": "text", "text": message}],
            "isError": True,
        }

    @staticmethod
    def _success(request_id: Any, result: JSON) -> JSON:
        return {"jsonrpc": "2.0", "id": request_id, "result": result}

    @staticmethod
    def _error(request_id: Any, code: int, message: str) -> JSON:
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": code, "message": message},
        }


def build_client_from_env(environ: Optional[Dict[str, str]] = None) -> Client:
    import os

    env = environ or os.environ
    base_url = env.get("ARCXA_BASE_URL", "http://localhost:8080")
    timeout = int(env.get("ARCXA_TIMEOUT", "30"))

    token = env.get("ARCXA_TOKEN")
    username = env.get("ARCXA_USERNAME")
    password = env.get("ARCXA_PASSWORD")

    auth = None
    if token:
        auth = TokenAuth(token)
    elif username and password:
        auth = BasicAuth(username, password)

    return Client(base_url=base_url, auth=auth, timeout=timeout)


def read_message(stdin: Any) -> Optional[JSON]:
    """Read one MCP/JSON-RPC message from stdio."""
    first_line = stdin.buffer.readline()
    if not first_line:
        return None

    if first_line.lstrip().startswith(b"{"):
        return json.loads(first_line.decode("utf-8"))

    headers: Dict[str, str] = {}
    line = first_line
    while line and line not in (b"\r\n", b"\n"):
        decoded = line.decode("utf-8").strip()
        if ":" in decoded:
            key, value = decoded.split(":", 1)
            headers[key.strip().lower()] = value.strip()
        line = stdin.buffer.readline()

    content_length = int(headers.get("content-length", "0"))
    if content_length <= 0:
        raise ValueError("Missing or invalid Content-Length header")

    body = stdin.buffer.read(content_length)
    return json.loads(body.decode("utf-8"))


def write_message(stdout: Any, payload: JSON) -> None:
    encoded = json.dumps(payload).encode("utf-8")
    stdout.buffer.write(f"Content-Length: {len(encoded)}\r\n\r\n".encode("utf-8"))
    stdout.buffer.write(encoded)
    stdout.buffer.flush()


def serve(server: ArcxaMcpServer, stdin: Any = sys.stdin, stdout: Any = sys.stdout) -> None:
    while True:
        request = read_message(stdin)
        if request is None:
            return
        response = server.handle_request(request)
        if response is not None:
            write_message(stdout, response)


def _json_safe(value: Any) -> Any:
    if isinstance(value, dict):
        return {str(key): _json_safe(val) for key, val in value.items()}
    if isinstance(value, list):
        return [_json_safe(item) for item in value]
    if isinstance(value, tuple):
        return [_json_safe(item) for item in value]
    return value
