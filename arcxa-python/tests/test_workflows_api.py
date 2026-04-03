from graphica.api.workflows import WorkflowsAPI
from graphica.errors import NotFoundError


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


def test_execute_uses_execute_async_route_for_async_mode():
    client = StubClient()
    api = WorkflowsAPI(client)

    api.execute("wf-123", inputs={"customer_id": "C001"}, async_mode=True)

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/workflows/wf-123/execute-async"
    assert client.calls[0][2]["json"] == {"input": {"customer_id": "C001"}}


def test_execute_falls_back_to_sync_when_async_surface_misses_workflow():
    class AsyncFallbackClient(StubClient):
        def post(self, path, **kwargs):
            self.calls.append(("POST", path, kwargs))
            if path.endswith("/execute-async"):
                raise NotFoundError("Workflow 'wf-123' not found")
            return {
                "workflow_id": "wf-123",
                "results": [{"execution_id": "exec-123", "success": True}],
                "started_at": "2026-04-02T19:00:00Z",
                "completed_at": "2026-04-02T19:00:01Z",
            }

    client = AsyncFallbackClient()
    api = WorkflowsAPI(client)

    result = api.execute("wf-123", inputs={"customer_id": "C001"}, async_mode=True)

    assert client.calls[0][1] == "/api/v1/workflows/wf-123/execute-async"
    assert client.calls[1][1] == "/api/v1/workflows/wf-123/execute"
    assert result["execution_id"] == "exec-123"
    assert result["workflow_id"] == "wf-123"
    assert result["fallback"] == "synchronous_execute"
    assert result["status"] == "completed"


def test_execute_uses_execute_route_for_sync_mode():
    client = StubClient()
    api = WorkflowsAPI(client)

    api.execute("wf-123", inputs=None, async_mode=False)

    assert client.calls[0][0] == "POST"
    assert client.calls[0][1] == "/api/v1/workflows/wf-123/execute"
    assert client.calls[0][2]["json"] == {"input": {}}


def test_get_execution_uses_modern_execution_detail_route():
    client = StubClient()
    api = WorkflowsAPI(client)

    api.get_execution("exec-123")

    assert client.calls[0][0] == "GET"
    assert client.calls[0][1] == "/api/v1/executions/exec-123"
