from typing import Any, Dict, Optional
import os
import mimetypes
try:
    from .._rustapi import Response
except ImportError:
    from .._rustapi import PyResponse as Response
from .._rustapi import StreamingResponse
from ..sse import EventSourceResponse, ServerSentEvent


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


class FileResponse(Response):
    """File response serving binary files from disk with proper Content-Type."""

    def __new__(
        cls,
        path: str,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
        media_type: Optional[str] = None,
        filename: Optional[str] = None,
    ):
        h = headers.copy() if headers else {}
        if not media_type:
            media_type, _ = mimetypes.guess_type(path)
            media_type = media_type or "application/octet-stream"
        h.setdefault("Content-Type", media_type)
        if filename:
            h.setdefault("Content-Disposition", f'attachment; filename="{filename}"')
        
        with open(path, "rb") as f:
            content = f.read().decode("latin1")
            
        return Response.__new__(cls, content=content, status_code=status_code, headers=h)


__all__ = [
    "Response",
    "HTMLResponse",
    "JSONResponse",
    "PlainTextResponse",
    "RedirectResponse",
    "StreamingResponse",
    "FileResponse",
    "EventSourceResponse",
    "ServerSentEvent",
]
