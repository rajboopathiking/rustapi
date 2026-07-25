from typing import Any, Callable, Dict, Optional, TypeVar

F = TypeVar("F", bound=Callable[..., Any])

class PyRequest:
    """The request object passed to a plain (non-pydantic) handler."""

    method: str
    path: str
    path_params: Dict[str, str]
    query_params: Dict[str, str]
    body: str

    def __init__(
        self,
        method: str,
        path: str,
        path_params: Dict[str, str],
        query_params: Dict[str, str],
        body: str,
    ) -> None: ...
    def json(self) -> Any:
        """Parse `.body` as JSON."""
        ...

class Engine:
    """The RustAPI application. Serves HTTP routes and an MCP server
    (JSON-RPC over Streamable HTTP at POST /mcp) from the same instance."""

    def __init__(self) -> None: ...

    # ---- HTTP routes ----
    def get(self, path: str) -> Callable[[F], F]: ...
    def post(self, path: str) -> Callable[[F], F]: ...
    def put(self, path: str) -> Callable[[F], F]: ...
    def delete(self, path: str) -> Callable[[F], F]: ...

    # ---- MCP: tools / resources / prompts ----
    def tool(
        self, name: Optional[str] = None, description: Optional[str] = None
    ) -> Callable[[F], F]:
        """Register an MCP tool. Input schema is auto-generated from the
        function's type hints; description defaults to its docstring."""
        ...
    def resource(
        self, uri: str, mime_type: Optional[str] = None
    ) -> Callable[[F], F]:
        """Register an MCP resource at the given URI."""
        ...
    def prompt(
        self, name: Optional[str] = None, description: Optional[str] = None
    ) -> Callable[[F], F]:
        """Register an MCP prompt template."""
        ...

    # ---- Server ----
    def run(
        self,
        host: str = "127.0.0.1",
        port: int = 8000,
        reload: bool = False,
        workers: int = 1,
    ) -> None:
        """Start the server. `workers > 1` spawns multiple processes sharing
        the listening socket (SO_REUSEPORT); `reload=True` restarts workers
        on .py file changes. Blocks until interrupted (Ctrl+C)."""
        ...

def compute(n: int) -> int:
    """Example native-speed function (sum 0..n), released from the GIL.
    Demonstrates the escape hatch for writing hot paths as real Rust."""
    ...
