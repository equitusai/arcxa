"""Data loader API for ETL jobs."""

from typing import Any, Dict, List, Optional


class LoaderAPI:
    """Manage ETL loading jobs with checkpointing and DLQ."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/loader"

    def health(self) -> Dict[str, Any]:
        """Check loader service health."""
        return self._client.get(f"{self._base}/health")

    def list_jobs(
        self,
        status: Optional[str] = None,
        limit: int = 50,
        offset: int = 0,
    ) -> Dict[str, Any]:
        """List loader jobs.

        Args:
            status: Filter by status (running, completed, failed, etc.)
            limit: Page size
            offset: Pagination offset
        """
        params = {"limit": limit, "offset": offset}
        if status:
            params["status"] = status
        return self._client.get(f"{self._base}/jobs", params=params)

    def get_job(self, job_id: str) -> Dict[str, Any]:
        """Get job details and status."""
        return self._client.get(f"{self._base}/jobs/{job_id}")

    def resume_job(self, job_id: str) -> Dict[str, Any]:
        """Resume a paused or failed job from checkpoint."""
        return self._client.post(f"{self._base}/jobs/{job_id}/resume")

    def get_checkpoint(self, job_id: str) -> Dict[str, Any]:
        """Get job checkpoint for resumption."""
        return self._client.get(f"{self._base}/jobs/{job_id}/checkpoint")

    # Dead Letter Queue management
    def get_dlq(self, job_id: str) -> Dict[str, Any]:
        """Get DLQ statistics for a job."""
        return self._client.get(f"{self._base}/jobs/{job_id}/dlq")

    def get_dlq_rows(
        self,
        job_id: str,
        limit: int = 100,
        offset: int = 0,
    ) -> Dict[str, Any]:
        """Get failed rows from DLQ.

        Returns rows that failed processing with error details.
        """
        params = {"limit": limit, "offset": offset}
        return self._client.get(f"{self._base}/jobs/{job_id}/dlq/rows", params=params)

    def reprocess_dlq(
        self,
        job_id: str,
        row_ids: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Reprocess failed rows from DLQ.

        Args:
            job_id: Job ID
            row_ids: Specific rows to reprocess. If None, reprocess all.
        """
        data = {}
        if row_ids:
            data["row_ids"] = row_ids
        return self._client.post(f"{self._base}/jobs/{job_id}/dlq/reprocess", json=data)
