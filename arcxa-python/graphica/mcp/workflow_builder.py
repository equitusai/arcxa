"""Structured workflow planning helpers for MCP tools."""

from copy import deepcopy
from typing import Any, Dict, List, Optional


def _normalize_fallback_strategy(raw: Optional[str]) -> str:
    """Normalize friendly fallback aliases to live API enum values."""
    value = (raw or "manual_review").strip().lower()
    aliases = {
        "manual_review": "manual_review",
        "manual": "manual_review",
        "review": "manual_review",
        "reject_fusion": "reject_fusion",
        "reject": "reject_fusion",
        "fail": "reject_fusion",
        "accept_fusion": "accept_fusion",
        "accept": "accept_fusion",
    }
    if value not in aliases:
        raise ValueError(
            "Unsupported fallback strategy. Use one of: manual_review, reject_fusion, accept_fusion"
        )
    return aliases[value]


def _normalize_database_type(raw: Optional[str]) -> str:
    """Normalize friendly database aliases to the live API enum values."""
    value = (raw or "d_b2").strip().lower()
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


def recommend_execution_surface(spec: Dict[str, Any]) -> Dict[str, Any]:
    """Recommend the safest ARCXA execution surface for a structured ETL plan."""
    sources = spec.get("sources", [])
    requires_join = bool(spec.get("join"))
    target = spec.get("target", {}) or {}
    target_kind = target.get("kind", "workflow")

    if requires_join or len(sources) > 1:
        return {
            "recommended_surface": "unified_mapping",
            "reason": (
                "Multi-source consolidation should use unified mapping sessions today. "
                "The workflow data_joiner step exists, but it is not the safest execution-ready "
                "surface for enterprise ETL."
            ),
            "safe_for_direct_workflow_execution": False,
            "notes": [
                "Use datasource registration + schema inference for each source first.",
                "Use unified mapping sessions to reconcile fields across parquet/oracle sources.",
                "Use the unified load path for DB2 or Oracle/PostgreSQL targets.",
            ],
            "target_kind": target_kind,
        }

    return {
        "recommended_surface": "workflow",
        "reason": (
            "This is a single-source ETL shape, so the workflow surface is the safest direct "
            "execution path."
        ),
        "safe_for_direct_workflow_execution": True,
        "notes": [
            "Validate the workflow before creation.",
            "Use execution progress and lineage tools after execution.",
        ],
        "target_kind": target_kind,
    }


