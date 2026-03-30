from graphica.api.datasources import DatasourcesAPI


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


def test_list_datasources_builds_expected_query_params():
    client = StubClient()
    api = DatasourcesAPI(client)

    response = api.list(
        source_type="Oracle",
        status="active",
        tags=["etl", "finance"],
        page=2,
        page_size=25,
    )

    assert response["ok"] is True
    assert client.calls[0][0] == "GET"
    assert client.calls[0][1] == "/api/v1/datasources"
    assert client.calls[0][2]["params"] == {
        "sourceType": "Oracle",
        "status": "active",
        "tags": ["etl", "finance"],
        "page": 2,
        "pageSize": 25,
    }


def test_infer_schema_uses_enhanced_endpoint_when_requested():
    client = StubClient()
    api = DatasourcesAPI(client)

    api.infer_schema("ds-123", table_name="CUSTOMERS", sample_size=250, enhanced=True)

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/datasources/ds-123/schema/infer-enhanced"
    assert client.calls[0][2]["json"] == {
        "sourceId": "ds-123",
        "tableName": "CUSTOMERS",
        "sampleSize": 250,
    }


def test_query_includes_source_id_in_request_body():
    client = StubClient()
    api = DatasourcesAPI(client)

    api.query(
        "ds-123",
        "SELECT * FROM public.customers ORDER BY customer_id",
        parameters={"segment": "gold"},
        limit=25,
    )

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/datasources/ds-123/query"
    assert client.calls[0][2]["json"] == {
        "sourceId": "ds-123",
        "query": "SELECT * FROM public.customers ORDER BY customer_id",
        "parameters": {"segment": "gold"},
        "limit": 25,
    }
