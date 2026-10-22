from typing import Any, Optional


class HTTPException(Exception):
    """Simple exception used to return structured HTTP errors."""

    def __init__(self, status_code: int, detail: Any = None, headers: Optional[dict] = None):
        self.status_code = status_code
        self.detail = detail
        self.headers = headers or {}
        super().__init__(f"{status_code}: {detail}")


class ResponseValidationError(Exception):
    """Raised when server handler output fails response_model validation (HTTP 500).

    This is distinct from request validation errors (422). When the handler returns
    data that does not conform to the declared response_model, this error triggers
    a 500 Internal Server Error — matching FastAPI's behavior.
    """

    def __init__(self, errors: Any, *, body: Any = None):
        self._errors = errors
        self.body = body
        super().__init__(f"ResponseValidationError: {errors}")

    def errors(self):
        return self._errors


class WebSocketException(Exception):
    """Exception raised for WebSocket error conditions."""

    def __init__(self, code: int, reason: Optional[str] = None):
        self.code = code
        self.reason = reason or ""
        super().__init__(f"WebSocketException code={code} reason={self.reason}")


class WebSocketDisconnect(Exception):
    """Exception raised when a WebSocket connection is closed/disconnected."""

    def __init__(self, code: int = 1000, reason: Optional[str] = None):
        self.code = code
        self.reason = reason or ""
        super().__init__(f"WebSocketDisconnect code={code} reason={self.reason}")

