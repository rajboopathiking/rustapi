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

# FastAPI Aliases for 100% compatibility
FastAPI = Engine
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
