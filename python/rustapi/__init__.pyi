from typing import Any, Callable, Dict, List, Optional, TypeVar, Union

F = TypeVar("F", bound=Callable[..., Any])

class PyRequest:
    """The request object passed to handlers requesting a `req` or `request` parameter."""

    method: str
    path: str
    path_params: Dict[str, str]
    query_params: Dict[str, str]
    headers: Dict[str, str]
    cookies: Dict[str, str]
    form: Dict[str, str]
    files: Dict[str, List[UploadFile]]
    body: str

    def __init__(
        self,
        method: str,
        path: str,
        path_params: Dict[str, str],
        query_params: Dict[str, str],
        headers: Dict[str, str],
        cookies: Dict[str, str],
        form: Dict[str, str],
        files: Dict[str, List[UploadFile]],
        body: str,
    ) -> None: ...

class Response:
    """Explicit HTTP response wrapper allowing custom content, status code, and headers."""

    content: Any
    status_code: int
    headers: Dict[str, str]

    def __init__(
        self,
        content: Any = None,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ) -> None: ...

class StreamingResponse:
    """Chunked streaming response wrapper for generators and iterables."""

    content: Any
    status_code: int
    headers: Dict[str, str]

    def __init__(
        self,
        content: Any,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
        media_type: Optional[str] = None,
    ) -> None: ...

class UploadFile:
    """Represents an uploaded file from a multipart form request."""

    filename: str
    content_type: str

    def read(self) -> bytes: ...

class WebSocket:
    """Bidirectional WebSocket connection wrapper."""

    def receive_text(self) -> str: ...
    def send_text(self, text: str) -> None: ...

class HTTPException(Exception):
    """Raise inside a handler to return a structured HTTP error response."""

    status_code: int
    detail: Any
    headers: Dict[str, str]

    def __init__(
        self,
        status_code: int,
        detail: Any = None,
        headers: Optional[Dict[str, str]] = None,
    ) -> None: ...

class Depends:
    """FastAPI-compatible dependency injection marker."""

    dependency: Optional[Callable[..., Any]]
    use_cache: bool

    def __init__(
        self,
        dependency: Optional[Callable[..., Any]] = None,
        *,
        use_cache: bool = True,
    ) -> None: ...

class APIRouter:
    """Sub-router for modular path routing and prefixing."""

    prefix: str
    routes: List[tuple]

    def __init__(self, prefix: str = "", tags: Optional[List[str]] = None) -> None: ...
    def get(self, path: str) -> Callable[[F], F]: ...
    def post(self, path: str) -> Callable[[F], F]: ...
    def put(self, path: str) -> Callable[[F], F]: ...
    def delete(self, path: str) -> Callable[[F], F]: ...
    def patch(self, path: str) -> Callable[[F], F]: ...
    def websocket(self, path: str) -> Callable[[F], F]: ...

class BackgroundTasks:
    """Collection of tasks to run asynchronously after sending the HTTP response."""

    tasks: List[tuple]

    def __init__(self) -> None: ...
    def add_task(self, func: Callable[..., Any], *args: Any, **kwargs: Any) -> None: ...

class Engine:
    """The RustAPI core engine powered by Hyper & Tokio."""

    def __init__(self) -> None: ...

    # ---- HTTP Route Decorators ----
    def get(self, path: str) -> Callable[[F], F]: ...
    def post(self, path: str) -> Callable[[F], F]: ...
    def put(self, path: str) -> Callable[[F], F]: ...
    def delete(self, path: str) -> Callable[[F], F]: ...
    def patch(self, path: str) -> Callable[[F], F]: ...
    def websocket(self, path: str) -> Callable[[F], F]: ...

    # ---- Modular Routing & Lifecycle ----
    def include_router(self, router: APIRouter, prefix: str = "") -> None: ...
    def on_event(self, event_type: str) -> Callable[[F], F]: ...

    # ---- Model Context Protocol (MCP) ----
    def tool(
        self, name: Optional[str] = None, description: Optional[str] = None
    ) -> Callable[[F], F]: ...
    def resource(
        self, uri: str, mime_type: Optional[str] = None
    ) -> Callable[[F], F]: ...
    def prompt(
        self, name: Optional[str] = None, description: Optional[str] = None
    ) -> Callable[[F], F]: ...

    # ---- Server Entry Point ----
    def run(
        self,
        host: str = "127.0.0.1",
        port: int = 8000,
        reload: bool = False,
        workers: int = 1,
    ) -> None: ...
