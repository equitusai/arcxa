"""Ontology management API."""

from typing import Any, Dict, List, Optional


class OntologyAPI:
    """Manage RDF/Turtle ontologies."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/ontology"

    def list(self, active_only: bool = False) -> Dict[str, Any]:
        """List all ontologies.

        Args:
            active_only: Only return active ontologies
        """
        params = {"active_only": active_only} if active_only else None
        return self._client.get(self._base, params=params)

    def get(self, ontology_id: str) -> Dict[str, Any]:
        """Get ontology by ID."""
        return self._client.get(f"{self._base}/{ontology_id}")

    def register(
        self,
        ontology_id: str,
        name: str,
        content: str,
        description: Optional[str] = None,
        namespace: Optional[str] = None,
        version: Optional[str] = None,
        author: Optional[str] = None,
        tags: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Register a new ontology.

        Args:
            ontology_id: Unique identifier
            name: Human-readable name
            content: Turtle/RDF content
            description: Optional description
            namespace: Namespace URI (auto-detected if not provided)
            version: Version string
            author: Author/organization
            tags: Tags for categorization
        """
        data = {
            "id": ontology_id,
            "name": name,
            "content": content,
        }
        if description:
            data["description"] = description
        if namespace:
            data["namespace"] = namespace
        if version:
            data["version"] = version
        if author:
            data["author"] = author
        if tags:
            data["tags"] = tags

        return self._client.post(self._base, json=data)

    def update(
        self,
        ontology_id: str,
        content: str,
        name: Optional[str] = None,
        description: Optional[str] = None,
        version: Optional[str] = None,
        tags: Optional[List[str]] = None,
        active: Optional[bool] = None,
    ) -> Dict[str, Any]:
        """Update an existing ontology."""
        data: Dict[str, Any] = {"content": content}
        if name:
            data["name"] = name
        if description:
            data["description"] = description
        if version:
            data["version"] = version
        if tags:
            data["tags"] = tags
        if active is not None:
            data["active"] = active

        return self._client.put(f"{self._base}/{ontology_id}", json=data)

    def delete(self, ontology_id: str, permanent: bool = False) -> None:
        """Delete or deactivate an ontology.

        Args:
            ontology_id: Ontology to delete
            permanent: If True, permanently delete. Otherwise soft-delete.
        """
        params = {"permanent": permanent} if permanent else None
        self._client.delete(f"{self._base}/{ontology_id}", params=params)

    def activate(self, ontology_id: str) -> None:
        """Activate a deactivated ontology."""
        self._client.post(f"{self._base}/{ontology_id}/activate")

    def validate(self, content: str) -> Dict[str, Any]:
        """Validate ontology syntax without registering.

        Returns validation status and any errors/warnings.
        """
        return self._client.post(f"{self._base}/validate", json={"content": content})

    def merge(
        self,
        ontology_ids: Optional[List[str]] = None,
        include_base: bool = True,
        include_extensions: bool = True,
    ) -> Dict[str, Any]:
        """Get merged ontology from multiple sources.

        Args:
            ontology_ids: Specific IDs to merge. If empty, merges all active.
            include_base: Include base catalog ontology
            include_extensions: Include extended inference ontology
        """
        data = {
            "ontology_ids": ontology_ids or [],
            "include_base": include_base,
            "include_extensions": include_extensions,
        }
        return self._client.post(f"{self._base}/merge", json=data)

    def tree(
        self,
        ontology_id: str,
        max_depth: int = -1,
        include_properties: bool = True,
        include_individuals: bool = False,
    ) -> Dict[str, Any]:
        """Get ontology as hierarchical tree structure.

        Useful for visualization and exploring class hierarchies.
        """
        params = {
            "max_depth": max_depth,
            "include_properties": include_properties,
            "include_individuals": include_individuals,
        }
        return self._client.get(f"{self._base}/{ontology_id}/tree", params=params)
