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
    def json(self) -> Any: ...

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

class HTMLResponse(Response):
    """HTML response wrapper automatically setting Content-Type: text/html."""

    def __init__(
        self,
        content: str = "",
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ) -> None: ...

class JSONResponse(Response):
    """JSON response wrapper automatically setting Content-Type: application/json."""

    def __init__(
        self,
        content: Any = None,
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ) -> None: ...

class PlainTextResponse(Response):
    """Plain text response wrapper automatically setting Content-Type: text/plain."""

    def __init__(
        self,
        content: str = "",
        status_code: int = 200,
        headers: Optional[Dict[str, str]] = None,
    ) -> None: ...

class RedirectResponse(Response):
    """HTTP redirect response wrapper setting Location header."""

    def __init__(
        self,
        url: str,
        status_code: int = 307,
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
    def get(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def post(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def put(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def delete(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def patch(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def websocket(self, path: str) -> Callable[[F], F]: ...

class BackgroundTasks:
    """Collection of tasks to run asynchronously after sending the HTTP response."""

    tasks: List[tuple]

    def __init__(self) -> None: ...
    def add_task(self, func: Callable[..., Any], *args: Any, **kwargs: Any) -> None: ...

class Database:
    """Rust-native connection pool and zero-copy SQL execution engine."""

    def execute(self, query: str) -> int: ...
    def query_json(self, query: str) -> Response: ...

class Engine:
    """The RustAPI core engine powered by Hyper & Tokio."""

    dependency_overrides: Dict[Callable[..., Any], Callable[..., Any]]
    db: Optional[Database]

    def __init__(self) -> None: ...

    # ---- Rust-Native Database Engine ----
    def connect_db(self, url: str) -> Database: ...

    # ---- HTTP Route Decorators ----
    def get(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def post(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def put(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def delete(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
    def patch(self, path: str, response_model: Optional[Any] = None) -> Callable[[F], F]: ...
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

# ---- Embedded Rust Power Primitives ----
def encode_jwt(claims: Dict[str, Any], secret: str, algorithm: Optional[str] = None) -> str: ...
def decode_jwt(token: str, secret: str, algorithm: Optional[str] = None) -> Dict[str, Any]: ...
def hash_password(password: str) -> str: ...
def verify_password(password: str, hash: str) -> bool: ...
def render_template(template_str: str, context: Dict[str, Any]) -> str: ...
