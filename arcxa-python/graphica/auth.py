"""Authentication strategies for Graphica API."""

from abc import ABC, abstractmethod
from typing import Dict


class Auth(ABC):
    """Base authentication class."""

    @abstractmethod
    def headers(self) -> Dict[str, str]:
        """Return auth headers to include in requests."""
        pass


class TokenAuth(Auth):
    """Bearer token authentication."""

    def __init__(self, token: str):
        self.token = token

    def headers(self) -> Dict[str, str]:
        return {"Authorization": f"Bearer {self.token}"}


class BasicAuth(Auth):
    """Basic username/password authentication."""

    def __init__(self, username: str, password: str):
        import base64
        credentials = base64.b64encode(f"{username}:{password}".encode()).decode()
        self._header = f"Basic {credentials}"

    def headers(self) -> Dict[str, str]:
        return {"Authorization": self._header}


class NoAuth(Auth):
    """No authentication (for local development)."""

    def headers(self) -> Dict[str, str]:
        return {}