def build_etl_workflow_definition(spec: Dict[str, Any]) -> Dict[str, Any]:
    """Build a structured ARCXA workflow request from a high-level ETL spec."""
    recommendation = recommend_execution_surface(spec)
    if not recommendation["safe_for_direct_workflow_execution"]:
        raise ValueError(
            "This ETL plan should use the unified_mapping surface instead of a direct workflow. "
            "Call recommend_execution_surface first or split the plan into workflow-safe stages."
        )

    name = spec["name"]
    workflow_id = spec.get("workflow_id")
    description = spec.get("description")
    tags = spec.get("tags", [])
    sources = spec.get("sources", [])
    if not sources:
        raise ValueError("At least one source is required")

    steps: List[Dict[str, Any]] = []
    current_step_ids: List[str] = []

    for index, source in enumerate(sources):
        step_id = source.get("step_id") or f"extract_{index + 1}"
        if source.get("kind", "db_extract") != "db_extract":
            raise ValueError(
                "Only db_extract workflow sources are supported by build_etl_workflow_definition"
            )

        config: Dict[str, Any] = {
            "datasource_id": source["datasource_id"],
            "table_name": source.get("table_name"),
            "schema_table": source.get("schema_table"),
            "query": source.get("query"),
            "incremental": source.get("incremental"),
            "incremental_column": source.get("incremental_column"),
            "last_value": source.get("last_value"),
            "batch_size": source.get("batch_size", 50000),
            "columns": source.get("columns"),
            "include_schema": source.get("include_schema"),
            "schema_sample_size": source.get("schema_sample_size"),
        }
        config = {k: v for k, v in config.items() if v is not None}

        steps.append(
            {
                "id": step_id,
                "step_type": "db_extract",
                "config": config,
                "depends_on": source.get("depends_on", []),
            }
        )
        current_step_ids = [step_id]

    if len(current_step_ids) != 1:
        raise ValueError("Only single-source direct workflow plans are supported here")

    current_step_id = current_step_ids[0]

    transformations = spec.get("field_transformations") or []
    if transformations:
        step_id = spec.get("field_transformer_step_id", "transform_fields")
        steps.append(
            {
                "id": step_id,
                "step_type": "field_transformer",
                "config": {"transformations": deepcopy(transformations)},
                "depends_on": [current_step_id],
            }
        )
        current_step_id = step_id

    validation_rules = spec.get("validation_rules") or []
    if validation_rules:
        step_id = spec.get("data_validator_step_id", "validate_rows")
        steps.append(
            {
                "id": step_id,
                "step_type": "data_validator",
                "config": {
                    "rules": deepcopy(validation_rules),
                    "fail_on_error": spec.get("fail_on_error", True),
                },
                "depends_on": [current_step_id],
            }
        )
        current_step_id = step_id

    deduplicator = spec.get("deduplicator")
    if deduplicator:
        step_id = deduplicator.get("step_id", "deduplicate_rows")
        config = {
            "method": deduplicator["method"],
            "key_fields": deduplicator["key_fields"],
            "threshold": deduplicator.get("threshold"),
            "keep": deduplicator.get("keep", "first"),
        }
        config = {k: v for k, v in config.items() if v is not None}
        steps.append(
            {
                "id": step_id,
                "step_type": "deduplicator",
                "config": config,
                "depends_on": [current_step_id],
            }
        )
        current_step_id = step_id

    aggregator = spec.get("aggregator")
    if aggregator:
        step_id = aggregator.get("step_id", "aggregate_rows")
        config = {
            "group_by": aggregator.get("group_by", []),
            "aggregations": aggregator.get("aggregations", []),
        }
        steps.append(
            {
                "id": step_id,
                "step_type": "aggregator",
                "config": config,
                "depends_on": [current_step_id],
            }
        )
        current_step_id = step_id

    semantic_mapper = spec.get("semantic_mapper")
    if semantic_mapper:
        step_id = semantic_mapper.get("step_id", "map_semantics")
        config = {
            "target_ontology": semantic_mapper["target_ontology"],
            "auto_approve_threshold": semantic_mapper.get(
                "auto_approve_threshold", 0.95
            ),
            "mapping_mode": semantic_mapper.get("mapping_mode", "hybrid"),
            "mapping_session_id": semantic_mapper.get("mapping_session_id"),
            "source_id": semantic_mapper.get("source_id"),
            "table_name": semantic_mapper.get("table_name"),
            "entity_uri": semantic_mapper.get("entity_uri"),
        }
        config = {k: v for k, v in config.items() if v is not None}
        steps.append(
            {
                "id": step_id,
                "step_type": "semantic_mapper",
                "config": config,
                "depends_on": [current_step_id],
            }
        )
        current_step_id = step_id

    target = spec.get("target")
    if target:
        kind = target.get("kind", "db_loader")
        if kind == "db_loader":
            step_id = target.get("step_id", "load_target")
            config = {
                "datasource_id": target["datasource_id"],
                "table_name": target["table_name"],
                "mode": target.get("mode", "insert"),
                "key_fields": target.get("key_fields"),
                "batch_size": target.get("batch_size", 50000),
                "create_table": target.get("create_table", False),
                "entity_uri": target.get("entity_uri"),
            }
            config = {k: v for k, v in config.items() if v is not None}
            steps.append(
                {
                    "id": step_id,
                    "step_type": "db_loader",
                    "config": config,
                    "depends_on": [current_step_id],
                }
            )
            current_step_id = step_id
        elif kind == "csv_exporter":
            step_id = target.get("step_id", "export_csv")
            config = {
                "output_path": target["output_path"],
                "include_headers": target.get("include_headers", True),
                "delimiter": target.get("delimiter", ","),
            }
            steps.append(
                {
                    "id": step_id,
                    "step_type": "csv_exporter",
                    "config": config,
                    "depends_on": [current_step_id],
                }
            )
            current_step_id = step_id
        else:
            raise ValueError(f"Unsupported target kind: {kind}")

    return {
        "workflow": {
            "id": workflow_id,
            "name": name,
            "description": description,
            "tags": tags,
            "definition": {
                "steps": steps,
                "fusion_threshold": spec.get("fusion_threshold", 0.75),
                "fallback": _normalize_fallback_strategy(spec.get("fallback")),
            },
        },
        "current_terminal_step_id": current_step_id,
        "recommended_surface": recommendation["recommended_surface"],
        "notes": recommendation["notes"],
    }


