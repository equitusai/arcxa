"""Workflow management API."""

from typing import Any, Dict, List, Optional


class WorkflowsAPI:
    """Manage ETL workflows with scheduling and execution."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/workflows"

    def list(
        self,
        limit: int = 50,
        offset: int = 0,
    ) -> Dict[str, Any]:
        """List all workflows."""
        params = {"limit": limit, "offset": offset}
        return self._client.get(self._base, params=params)

    def get(self, workflow_id: str) -> Dict[str, Any]:
        """Get workflow by ID."""
        return self._client.get(f"{self._base}/{workflow_id}")

    def create(self, workflow: Dict[str, Any]) -> Dict[str, Any]:
        """Create a new workflow.

        Args:
            workflow: Workflow definition with steps, inputs, outputs
        """
        return self._client.post(self._base, json=workflow)

    def update(self, workflow_id: str, workflow: Dict[str, Any]) -> Dict[str, Any]:
        """Update an existing workflow."""
        return self._client.put(f"{self._base}/{workflow_id}", json=workflow)

    def delete(self, workflow_id: str) -> None:
        """Delete a workflow."""
        self._client.delete(f"{self._base}/{workflow_id}")

    def validate(self, workflow: Dict[str, Any]) -> Dict[str, Any]:
        """Validate workflow definition without creating.

        Returns validation errors and warnings.
        """
        return self._client.post(f"{self._base}/validate", json=workflow)

    def dry_run(
        self,
        workflow: Dict[str, Any],
        inputs: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Execute workflow in dry-run mode.

        Validates inputs and simulates execution without side effects.
        """
        data = {"workflow": workflow}
        if inputs:
            data["input"] = inputs  # Fixed: use "input" not "inputs"
        return self._client.post(f"{self._base}/dry-run", json=data)

    def test_step(
        self,
        step: Dict[str, Any],
        inputs: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Test a single workflow step.

        Execute one step with sample data for debugging.
        """
        data = {"step": step}
        if inputs:
            data["input"] = inputs  # Fixed: use "input" not "inputs"
        return self._client.post(f"{self._base}/test-step", json=data)

    def execute(
        self,
        workflow_id: str,
        inputs: Optional[Dict[str, Any]] = None,
        async_mode: bool = False,
    ) -> Dict[str, Any]:
        """Execute a workflow.

        Args:
            workflow_id: Workflow to execute
            inputs: Input parameters (if None or empty, sends empty object)
            async_mode: If True, return immediately with execution ID
        """
        # Note: Coordinator expects "input" (singular), not "inputs" (plural)
        # Always send input field with empty object if not provided (untagged enum needs explicit empty object)
        data: Dict[str, Any] = {
            "input": inputs if inputs else {}
        }

        # Use async endpoint if async_mode is True
        endpoint = f"{self._base}/{workflow_id}/execute"
        if async_mode:
            endpoint = f"{self._base}/{workflow_id}/execute/async"

        return self._client.post(endpoint, json=data)

    def list_executions(
        self,
        workflow_id: str,
        status: Optional[str] = None,
        limit: int = 50,
        offset: int = 0,
    ) -> Dict[str, Any]:
        """List workflow executions.

        Args:
            workflow_id: Workflow to get executions for
            status: Filter by status (running, completed, failed)
            limit: Page size
            offset: Pagination offset
        """
        params = {"limit": limit, "offset": offset}
        if status:
            params["status"] = status
        return self._client.get(f"{self._base}/{workflow_id}/executions", params=params)

    # Scheduling
    def create_schedule(
        self,
        workflow_id: str,
        cron: str,
        enabled: bool = True,
        inputs: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Create a schedule for a workflow.

        Args:
            workflow_id: Workflow to schedule
            cron: Cron expression (e.g., "0 * * * *" for hourly)
            enabled: Whether schedule is active
            inputs: Default inputs for scheduled runs
        """
        data = {"cron": cron, "enabled": enabled}
        if inputs:
            data["input"] = inputs  # Fixed: use "input" not "inputs"
        return self._client.post(f"{self._base}/{workflow_id}/schedule", json=data)

    def list_schedules(self, workflow_id: str) -> Dict[str, Any]:
        """List schedules for a workflow."""
        return self._client.get(f"{self._base}/{workflow_id}/schedules")

    def get_schedule(self, workflow_id: str, schedule_id: str) -> Dict[str, Any]:
        """Get schedule details."""
        return self._client.get(f"{self._base}/{workflow_id}/schedules/{schedule_id}")

    def update_schedule(
        self,
        workflow_id: str,
        schedule_id: str,
        cron: Optional[str] = None,
        enabled: Optional[bool] = None,
        inputs: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Update a schedule."""
        data: Dict[str, Any] = {}
        if cron:
            data["cron"] = cron
        if enabled is not None:
            data["enabled"] = enabled
        if inputs:
            data["input"] = inputs  # Fixed: use "input" not "inputs"
        return self._client.put(f"{self._base}/{workflow_id}/schedules/{schedule_id}", json=data)

    def delete_schedule(self, workflow_id: str, schedule_id: str) -> None:
        """Delete a schedule."""
        self._client.delete(f"{self._base}/{workflow_id}/schedules/{schedule_id}")
