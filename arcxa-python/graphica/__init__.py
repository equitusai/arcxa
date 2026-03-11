"""Graphica Python client for data governance and lineage."""

from graphica.client import Client
from graphica.auth import Auth, TokenAuth, BasicAuth

__version__ = "0.1.0"
__all__ = ["Client", "Auth", "TokenAuth", "BasicAuth"]
