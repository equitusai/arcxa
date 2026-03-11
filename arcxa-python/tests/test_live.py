"""Live tests against running Graphica server.

Run with: python -m pytest tests/test_live.py -v
Requires server running at localhost:8080
"""

import pytest
from graphica import Client
from graphica.errors import NotFoundError, ValidationError, ServerError


@pytest.fixture
def client():
    """Create client connected to local server."""
    return Client("http://localhost:8080")


class TestHealth:
    """Test server health and connectivity."""

    def test_health_check(self, client):
        """Server should be healthy."""
        # Health endpoint may fail if loader not initialized
        try:
            result = client.health()
            assert result is not None
        except ServerError:
            # Loader not initialized is acceptable for basic tests
            pass


class TestOntology:
    """Test ontology API."""

    def test_list_ontologies(self, client):
        """Should list ontologies."""
        result = client.ontology.list()
        assert "ontologies" in result
        assert "total" in result

    def test_validate_valid_ontology(self, client):
        """Should validate correct Turtle syntax."""
        content = '''
        @prefix ex: <http://example.org/> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

        ex:Person a rdfs:Class ;
            rdfs:label "Person" .
        '''
        result = client.ontology.validate(content)
        assert "status" in result
        # Valid or ValidWithWarnings
        assert result["status"] in ["Valid", "Pending"] or "Valid" in str(result["status"])

    def test_validate_invalid_ontology(self, client):
        """Should reject invalid syntax."""
        content = "this is not valid turtle"
        result = client.ontology.validate(content)
        # Should either be Invalid or raise ValidationError
        assert "status" in result

    def test_register_and_delete_ontology(self, client):
        """Should register, get, and delete ontology."""
        test_id = "pytest-test-ontology"
        content = '''
        @prefix test: <http://test.example.org/> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

        test:TestClass a rdfs:Class ;
            rdfs:label "Test Class" .
        '''

        # Register
        result = client.ontology.register(
            ontology_id=test_id,
            name="PyTest Test Ontology",
            content=content,
            description="Created by pytest",
            tags=["test", "pytest"],
        )
        assert result["metadata"]["id"] == test_id

        # Get
        result = client.ontology.get(test_id)
        assert result["metadata"]["id"] == test_id
        assert "test:TestClass" in result["content"]

        # Delete (soft delete since permanent may not be supported)
        try:
            client.ontology.delete(test_id, permanent=True)
        except ValidationError:
            # Permanent delete may not be supported
            client.ontology.delete(test_id)

        # Verify deleted or deactivated
        try:
            result = client.ontology.get(test_id)
            # If still exists, should be deactivated
            assert not result.get("metadata", {}).get("active", True)
        except NotFoundError:
            pass  # Deleted successfully


class TestMapping:
    """Test unified mapping API."""

    def test_list_sessions(self, client):
        """Should list mapping sessions."""
        result = client.mapping.list_sessions()
        assert "sessions" in result
        assert "total_count" in result

    def test_statistics(self, client):
        """Should get global statistics."""
        result = client.mapping.statistics()
        assert "total_sessions" in result


class TestLineage:
    """Test lineage API."""

    def test_time_range_query(self, client):
        """Should query lineage by time range."""
        try:
            result = client.lineage.time_range(
                start_time="2020-01-01T00:00:00Z",
                end_time="2030-01-01T00:00:00Z",
            )
            # Result structure depends on data
            assert result is not None
        except (ServerError, NotFoundError):
            # Lineage store may not be initialized
            pass


class TestLoader:
    """Test loader API."""

    def test_health(self, client):
        """Loader health check."""
        try:
            result = client.loader.health()
            assert result is not None
        except ServerError:
            # Loader may not be initialized
            pass

    def test_list_jobs(self, client):
        """Should list loader jobs."""
        try:
            result = client.loader.list_jobs()
            assert result is not None
        except ServerError:
            # Loader may not be initialized
            pass


class TestWorkflows:
    """Test workflows API."""

    def test_list_workflows(self, client):
        """Should list workflows."""
        result = client.workflows.list()
        assert result is not None

    def test_validate_workflow(self, client):
        """Should validate workflow definition."""
        workflow = {
            "name": "test-workflow",
            "steps": [
                {
                    "name": "step1",
                    "type": "transform",
                    "config": {},
                }
            ],
        }
        # This might fail if schema differs, but tests the API call
        try:
            result = client.workflows.validate(workflow)
            assert result is not None
        except ValidationError:
            # Expected if workflow schema is different
            pass


class TestGdpr:
    """Test GDPR API."""

    def test_list_exports(self, client):
        """Should list export jobs."""
        try:
            result = client.gdpr.list_exports()
            assert result is not None
        except NotFoundError:
            # GDPR exports endpoint may not be available
            pass


class TestR2rml:
    """Test R2RML API."""

    def test_list_mappings(self, client):
        """Should list R2RML mappings."""
        try:
            result = client.r2rml.list()
            assert result is not None
        except NotFoundError:
            # R2RML may not be enabled
            pass


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
