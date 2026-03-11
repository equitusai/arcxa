"""Data lineage tracking API."""

from typing import Any, Dict, List, Optional
from urllib.parse import quote


class LineageAPI:
    """Track row-level, column-level, and schema lineage."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/lineage"

    # Row-level lineage
    def get_row(self, row_key: str) -> Dict[str, Any]:
        """Get lineage for a specific row."""
        # URL-encode the row_key to handle slashes and colons in file paths
        encoded_key = quote(row_key, safe='')
        return self._client.get(f"{self._base}/row/{encoded_key}")

    def get_row_journey(self, row_key: str) -> Dict[str, Any]:
        """Get complete transformation journey for a row."""
        # URL-encode the row_key to handle slashes and colons in file paths
        encoded_key = quote(row_key, safe='')
        return self._client.get(f"{self._base}/row/{encoded_key}/journey")

    def get_record(self, record_id: str) -> Dict[str, Any]:
        """Get lineage record by ID."""
        return self._client.get(f"{self._base}/record/{record_id}")

    def get_record_graph(self, record_id: str) -> Dict[str, Any]:
        """Get lineage graph for a record."""
        return self._client.get(f"{self._base}/record/{record_id}/graph")

    # Column-level lineage
    def get_column(self, table: str, column: str) -> Dict[str, Any]:
        """Get lineage for a specific column."""
        return self._client.get(f"{self._base}/column/{table}/{column}")

    def get_column_derived(self, table: str, column: str) -> Dict[str, Any]:
        """Get columns derived from this column."""
        return self._client.get(f"{self._base}/column/{table}/{column}/derived")

    def get_column_graph(self, table: str, column: str) -> Dict[str, Any]:
        """Get column lineage graph."""
        return self._client.get(f"{self._base}/column/{table}/{column}/graph")

    def impact_analysis(
        self,
        table: str,
        column: str,
        include_downstream: bool = True,
        include_upstream: bool = True,
    ) -> Dict[str, Any]:
        """Analyze impact of column changes.

        Find all dependent and source columns.
        """
        params = {
            "include_downstream": include_downstream,
            "include_upstream": include_upstream,
        }
        return self._client.post(
            f"{self._base}/column/impact-analysis",
            json={"table": table, "column": column},
            params=params,
        )

    # Job and run lineage
    def get_run(self, run_id: str) -> Dict[str, Any]:
        """Get lineage for a workflow run."""
        return self._client.get(f"{self._base}/run/{run_id}")

    def get_job_stats(self, job_id: str) -> Dict[str, Any]:
        """Get lineage statistics for a job."""
        return self._client.get(f"{self._base}/job/{job_id}/stats")

    def get_job_filtered(
        self,
        job_id: str,
        filters: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Get filtered lineage for a job."""
        return self._client.get(f"{self._base}/job/{job_id}/filtered", params=filters)

    def get_batch(self, batch_id: str) -> Dict[str, Any]:
        """Get lineage for a batch."""
        return self._client.get(f"{self._base}/batch/{batch_id}")

    # Time-based queries
    def time_range(
        self,
        start_time: str,
        end_time: str,
        filters: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Query lineage within a time range.

        Args:
            start_time: ISO 8601 timestamp
            end_time: ISO 8601 timestamp
            filters: Additional filters
        """
        params = {"start_time": start_time, "end_time": end_time}
        if filters:
            params.update(filters)
        return self._client.get(f"{self._base}/time-range", params=params)

    # Schema evolution
    def record_schema_change(
        self,
        datasource_id: str,
        table_name: str,
        change_type: str,
        changes: Dict[str, Any],
    ) -> Dict[str, Any]:
        """Record a schema change event.

        Args:
            datasource_id: Data source ID
            table_name: Table name
            change_type: Type of change (add_column, drop_column, etc.)
            changes: Change details
        """
        data = {
            "datasource_id": datasource_id,
            "table_name": table_name,
            "change_type": change_type,
            "changes": changes,
        }
        return self._client.post(f"{self._base}/schema/change", json=data)

    def get_schema_version(self, version_id: str) -> Dict[str, Any]:
        """Get schema version by ID."""
        return self._client.get(f"{self._base}/schema/version", params={"version_id": version_id})

    def get_schema_changes(
        self,
        datasource_id: str,
        table_name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Get schema change history for a datasource/table."""
        if table_name:
            return self._client.get(
                f"{self._base}/schema/datasource/{datasource_id}/table/{table_name}/changes"
            )
        return self._client.get(f"{self._base}/schema/datasource/{datasource_id}/changes")

    def get_latest_schema(self, datasource_id: str) -> Dict[str, Any]:
        """Get latest schema version for a datasource."""
        return self._client.get(f"{self._base}/schema/datasource/{datasource_id}/version/latest")

    def schema_drift(self, source_version: str, target_version: str) -> Dict[str, Any]:
        """Compare two schema versions for drift."""
        return self._client.get(f"{self._base}/schema/drift/{source_version}/{target_version}")

    def schema_impact(self, changes: List[Dict[str, Any]]) -> Dict[str, Any]:
        """Analyze impact of proposed schema changes."""
        return self._client.post(f"{self._base}/schema/impact", json={"changes": changes})

    # ML model lineage
    def model_impact(self, model_id: str) -> Dict[str, Any]:
        """Get impact analysis for an ML model."""
        return self._client.get(f"{self._base}/model/{model_id}/impact")
