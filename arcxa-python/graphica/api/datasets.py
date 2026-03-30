"""Dataset import and inspection API."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Dict, List, Optional


class DatasetsAPI:
    """Manage materialized datasets and dataset import jobs."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/datasets"

    def list(
        self,
        dataset_type: Optional[str] = None,
        dataset_scope: Optional[str] = None,
        page: int = 0,
        page_size: int = 50,
    ) -> Dict[str, Any]:
        """List datasets visible to the platform."""
        params: Dict[str, Any] = {"page": page, "page_size": page_size}
        if dataset_type:
            params["dataset_type"] = dataset_type
        if dataset_scope:
            params["dataset_scope"] = dataset_scope
        return self._client.get(self._base, params=params)

    def get(self, dataset_id: str) -> Dict[str, Any]:
        """Get a dataset by ID, including schema and lineage metadata."""
        return self._client.get(f"{self._base}/{dataset_id}")

    def import_file(
        self,
        file_path: str,
        name: Optional[str] = None,
        description: Optional[str] = None,
        tags: Optional[List[str]] = None,
        schema: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Import a local parquet/csv/json/jsonl file as a managed dataset."""
        path = Path(file_path)
        metadata: Dict[str, Any] = {
            "name": name or path.name,
            "description": description,
            "tags": tags or [],
            "schema": schema,
        }
        metadata = {key: value for key, value in metadata.items() if value is not None}

        with path.open("rb") as handle:
            return self._client.post(
                f"{self._base}/import",
                files={"file": (path.name, handle)},
                data={"metadata": json.dumps(metadata)},
            )

    def import_from_datasource(
        self,
        source_id: str,
        table: str,
        schema: Optional[str] = None,
        name: Optional[str] = None,
        columns: Optional[List[str]] = None,
        limit: Optional[int] = None,
        description: Optional[str] = None,
        tags: Optional[List[str]] = None,
        profile: bool = False,
        async_mode: bool = False,
        incremental: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Materialize a datasource table into a managed dataset."""
        payload: Dict[str, Any] = {
            "source_id": source_id,
            "table": table,
            "schema": schema,
            "name": name,
            "columns": columns or [],
            "limit": limit,
            "description": description,
            "tags": tags or [],
            "profile": profile,
            "async_mode": async_mode,
            "incremental": incremental,
        }
        payload = {key: value for key, value in payload.items() if value is not None}
        return self._client.post(f"{self._base}/import-datasource", json=payload)

    def batch_import_from_datasource(
        self,
        source_id: str,
        tables: List[Dict[str, Any]],
        tags: Optional[List[str]] = None,
        profile: bool = False,
    ) -> Dict[str, Any]:
        """Queue a batch datasource import."""
        payload = {
            "source_id": source_id,
            "tables": tables,
            "tags": tags or [],
            "profile": profile,
        }
        return self._client.post(f"{self._base}/import-batch", json=payload)

    def get_import_status(self, import_id: str) -> Dict[str, Any]:
        """Get one dataset import job status."""
        return self._client.get(f"{self._base}/imports/{import_id}")

    def list_imports(
        self,
        status: Optional[str] = None,
        page: int = 0,
        page_size: int = 50,
    ) -> Dict[str, Any]:
        """List dataset import jobs."""
        params: Dict[str, Any] = {"page": page, "page_size": page_size}
        if status:
            params["status"] = status
        return self._client.get(f"{self._base}/imports", params=params)
