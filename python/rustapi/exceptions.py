from typing import Any, Optional


class HTTPException(Exception):
    """Simple exception used to return structured HTTP errors."""

    def __init__(self, status_code: int, detail: Any = None, headers: Optional[dict] = None):
        self.status_code = status_code
        self.detail = detail
        self.headers = headers or {}
        super().__init__(detail)
