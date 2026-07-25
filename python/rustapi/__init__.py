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

from rustapi._rustapi import Engine, PyRequest, compute

__version__ = "0.1.0"
__all__ = ["Engine", "PyRequest", "compute"]
