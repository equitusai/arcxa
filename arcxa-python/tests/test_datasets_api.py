import json

from graphica.api.datasets import DatasetsAPI


class StubClient:
    def __init__(self):
        self.calls = []

    def get(self, path, **kwargs):
        self.calls.append(("GET", path, kwargs))
        return {"ok": True, "path": path, "kwargs": kwargs}

    def post(self, path, **kwargs):
        self.calls.append(("POST", path, kwargs))
        return {"ok": True, "path": path, "kwargs": kwargs}


def test_list_datasets_builds_expected_query_params():
    client = StubClient()
    api = DatasetsAPI(client)

    response = api.list(dataset_type="materialized", dataset_scope="all", page=1, page_size=25)

    assert response["ok"] is True
    assert client.calls[0][0] == "GET"
    assert client.calls[0][1] == "/api/v1/datasets"
    assert client.calls[0][2]["params"] == {
        "dataset_type": "materialized",
        "dataset_scope": "all",
        "page": 1,
        "page_size": 25,
    }


def test_import_dataset_file_uses_multipart_metadata(tmp_path):
    client = StubClient()
    api = DatasetsAPI(client)
    parquet_file = tmp_path / "customers.parquet"
    parquet_file.write_bytes(b"PAR1")

    api.import_file(
        file_path=str(parquet_file),
        name="customers",
        description="customer extract",
        tags=["oracle", "finance"],
    )

    call = client.calls[0]
    assert call[0] == "POST"
    assert call[1] == "/api/v1/datasets/import"
    assert call[2]["files"]["file"][0] == "customers.parquet"
    assert json.loads(call[2]["data"]["metadata"]) == {
        "name": "customers",
        "description": "customer extract",
        "tags": ["oracle", "finance"],
    }


def test_import_from_datasource_builds_expected_payload():
    client = StubClient()
    api = DatasetsAPI(client)

    api.import_from_datasource(
        source_id="urn:graphica:datasource:oracle",
        table="CUSTOMERS",
        schema="HR",
        name="oracle-customers",
        columns=["CUSTOMER_ID", "CUSTOMER_NAME"],
        limit=1000,
        profile=True,
        async_mode=True,
    )

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/datasets/import-datasource"
    assert client.calls[0][2]["json"] == {
        "source_id": "urn:graphica:datasource:oracle",
        "table": "CUSTOMERS",
        "schema": "HR",
        "name": "oracle-customers",
        "columns": ["CUSTOMER_ID", "CUSTOMER_NAME"],
        "limit": 1000,
        "tags": [],
        "profile": True,
        "async_mode": True,
    }
