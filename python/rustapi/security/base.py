from typing import Any, Optional
try:
    from pydantic import BaseModel
except ImportError:
    class BaseModel:
        pass


class SecurityBase:
    """Base class for all FastAPI-compatible security dependencies."""

    def __init__(self, scheme_name: Optional[str] = None):
        self.scheme_name = scheme_name or self.__class__.__name__
        self.model: Any = None
