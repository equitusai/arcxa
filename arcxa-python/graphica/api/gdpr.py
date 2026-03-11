"""GDPR compliance API (Articles 17 and 20)."""

from typing import Any, Dict, List, Optional


class GdprAPI:
    """GDPR data erasure, export, and compliance operations."""

    def __init__(self, client: Any):
        self._client = client
        self._base = "/api/v1/gdpr"

    # Tenant-level operations
    def count_tenant_data(self, tenant_id: str) -> Dict[str, Any]:
        """Count data records for a tenant."""
        return self._client.get(f"{self._base}/tenants/{tenant_id}/count")

    def erase_tenant(
        self,
        tenant_id: str,
        dry_run: bool = False,
    ) -> Dict[str, Any]:
        """Erase all data for a tenant (Article 17).

        Args:
            tenant_id: Tenant to erase
            dry_run: If True, simulate without deleting
        """
        data = {"dry_run": dry_run}
        return self._client.post(f"{self._base}/tenants/{tenant_id}/erase", json=data)

    def verify_erasure(self, tenant_id: str) -> Dict[str, Any]:
        """Verify that tenant data has been erased."""
        return self._client.get(f"{self._base}/tenants/{tenant_id}/verify")

    # User-level operations
    def count_user_data(self, user_id: str) -> Dict[str, Any]:
        """Count data records for a user."""
        return self._client.get(f"{self._base}/users/{user_id}/count")

    def erase_user(
        self,
        user_id: str,
        strategy: str = "hard_delete",
        dry_run: bool = False,
        categories: Optional[List[str]] = None,
        skip_retention_check: bool = False,
    ) -> Dict[str, Any]:
        """Erase all data for a user (Article 17).

        Args:
            user_id: User to erase
            strategy: "hard_delete", "anonymize", "tombstone", or "archive_then_delete"
            dry_run: If True, simulate without deleting
            categories: Specific data categories to erase
            skip_retention_check: Skip retention policy validation
        """
        data: Dict[str, Any] = {
            "strategy": strategy,
            "dry_run": dry_run,
            "skip_retention_check": skip_retention_check,
        }
        if categories:
            data["categories"] = categories
        return self._client.post(f"{self._base}/users/{user_id}/erase", json=data)

    def check_legal_holds(self, user_id: str) -> Dict[str, Any]:
        """Check if user has any active legal holds.

        Legal holds prevent data deletion for litigation purposes.
        """
        return self._client.get(f"{self._base}/users/{user_id}/legal-holds")

    # Data export (Article 20 - Data Portability)
    def export_data(
        self,
        user_id: str,
        format: str = "json",
        categories: Optional[List[str]] = None,
        include_metadata: bool = True,
    ) -> Dict[str, Any]:
        """Request data export for a user (Article 20).

        Args:
            user_id: User to export data for
            format: "json", "csv", or "parquet"
            categories: Specific data categories to export
            include_metadata: Include lineage and processing metadata

        Returns:
            Export job info with job_id for tracking
        """
        data: Dict[str, Any] = {
            "user_id": user_id,
            "format": format,
            "include_metadata": include_metadata,
        }
        if categories:
            data["categories"] = categories
        return self._client.post(f"{self._base}/exports", json=data)

    def get_export_status(self, job_id: str) -> Dict[str, Any]:
        """Get export job status and progress."""
        return self._client.get(f"{self._base}/exports/{job_id}")

    def download_export(self, job_id: str) -> bytes:
        """Download completed export file.

        Returns raw file bytes. Write to file or process as needed.
        """
        return self._client.get(f"{self._base}/exports/{job_id}/download")

    def list_exports(
        self,
        user_id: Optional[str] = None,
        status: Optional[str] = None,
        limit: int = 50,
        offset: int = 0,
    ) -> Dict[str, Any]:
        """List export jobs.

        Args:
            user_id: Filter by user
            status: Filter by status (pending, processing, completed, failed)
            limit: Page size
            offset: Pagination offset
        """
        params = {"limit": limit, "offset": offset}
        if user_id:
            params["user_id"] = user_id
        if status:
            params["status"] = status
        return self._client.get(f"{self._base}/exports", params=params)
