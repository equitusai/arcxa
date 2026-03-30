from graphica.api.mapping import MappingAPI


class StubClient:
    def __init__(self):
        self.calls = []

    def get(self, path, **kwargs):
        self.calls.append(("GET", path, kwargs))
        return {"ok": True, "path": path, "kwargs": kwargs}

    def post(self, path, **kwargs):
        self.calls.append(("POST", path, kwargs))
        return {"ok": True, "path": path, "kwargs": kwargs}

    def put(self, path, **kwargs):
        self.calls.append(("PUT", path, kwargs))
        return {"ok": True, "path": path, "kwargs": kwargs}

    def delete(self, path, **kwargs):
        self.calls.append(("DELETE", path, kwargs))
        return {"ok": True, "path": path, "kwargs": kwargs}


def test_analyze_datasource_for_mapping_uses_datasource_route():
    client = StubClient()
    api = MappingAPI(client)

    api.analyze_datasource_for_mapping(
        "urn:graphica:datasource:oracle",
        user_id="agent",
        tables=["CUSTOMERS"],
        sample_size=100,
    )

    assert client.calls[0][0] == "POST"
    assert (
        client.calls[0][1]
        == "/api/v1/datasources/urn:graphica:datasource:oracle/analyze-for-mapping"
    )
    assert client.calls[0][2]["json"]["user_id"] == "agent"
    assert client.calls[0][2]["json"]["tables"] == ["CUSTOMERS"]


def test_analyze_dataset_for_mapping_uses_dataset_route():
    client = StubClient()
    api = MappingAPI(client)

    api.analyze_dataset_for_mapping(
        "ds_import_123",
        user_id="agent",
        tables=["customers_file"],
        sample_size=25,
    )

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/datasets/ds_import_123/analyze-for-mapping"
    assert client.calls[0][2]["json"]["user_id"] == "agent"
    assert client.calls[0][2]["json"]["tables"] == ["customers_file"]


def test_review_source_session_uses_session_review_route():
    client = StubClient()
    api = MappingAPI(client)

    api.review_source_session(
        "sess-123",
        field_mappings=[{"field_id": "f1", "action": "approve"}],
        reviewed_by="agent",
        finalize=True,
    )

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/mapping/sessions/sess-123/review"
    assert client.calls[0][2]["json"]["reviewed_by"] == "agent"
    assert client.calls[0][2]["json"]["finalize"] is True


def test_apply_and_import_source_session_use_session_routes():
    client = StubClient()
    api = MappingAPI(client)

    api.apply_source_session("sess-123", create_default_import=True)
    api.import_source_session(
        "sess-123",
        user_id="agent",
        batch_size=500,
        limit=10,
    )

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/mapping/sessions/sess-123/apply"
    assert client.calls[0][2]["json"] == {"create_default_import": True}

    assert client.calls[1][0] == "POST"
    assert client.calls[1][1] == "/api/v1/mapping/sessions/sess-123/import"
    assert client.calls[1][2]["json"]["user_id"] == "agent"
    assert client.calls[1][2]["json"]["batch_size"] == 500
    assert client.calls[1][2]["json"]["limit"] == 10


def test_load_to_database_omits_connection_config_when_not_needed():
    client = StubClient()
    api = MappingAPI(client)

    api.load_to_database(
        "unified_123",
        database_type="postgre_sql",
        create_tables=False,
        validate_data=True,
        batch_size=250,
    )

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/mapping/unified-sessions/unified_123/load"
    assert client.calls[0][2]["json"] == {
        "database_type": "postgre_sql",
        "create_tables": False,
        "validate_data": True,
        "batch_size": 250,
    }
