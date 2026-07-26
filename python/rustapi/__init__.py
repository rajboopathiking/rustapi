"""
RustAPI - a FastAPI-style Python web framework backed by a Rust (tokio/hyper)
engine, with a built-in MCP (Model Context Protocol) server.

    from rustapi import Engine

    app = Engine()

    @app.get("/")
    def root():
        return {"message": "hello"}

    @app.tool()
    def add(a: int, b: int) -> int:
        '''Add two numbers.'''
        return a + b

    app.run(host="0.0.0.0", port=8000)
"""

# from rustapi._rustapi import Engine, PyRequest, compute

# __version__ = "0.1.17"
# __all__ = ["Engine", "PyRequest", "compute"]


from ._rustapi import Engine, PyRequest, compute
from .exceptions import HTTPException
from .depends import Depends


class Response:
    """Python-level compatibility wrapper for the Rust-backed response object."""

    def __init__(self, content, status_code=200, headers=None):
        self.content = content
        self.status_code = status_code
        self.headers = headers or {}


__version__ = "0.1.15"
__all__ = ["Engine", "PyRequest", "Response", "compute", "HTTPException", "Depends"]