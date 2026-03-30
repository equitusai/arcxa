"""Main Graphica client."""

from typing import Any, Dict, Optional
import requests

from graphica.auth import Auth, NoAuth
from graphica.errors import (
    AuthError,
    NotFoundError,
    ValidationError,
    ConflictError,
    ServerError,
    ConnectionError,
)


class Client:
    """Graphica API client.

    Usage:
        client = Client("http://localhost:8080")
        ontologies = client.ontology.list()
    """

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        auth: Optional[Auth] = None,
        timeout: int = 30,
    ):
        self.base_url = base_url.rstrip("/")
        self.auth = auth or NoAuth()
        self.timeout = timeout
        self._session = requests.Session()

        # Import and attach API modules
        from graphica.api.ontology import OntologyAPI
        from graphica.api.mapping import MappingAPI
        from graphica.api.lineage import LineageAPI
        from graphica.api.loader import LoaderAPI
        from graphica.api.workflows import WorkflowsAPI
        from graphica.api.datasources import DatasourcesAPI
        from graphica.api.datasets import DatasetsAPI
        from graphica.api.gdpr import GdprAPI
        from graphica.api.r2rml import R2rmlAPI

        self.ontology = OntologyAPI(self)
        self.mapping = MappingAPI(self)
        self.lineage = LineageAPI(self)
        self.loader = LoaderAPI(self)
        self.workflows = WorkflowsAPI(self)
        self.datasources = DatasourcesAPI(self)
        self.datasets = DatasetsAPI(self)
        self.gdpr = GdprAPI(self)
        self.r2rml = R2rmlAPI(self)

    def _request(
        self,
        method: str,
        path: str,
        json: Optional[Dict[str, Any]] = None,
        params: Optional[Dict[str, Any]] = None,
        data: Any = None,
        files: Optional[Dict[str, Any]] = None,
    ) -> Any:
        """Make HTTP request to API."""
        url = f"{self.base_url}{path}"
        headers = {"Content-Type": "application/json", **self.auth.headers()}

        if files:
            # Remove content-type for multipart
            headers.pop("Content-Type", None)

        try:
            resp = self._session.request(
                method,
                url,
                json=json,
                params=params,
                data=data,
                files=files,
                headers=headers,
                timeout=self.timeout,
            )
        except requests.exceptions.ConnectionError as e:
            raise ConnectionError(f"Failed to connect to {url}: {e}")
        except requests.exceptions.Timeout as e:
            raise ConnectionError(f"Request timed out: {e}")

        return self._handle_response(resp)

    def _handle_response(self, resp: requests.Response) -> Any:
        """Process response, raise appropriate errors."""
        if resp.status_code == 204:
            return None

        if resp.status_code == 401:
            raise AuthError("Authentication failed")
        if resp.status_code == 403:
            raise AuthError("Permission denied")
        if resp.status_code == 404:
            raise NotFoundError(self._error_message(resp))
        if resp.status_code == 400:
            raise ValidationError(self._error_message(resp))
        if resp.status_code == 409:
            raise ConflictError(self._error_message(resp))
        if resp.status_code == 422:
            raise ValidationError(self._error_message(resp))
        if resp.status_code >= 500:
            raise ServerError(self._error_message(resp))
        if not resp.ok:
            raise ServerError(f"Request failed: {resp.status_code}")

        # Return raw content for non-JSON responses
        if "application/json" not in resp.headers.get("Content-Type", ""):
            return resp.content

        return resp.json()

    def _error_message(self, resp: requests.Response) -> str:
        """Extract error message from response."""
        try:
            data = resp.json()
            return data.get("error", data.get("message", str(data)))
        except Exception:
            return resp.text or f"HTTP {resp.status_code}"

    def get(self, path: str, **kwargs: Any) -> Any:
        return self._request("GET", path, **kwargs)

    def post(self, path: str, **kwargs: Any) -> Any:
        return self._request("POST", path, **kwargs)

    def put(self, path: str, **kwargs: Any) -> Any:
        return self._request("PUT", path, **kwargs)

    def delete(self, path: str, **kwargs: Any) -> Any:
        return self._request("DELETE", path, **kwargs)

    def health(self) -> Dict[str, Any]:
        """Check coordinator health."""
        return self.get("/health")
