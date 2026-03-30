"""Exception classes for Graphica client."""


class GraphicaError(Exception):
    """Base exception for all Graphica errors."""
    pass


class AuthError(GraphicaError):
    """Authentication or authorization failed."""
    pass


class NotFoundError(GraphicaError):
    """Resource not found (404)."""
    pass


class ValidationError(GraphicaError):
    """Request validation failed (400)."""
    pass


class ConflictError(GraphicaError):
    """Request conflicted with current resource state (409)."""
    pass


class ServerError(GraphicaError):
    """Server-side error (5xx)."""
    pass


class ConnectionError(GraphicaError):
    """Failed to connect to server."""
    pass
