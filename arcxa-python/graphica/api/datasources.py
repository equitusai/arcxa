"""Datasource catalog API."""

from typing import Any, Dict, List, Optional


class DatasourcesAPI:
    """Manage datasource registration, testing, schema inference, and querying."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/datasources"

    def list(
        self,
        source_type: Optional[str] = None,
        tags: Optional[List[str]] = None,
        status: Optional[str] = None,
        page: int = 0,
        page_size: int = 50,
    ) -> Dict[str, Any]:
        params: Dict[str, Any] = {"page": page, "pageSize": page_size}
        if source_type:
            params["sourceType"] = source_type
        if status:
            params["status"] = status
        if tags:
            params["tags"] = tags
        return self._client.get(self._base, params=params)

    def get(self, datasource_id: str) -> Dict[str, Any]:
        return self._client.get(f"{self._base}/{datasource_id}")

    def create(self, datasource: Dict[str, Any]) -> Dict[str, Any]:
        return self._client.post(self._base, json=datasource)

    def update(self, datasource_id: str, updates: Dict[str, Any]) -> Dict[str, Any]:
        return self._client.put(f"{self._base}/{datasource_id}", json=updates)

    def delete(self, datasource_id: str) -> None:
        self._client.delete(f"{self._base}/{datasource_id}")

    def test_connection(
        self,
        source_id: Optional[str] = None,
        source_type: Optional[str] = None,
        connection: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        payload: Dict[str, Any] = {}
        if source_id is not None:
            payload["sourceId"] = source_id
        if source_type is not None:
            payload["sourceType"] = source_type
        if connection is not None:
            payload["connection"] = connection
        return self._client.post(f"{self._base}/test", json=payload)

    def infer_schema(
        self,
        datasource_id: str,
        table_name: Optional[str] = None,
        sample_size: int = 100,
        enhanced: bool = False,
    ) -> Dict[str, Any]:
        path = f"{self._base}/{datasource_id}/schema/infer"
        if enhanced:
            path = f"{path}-enhanced"
        return self._client.post(
            path,
            json={"sourceId": datasource_id, "tableName": table_name, "sampleSize": sample_size},
        )

    def query(
        self,
        datasource_id: str,
        query: str,
        parameters: Optional[Dict[str, Any]] = None,
        limit: Optional[int] = None,
    ) -> Dict[str, Any]:
        payload: Dict[str, Any] = {"sourceId": datasource_id, "query": query}
        if parameters:
            payload["parameters"] = parameters
        if limit is not None:
            payload["limit"] = limit
        return self._client.post(f"{self._base}/{datasource_id}/query", json=payload)
