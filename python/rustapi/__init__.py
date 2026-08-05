from typing import Any, Dict, Optional
from .status import status
from ._rustapi import (
    Engine,
    PyRequest,
    UploadFile,
    WebSocket,
    StreamingResponse,
    encode_jwt,
    decode_jwt,
    hash_password,
    verify_password,
    render_template,
)
from .exceptions import HTTPException, WebSocketException, WebSocketDisconnect
from .depends import Depends
from .router import APIRouter
from .background import BackgroundTasks
from .sse import EventSourceResponse, ServerSentEvent, format_sse_event
from .encoders import jsonable_encoder
from .param_functions import (
    Body,
    Cookie,
    File,
    Form,
    Header,
    Path,
    Query,
    Security,
)

from .responses import FileResponse
from . import responses, middleware

class FastAPI(Engine):
    """FastAPI-compatible application class wrapping the Rust Tokio core engine."""

    def __init__(self, *args, **kwargs):
        super().__init__()
        self.middlewares: list = []

    def add_middleware(self, middleware_cls: type, **kwargs: Any):
        """Add middleware (such as CORSMiddleware) to application configuration."""
        self.middlewares.append((middleware_cls, kwargs))

    def frontend(self, path: str = "/", directory: str = "dist"):
        """Serve a built static frontend app (e.g. Vite, React, Vue, Svelte output directory)."""
        from .staticfiles import StaticFiles
        handler = StaticFiles(directory=directory, html=True)
        norm_path = path.rstrip("/")
        wildcard_path = f"{norm_path}/{{file_path:path}}" if norm_path else "/{file_path:path}"
        root_path = norm_path if norm_path else "/"

        self.get(root_path)(lambda: handler(""))
        self.get(wildcard_path)(lambda file_path="": handler(file_path))

Engine = FastAPI
Request = PyRequest

try:
    from ._rustapi import Response
except ImportError:
    from ._rustapi import PyResponse as Response

try:
    from ._rustapi import Database
except ImportError:
    pass


class HTMLResponse(Response):
    """HTML response wrapper automatically setting Content-Type: text/html; charset=utf-8."""

    def __new__(
        cls,
        content: str = "",
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "text/html; charset=utf-8")
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


class JSONResponse(Response):
    """JSON response wrapper automatically setting Content-Type: application/json."""

    def __new__(
        cls,
        content: Any = None,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "application/json")
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


class PlainTextResponse(Response):
    """Plain text response wrapper automatically setting Content-Type: text/plain; charset=utf-8."""

    def __new__(
        cls,
        content: str = "",
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h.setdefault("Content-Type", "text/plain; charset=utf-8")
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


class RedirectResponse(Response):
    """HTTP redirect response wrapper setting Location header."""

    def __new__(
        cls,
        url: str,
        status_code: int = 307,
        headers: Optional[Dict[str, str]] = None,
    ):
        h = headers.copy() if headers else {}
        h["Location"] = url
        return Response.__new__(cls, content="", status_code=status_code, headers=h)


__version__ = "0.1.30"
__all__ = [
    "Engine",
    "FastAPI",
    "PyRequest",
    "Request",
    "Response",
    "HTMLResponse",
    "JSONResponse",
    "PlainTextResponse",
    "RedirectResponse",
    "StreamingResponse",
    "FileResponse",
    "responses",
    "middleware",
    "EventSourceResponse",
    "ServerSentEvent",
    "format_sse_event",
    "HTTPException",
    "WebSocketException",
    "WebSocketDisconnect",
    "Depends",
    "APIRouter",
    "BackgroundTasks",
    "UploadFile",
    "WebSocket",
    "Database",
    "encode_jwt",
    "decode_jwt",
    "hash_password",
    "verify_password",
    "render_template",
    "status",
    "jsonable_encoder",
    "Body",
    "Cookie",
    "File",
    "Form",
    "Header",
    "Path",
    "Query",
    "Security",
]
