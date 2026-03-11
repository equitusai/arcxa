"""Unified mapping API for consolidating multiple data sources."""

from typing import Any, Dict, List, Optional


class MappingAPI:
    """Manage unified field mappings across multiple CSV sources."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/mapping"

    def suggest(self, datasets: List[Dict[str, Any]]) -> Dict[str, Any]:
        """Suggest field mappings using AI/ML similarity analysis.

        Analyzes datasets and returns auto-mapped, recommended, and possible matches.

        Args:
            datasets: List of dataset schemas or IDs to analyze
        """
        return self._client.post(f"{self._base}/suggest", json={"datasets": datasets})

    def list_sessions(
        self,
        status: Optional[str] = None,
        created_by: Optional[str] = None,
        offset: int = 0,
        limit: int = 50,
    ) -> Dict[str, Any]:
        """List unified mapping sessions.

        Args:
            status: Filter by status (e.g., "draft", "ready_to_load", "loaded")
            created_by: Filter by creator
            offset: Pagination offset
            limit: Page size
        """
        params = {"offset": offset, "limit": limit}
        if status:
            params["status"] = status
        if created_by:
            params["created_by"] = created_by

        return self._client.get(f"{self._base}/unified-sessions", params=params)

    def get_session(self, session_id: str) -> Dict[str, Any]:
        """Get unified session by ID."""
        return self._client.get(f"{self._base}/unified-sessions/{session_id}")

    def create_session(
        self,
        source_session_ids: List[str],
        target_database: Dict[str, Any],
        created_by: str,
    ) -> Dict[str, Any]:
        """Create new unified mapping session.

        Consolidates multiple source sessions into a single unified schema.

        Args:
            source_session_ids: IDs of source mapping sessions
            target_database: Target database configuration
            created_by: Username creating the session
        """
        data = {
            "source_session_ids": source_session_ids,
            "target_database": target_database,
            "created_by": created_by,
        }
        return self._client.post(f"{self._base}/unified-sessions", json=data)

    def update_session(
        self,
        session_id: str,
        target_database: Optional[Dict[str, Any]] = None,
        field_mappings: Optional[List[Dict[str, Any]]] = None,
    ) -> Dict[str, Any]:
        """Update unified session mappings or target database."""
        data: Dict[str, Any] = {}
        if target_database:
            data["target_database"] = target_database
        if field_mappings:
            data["field_mappings"] = field_mappings

        return self._client.put(f"{self._base}/unified-sessions/{session_id}", json=data)

    def delete_session(self, session_id: str) -> None:
        """Delete unified session permanently."""
        self._client.delete(f"{self._base}/unified-sessions/{session_id}")

    def resolve_conflicts(
        self,
        session_id: str,
        resolutions: Dict[str, Dict[str, Any]],
    ) -> Dict[str, Any]:
        """Resolve field mapping conflicts.

        Args:
            session_id: Session to resolve conflicts for
            resolutions: Map of conflict_id to resolution strategy

        Example:
            client.mapping.resolve_conflicts("sess-123", {
                "conflict-1": {"strategy": "use_primary", "parameters": {"primary_source": "src-a"}},
                "conflict-2": {"strategy": "coalesce"}
            })
        """
        return self._client.post(
            f"{self._base}/unified-sessions/{session_id}/resolve-conflicts",
            json={"resolutions": resolutions},
        )

    def load_to_database(
        self,
        session_id: str,
        database_type: str,
        connection_config: Dict[str, Any],
        create_tables: bool = True,
        validate_data: bool = True,
        batch_size: int = 1000,
    ) -> Dict[str, Any]:
        """Load unified session data to target database.

        Args:
            session_id: Session to load
            database_type: "postgre_s_q_l", "d_b2", or "oracle"
            connection_config: Database connection details
            create_tables: Create tables if they don't exist
            validate_data: Validate data before loading
            batch_size: Batch size for bulk loading
        """
        data = {
            "database_type": database_type,
            "connection_config": connection_config,
            "create_tables": create_tables,
            "validate_data": validate_data,
            "batch_size": batch_size,
        }
        return self._client.post(
            f"{self._base}/unified-sessions/{session_id}/load",
            json=data,
        )

    def get_load_job_status(self, job_id: str) -> Dict[str, Any]:
        """Get load job progress and status."""
        return self._client.get(f"{self._base}/load-jobs/{job_id}")

    def statistics(self) -> Dict[str, Any]:
        """Get global statistics for all unified sessions."""
        return self._client.get(f"{self._base}/unified-sessions/statistics")
