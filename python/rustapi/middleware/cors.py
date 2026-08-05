from typing import Any, List, Optional


class CORSMiddleware:
    """FastAPI-compatible CORS Middleware configuration class."""

    def __init__(
        self,
        app: Optional[Any] = None,
        allow_origins: Optional[List[str]] = None,
        allow_methods: Optional[List[str]] = None,
        allow_headers: Optional[List[str]] = None,
        allow_credentials: bool = False,
        allow_origin_regex: Optional[str] = None,
        expose_headers: Optional[List[str]] = None,
        max_age: int = 600,
    ):
        self.app = app
        self.allow_origins = allow_origins or ["*"]
        self.allow_methods = allow_methods or ["*"]
        self.allow_headers = allow_headers or ["*"]
        self.allow_credentials = allow_credentials
        self.allow_origin_regex = allow_origin_regex
        self.expose_headers = expose_headers or []
        self.max_age = max_age
