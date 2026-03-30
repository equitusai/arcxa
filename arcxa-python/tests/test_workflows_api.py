from graphica.api.workflows import WorkflowsAPI


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
