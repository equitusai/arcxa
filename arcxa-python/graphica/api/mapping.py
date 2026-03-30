"""Mapping APIs for source analysis and unified consolidation."""

from typing import Any, Dict, List, Optional


class MappingAPI:
    """Manage source mapping sessions and unified multi-source mappings."""

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
        connection_config: Optional[Dict[str, Any]] = None,
        create_tables: bool = True,
        validate_data: bool = True,
        batch_size: int = 1000,
    ) -> Dict[str, Any]:
        """Load unified session data to target database.

        Args:
            session_id: Session to load
            database_type: "postgre_s_q_l", "d_b2", or "oracle"
            connection_config: Database connection details when required by the target backend
            create_tables: Create tables if they don't exist
            validate_data: Validate data before loading
            batch_size: Batch size for bulk loading
        """
        data = {
            "database_type": database_type,
            "create_tables": create_tables,
            "validate_data": validate_data,
            "batch_size": batch_size,
        }
        if connection_config is not None:
            data["connection_config"] = connection_config
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

    def analyze_datasource_for_mapping(
        self,
        source_id: str,
        *,
        user_id: str,
        tables: Optional[List[str]] = None,
        sample_size: Optional[int] = None,
        auto_approve_threshold: Optional[float] = None,
        min_confidence: Optional[float] = None,
        max_candidates: Optional[int] = None,
        ontology_namespaces: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Analyze one datasource and create a source mapping session."""
        data: Dict[str, Any] = {"user_id": user_id}
        if tables is not None:
            data["tables"] = tables
        if sample_size is not None:
            data["sample_size"] = sample_size
        if auto_approve_threshold is not None:
            data["auto_approve_threshold"] = auto_approve_threshold
        if min_confidence is not None:
            data["min_confidence"] = min_confidence
        if max_candidates is not None:
            data["max_candidates"] = max_candidates
        if ontology_namespaces is not None:
            data["ontology_namespaces"] = ontology_namespaces
        return self._client.post(
            f"/api/v1/datasources/{source_id}/analyze-for-mapping",
            json=data,
        )

    def analyze_dataset_for_mapping(
        self,
        dataset_id: str,
        *,
        user_id: str,
        tables: Optional[List[str]] = None,
        sample_size: Optional[int] = None,
        auto_approve_threshold: Optional[float] = None,
        min_confidence: Optional[float] = None,
        max_candidates: Optional[int] = None,
        ontology_namespaces: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Analyze one managed dataset and create a source mapping session."""
        data: Dict[str, Any] = {"user_id": user_id}
        if tables is not None:
            data["tables"] = tables
        if sample_size is not None:
            data["sample_size"] = sample_size
        if auto_approve_threshold is not None:
            data["auto_approve_threshold"] = auto_approve_threshold
        if min_confidence is not None:
            data["min_confidence"] = min_confidence
        if max_candidates is not None:
            data["max_candidates"] = max_candidates
        if ontology_namespaces is not None:
            data["ontology_namespaces"] = ontology_namespaces
        return self._client.post(
            f"/api/v1/datasets/{dataset_id}/analyze-for-mapping",
            json=data,
        )

    def get_source_session(self, session_id: str) -> Dict[str, Any]:
        """Get a datasource-backed source mapping session by ID."""
        return self._client.get(f"{self._base}/sessions/{session_id}")

    def review_source_session(
        self,
        session_id: str,
        *,
        field_mappings: List[Dict[str, Any]],
        reviewed_by: str,
        finalize: bool = False,
    ) -> Dict[str, Any]:
        """Review field mappings for a source mapping session."""
        return self._client.post(
            f"{self._base}/sessions/{session_id}/review",
            json={
                "field_mappings": field_mappings,
                "reviewed_by": reviewed_by,
                "finalize": finalize,
            },
        )

    def apply_source_session(
        self,
        session_id: str,
        *,
        create_default_import: bool = False,
    ) -> Dict[str, Any]:
        """Apply an approved source mapping session to the RDF/governance store."""
        return self._client.post(
            f"{self._base}/sessions/{session_id}/apply",
            json={"create_default_import": create_default_import},
        )

    def import_source_session(
        self,
        session_id: str,
        *,
        user_id: str,
        batch_size: int = 1000,
        target_graph: Optional[str] = None,
        tables: Optional[List[str]] = None,
        limit: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Import data using an applied source mapping session."""
        data: Dict[str, Any] = {
            "user_id": user_id,
            "batch_size": batch_size,
        }
        if target_graph is not None:
            data["target_graph"] = target_graph
        if tables is not None:
            data["tables"] = tables
        if limit is not None:
            data["limit"] = limit
        return self._client.post(
            f"{self._base}/sessions/{session_id}/import",
            json=data,
        )
