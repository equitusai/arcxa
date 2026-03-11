"""R2RML mapping API for relational-to-RDF conversion."""

from typing import Any, Dict, Optional


class R2rmlAPI:
    """Manage R2RML mappings for converting relational data to RDF."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/r2rml/mappings"

    def list(
        self,
        limit: int = 50,
        offset: int = 0,
    ) -> Dict[str, Any]:
        """List all R2RML mappings."""
        params = {"limit": limit, "offset": offset}
        return self._client.get(self._base, params=params)

    def get(self, mapping_id: str) -> Dict[str, Any]:
        """Get R2RML mapping by ID."""
        return self._client.get(f"{self._base}/{mapping_id}")

    def create(self, mapping: Dict[str, Any]) -> Dict[str, Any]:
        """Create a new R2RML mapping.

        Args:
            mapping: R2RML mapping definition with triples maps
        """
        return self._client.post(self._base, json=mapping)

    def update(self, mapping_id: str, mapping: Dict[str, Any]) -> Dict[str, Any]:
        """Update an existing R2RML mapping."""
        return self._client.put(f"{self._base}/{mapping_id}", json=mapping)

    def delete(self, mapping_id: str) -> None:
        """Delete an R2RML mapping."""
        self._client.delete(f"{self._base}/{mapping_id}")

    def validate(self, mapping: Dict[str, Any]) -> Dict[str, Any]:
        """Validate R2RML mapping without creating.

        Returns validation errors and warnings.
        """
        return self._client.post(f"{self._base}/validate", json=mapping)

    def suggest(
        self,
        database_config: Dict[str, Any],
        table_name: str,
        ontology_id: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Suggest R2RML mapping based on table schema.

        Uses AI to suggest triples maps based on table structure and ontology.

        Args:
            database_config: Database connection details
            table_name: Table to generate mapping for
            ontology_id: Target ontology for mapping suggestions
        """
        data = {
            "database_config": database_config,
            "table_name": table_name,
        }
        if ontology_id:
            data["ontology_id"] = ontology_id
        return self._client.post(f"{self._base}/suggest", json=data)

    def execute(
        self,
        mapping_id: str,
        database_config: Dict[str, Any],
        output_format: str = "turtle",
        limit: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Execute R2RML mapping to generate RDF.

        Args:
            mapping_id: Mapping to execute
            database_config: Source database connection
            output_format: "turtle", "ntriples", "rdfxml", or "jsonld"
            limit: Max rows to process (for testing)
        """
        data: Dict[str, Any] = {
            "database_config": database_config,
            "output_format": output_format,
        }
        if limit:
            data["limit"] = limit
        return self._client.post(f"{self._base}/{mapping_id}/execute", json=data)
