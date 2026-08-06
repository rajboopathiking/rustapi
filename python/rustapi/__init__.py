from typing import Any, Dict, Optional
from .status import status
from ._rustapi import (
    Engine,
    Route,
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
from . import responses, middleware, security, openapi

class FastAPI(Engine):
    """FastAPI-compatible application class wrapping the Rust Tokio core engine."""

    def __new__(cls, *args, **kwargs):
        return super().__new__(cls)

    def __init__(
        self,
        title: str = "RustAPI",
        description: str = "",
        version: str = "0.1.0",
        openapi_url: Optional[str] = "/openapi.json",
        docs_url: Optional[str] = "/docs",
        redoc_url: Optional[str] = "/redoc",
        swagger_ui_oauth2_redirect_url: Optional[str] = "/docs/oauth2-redirect",
        **kwargs: Any,
    ):
        super().__init__()
        self.title = title
        self.description = description
        self.version = version
        self.openapi_url = openapi_url
        self.docs_url = docs_url
        self.redoc_url = redoc_url
        self.swagger_ui_oauth2_redirect_url = swagger_ui_oauth2_redirect_url
        self.exception_handlers: Dict[Any, Any] = {}
        self.middlewares: list = []

    def add_middleware(self, middleware_cls: type, **kwargs: Any):
        """Add middleware (such as CORSMiddleware) to application configuration."""
        self.middlewares.append((middleware_cls, kwargs))

    async def __call__(self, scope: Dict[str, Any], receive: Any, send: Any):
        """ASGI 3.0 interface implementation for ASGITransport, TestClient, uvicorn, and pytest."""
        import inspect, json
        if scope["type"] == "lifespan":
            while True:
                message = await receive()
                if message["type"] == "lifespan.startup":
                    for handler in getattr(self, "startup_handlers", []):
                        if inspect.iscoroutinefunction(handler):
                            await handler()
                        else:
                            handler()
                    await send({"type": "lifespan.startup.complete"})
                elif message["type"] == "lifespan.shutdown":
                    for handler in getattr(self, "shutdown_handlers", []):
                        if inspect.iscoroutinefunction(handler):
                            await handler()
                        else:
                            handler()
                    await send({"type": "lifespan.shutdown.complete"})
                    break
            return

        if scope["type"] != "http":
            return

        method = scope.get("method", "GET")
        path = scope.get("path", "/")
        query_string = scope.get("query_string", b"").decode("latin1")
        headers = {k.decode("latin1").lower(): v.decode("latin1") for k, v in scope.get("headers", [])}

        body_bytes = bytearray()
        while True:
            msg = await receive()
            if msg["type"] == "http.request":
                body_bytes.extend(msg.get("body", b""))
                if not msg.get("more_body", False):
                    break

        body_str = body_bytes.decode("utf-8", errors="replace")

        try:
            status_code, response_body, resp_headers = await self.dispatch_request(
                method, path, query_string, headers, body_str
            )
        except Exception as exc:
            import logging
            logging.getLogger("rustapi").error(f"Error handling ASGI request [{method} {path}]: {exc}", exc_info=True)
            handler = self.exception_handlers.get(type(exc)) or self.exception_handlers.get(getattr(exc, "status_code", None))
            if handler:
                req = PyRequest(method=method, path=path, path_params={}, query_params={}, headers=headers, cookies={}, form={}, files={}, body=body_str)
                resp = await handler(req, exc) if inspect.iscoroutinefunction(handler) else handler(req, exc)
                status_code = getattr(resp, "status_code", 500)
                response_body = getattr(resp, "content", str(exc))
                resp_headers = getattr(resp, "headers", {"content-type": "application/json"})
            else:
                status_code = getattr(exc, "status_code", 500)
                detail = getattr(exc, "detail", str(exc))
                response_body = f'{{"detail": "{detail}"}}' if isinstance(detail, str) else json.dumps({"detail": detail})
                resp_headers = {"content-type": "application/json"}

        encoded_headers = [(k.encode("latin1"), v.encode("latin1")) for k, v in resp_headers.items()]
        await send({
            "type": "http.response.start",
            "status": status_code,
            "headers": encoded_headers,
        })
        await send({
            "type": "http.response.body",
            "body": response_body.encode("utf-8") if isinstance(response_body, str) else response_body,
        })

    def exception_handler(self, exc_class_or_status_code: Any):
        """Register an exception handler decorator for an exception class or status code."""
        def decorator(func: Any):
            self.exception_handlers[exc_class_or_status_code] = func
            return func
        return decorator

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


__version__ = "1.8.8"
__all__ = [
    "Engine",
    "FastAPI",
    "Route",
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
    "security",
    "openapi",
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