def build_unified_mapping_plan(spec: Dict[str, Any]) -> Dict[str, Any]:
    """Build a structured multi-source consolidation plan for agent execution."""
    sources = spec.get("sources", [])
    if len(sources) < 2 and not spec.get("join"):
        raise ValueError(
            "Unified mapping plans are intended for multi-source or explicit join/fusion work"
        )

    target = spec.get("target", {}) or {}
    created_by = spec.get("created_by", "agent")
    dataset_preparation: List[Dict[str, Any]] = []
    source_session_preparation: List[Dict[str, Any]] = []
    dataset_inputs: List[Dict[str, Any]] = []
    source_session_inputs: List[Dict[str, Any]] = []
    blocking_reasons: List[str] = []
    provided_source_session_ids = spec.get("source_session_ids", [])

    for index, source in enumerate(sources):
        source_ref = source.get("id") or source.get("name") or f"source_{index + 1}"
        kind = source.get("kind", "dataset")

        if kind == "dataset":
            dataset_id = source["dataset_id"]
            dataset_preparation.append(
                {
                    "phase": "reuse_dataset",
                    "source_ref": source_ref,
                    "tool": "arcxa_get_dataset",
                    "arguments": {"dataset_id": dataset_id},
                    "expected_output": "dataset schema and lineage metadata",
                }
            )
            dataset_inputs.append(
                {"source_ref": source_ref, "dataset_id": dataset_id}
            )
            source_session_preparation.append(
                {
                    "phase": "analyze_dataset_for_mapping",
                    "source_ref": source_ref,
                    "tool": "arcxa_analyze_dataset_for_mapping",
                    "arguments": {
                        "dataset_id": dataset_id,
                        "tables": source.get("tables"),
                        "sample_size": source.get("sample_size", 100),
                        "auto_approve_threshold": source.get(
                            "auto_approve_threshold", 0.95
                        ),
                        "min_confidence": source.get("min_confidence"),
                        "max_candidates": source.get("max_candidates"),
                        "ontology_namespaces": source.get("ontology_namespaces"),
                        "user_id": created_by,
                    },
                    "expected_output": "source mapping session_id for unified session creation",
                }
            )
            source_session_inputs.append(
                {
                    "source_ref": source_ref,
                    "session_id": f"<source_mapping_session_id_from_{source_ref}_dataset_analysis>",
                }
            )
            continue

        if kind in {"parquet_file", "csv_file", "json_file", "file_import"}:
            dataset_id_placeholder = f"<dataset_id_from_{source_ref}_file_import>"
            dataset_preparation.append(
                {
                    "phase": "materialize_file_dataset",
                    "source_ref": source_ref,
                    "tool": "arcxa_import_dataset_file",
                    "arguments": {
                        "file_path": source["file_path"],
                        "name": source.get("dataset_name") or source_ref,
                        "description": source.get("description"),
                        "tags": source.get("tags", []),
                    },
                    "expected_output": "dataset_id for the imported file-backed dataset",
                }
            )
            dataset_inputs.append(
                {
                    "source_ref": source_ref,
                    "dataset_id": dataset_id_placeholder,
                }
            )
            source_session_preparation.append(
                {
                    "phase": "analyze_imported_dataset_for_mapping",
                    "source_ref": source_ref,
                    "tool": "arcxa_analyze_dataset_for_mapping",
                    "arguments": {
                        "dataset_id": dataset_id_placeholder,
                        "tables": source.get("tables"),
                        "sample_size": source.get("sample_size", 100),
                        "auto_approve_threshold": source.get(
                            "auto_approve_threshold", 0.95
                        ),
                        "min_confidence": source.get("min_confidence"),
                        "max_candidates": source.get("max_candidates"),
                        "ontology_namespaces": source.get("ontology_namespaces"),
                        "user_id": created_by,
                    },
                    "expected_output": "source mapping session_id for unified session creation",
                }
            )
            source_session_inputs.append(
                {
                    "source_ref": source_ref,
                    "session_id": f"<source_mapping_session_id_from_{source_ref}_dataset_analysis>",
                }
            )
            continue

        if kind in {
            "datasource_table",
            "datasource_query",
            "oracle_table",
            "db2_table",
            "source_table",
        }:
            datasource_id = source["datasource_id"]
            table_name = source.get("table_name") or source.get("table")
            datasource_steps = [
                {
                    "phase": "check_datasource",
                    "source_ref": source_ref,
                    "tool": "arcxa_get_datasource",
                    "arguments": {"datasource_id": datasource_id},
                    "expected_output": "datasource readiness and capabilities",
                },
                {
                    "phase": "infer_schema",
                    "source_ref": source_ref,
                    "tool": "arcxa_infer_datasource_schema",
                    "arguments": {
                        "datasource_id": datasource_id,
                        "table_name": table_name,
                        "sample_size": source.get("sample_size", 100),
                        "enhanced": source.get("enhanced", True),
                    },
                    "expected_output": "source schema for mapping and validation",
                },
                {
                    "phase": "analyze_for_mapping",
                    "source_ref": source_ref,
                    "tool": "arcxa_analyze_datasource_for_mapping",
                    "arguments": {
                        "datasource_id": datasource_id,
                        "tables": [table_name] if table_name else None,
                        "sample_size": source.get("sample_size", 100),
                        "auto_approve_threshold": source.get(
                            "auto_approve_threshold", 0.95
                        ),
                        "min_confidence": source.get("min_confidence"),
                        "max_candidates": source.get("max_candidates"),
                        "ontology_namespaces": source.get("ontology_namespaces"),
                        "user_id": created_by,
                    },
                    "expected_output": "source mapping session_id for unified session creation",
                },
            ]
            source_session_preparation.extend(datasource_steps)
            dataset_preparation.extend(
                [
                    {
                        "phase": "materialize_dataset",
                        "source_ref": source_ref,
                        "tool": "arcxa_import_dataset_from_datasource",
                        "arguments": {
                            "source_id": datasource_id,
                            "table": table_name,
                            "schema": source.get("schema"),
                            "name": source.get("dataset_name") or source_ref,
                            "columns": source.get("columns", []),
                            "limit": source.get("limit"),
                            "profile": source.get("profile", True),
                            "async_mode": source.get("async_mode", False),
                        },
                        "expected_output": "dataset_id for the imported datasource-backed dataset",
                    },
                ]
            )
            source_session_inputs.append(
                {
                    "source_ref": source_ref,
                    "session_id": f"<source_mapping_session_id_from_{source_ref}_analysis>",
                }
            )
            dataset_inputs.append(
                {
                    "source_ref": source_ref,
                    "dataset_id": f"<dataset_id_from_{source_ref}_datasource_import>",
                }
            )
            continue

        raise ValueError(f"Unsupported unified mapping source kind: {kind}")

    executable_via_mcp = bool(provided_source_session_ids) or (
        not blocking_reasons and bool(source_session_inputs)
    )
    source_session_ids = provided_source_session_ids or [
        item["session_id"] for item in source_session_inputs
    ]

    load_arguments: Dict[str, Any] = {
        "session_id": "<session_id_from_unified_mapping>",
        "database_type": _normalize_database_type(
            target.get("database_type") or target.get("kind") or "d_b2"
        ),
        "create_tables": target.get("create_tables", True),
        "validate_data": target.get("validate_data", True),
        "batch_size": target.get("batch_size", 1000),
    }
    if target.get("connection_config") is not None:
        load_arguments["connection_config"] = deepcopy(target.get("connection_config"))

    return {
        "name": spec.get("name"),
        "recommended_surface": "unified_mapping",
        "reason": (
            "This plan requires multi-source consolidation. The current enterprise-safe path "
            "is to reconcile managed source mapping sessions through unified mapping, then load "
            "into the target system. Datasources and managed parquet-backed datasets can both "
            "feed executable source sessions through MCP."
        ),
        "execution_readiness": {
            "can_execute_end_to_end_via_mcp": executable_via_mcp,
            "status": "ready" if executable_via_mcp else "partial",
            "blocking_reasons": blocking_reasons,
            "required_source_sessions": source_session_inputs,
        },
        "dataset_preparation": dataset_preparation,
        "source_session_preparation": source_session_preparation,
        "mapping_execution": [
            {
                "phase": "suggest_mappings",
                "tool": "arcxa_suggest_field_mappings",
                "arguments": {"datasets": deepcopy(dataset_inputs)},
                "expected_output": "field-level similarity suggestions across input datasets",
            },
            {
                "phase": "create_unified_session",
                "tool": "arcxa_create_unified_mapping_session",
                "arguments": {
                    "source_session_ids": source_session_ids,
                    "target_database": deepcopy(target),
                    "created_by": created_by,
                },
                "expected_output": "session_id for unified schema review and load",
                "notes": [
                    "Create or look up source mapping sessions before this step if they do not already exist.",
                    "Resolve field conflicts before loading into the target system.",
                ],
            },
            {
                "phase": "load_unified_session",
                "tool": "arcxa_load_unified_mapping_session",
                "arguments": load_arguments,
                "expected_output": "load job status or accepted load request",
            },
        ],
        "verification": [
            {
                "phase": "inspect_load_job",
                "tool": "arcxa_get_mapping_load_job",
                "arguments": {"job_id": "<load_job_id_from_previous_step>"},
                "expected_output": "load job completion state and counters",
            },
            {
                "phase": "inspect_runtime_lineage",
                "tool": "arcxa_get_run_lineage",
                "arguments": {"run_id": "<execution_or_load_run_id>"},
                "expected_output": "run lineage and step-level provenance",
            },
        ],
        "notes": [
            "For parquet + Oracle -> DB2, import the local parquet file as a managed dataset first, then analyze that dataset for mapping.",
            "Do not rely on direct workflow data_joiner execution for production-grade multi-source fusion yet.",
        ],
    }


def plan_data_integration(spec: Dict[str, Any]) -> Dict[str, Any]:
    """Return a complete agent-facing plan for ETL or multi-source integration."""
    recommendation = recommend_execution_surface(spec)
    plan: Dict[str, Any] = {"recommendation": recommendation}

    if recommendation["safe_for_direct_workflow_execution"]:
        workflow_request = build_etl_workflow_definition(spec)
        plan["workflow_candidate"] = workflow_request
        plan["next_actions"] = [
            {
                "tool": "arcxa_validate_workflow",
                "arguments": {"workflow": workflow_request},
                "purpose": "validate the generated workflow before creation",
            },
            {
                "tool": "arcxa_create_workflow",
                "arguments": {"workflow": workflow_request},
                "purpose": "register the workflow if validation is clean",
            },
        ]
        return plan

    plan["unified_mapping_plan"] = build_unified_mapping_plan(spec)
    return plan
