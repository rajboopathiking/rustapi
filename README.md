# RustAPI

A FastAPI-style Python web framework backed by a Rust (tokio/hyper) engine —
with a built-in MCP (Model Context Protocol) server on the same instance.

```python
from rustapi import Engine
from pydantic import BaseModel

app = Engine()

class Item(BaseModel):
    name: str
    price: float

@app.get("/")
def root():
    return {"message": "hello"}

@app.post("/items")
def create_item(item: Item):
    return {"created": item.name, "price": item.price}

@app.tool()
def add(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

- `GET/POST/PUT/DELETE` routes with path params, query params, and Pydantic
  request-body validation
- Auto-generated OpenAPI schema at `/openapi.json`, Swagger UI at `/docs`
- MCP server at `POST /mcp` (JSON-RPC 2.0, Streamable HTTP transport):
  `@app.tool()`, `@app.resource(uri)`, `@app.prompt()`
- Multi-worker mode (`workers=N`, `SO_REUSEPORT`) and `reload=True` for dev
- Async and sync handlers, dispatched off the request-handling thread so a
  slow handler doesn't stall other in-flight requests

## Install

```bash
pip install rustapi-framework
```

## Development

```bash
pip install maturin
maturin develop --release
pytest tests/ -v
```

## What runs where

Routing, validation dispatch, JSON transport, and the MCP JSON-RPC envelope
are Rust. Your handler bodies run as ordinary Python (CPython), same as any
other Python web framework — that part doesn't change unless you write a
handler as a native `#[pyfunction]` yourself (see `rustapi.compute` for an
example of that pattern).

## Architectural Boundaries & Trade-Offs

To maintain maximum performance, RustAPI strictly adheres to the following constraints:

```
1).  No ASGI Middleware: RustAPI does not run under Uvicorn/Gunicorn. Core middleware (Auth, CORS, Logging) must run in the Rust layer to preserve speed.
```

```
2). The Python GIL Ceiling: RustAPI eliminates framework overhead, but cannot magically speed up poorly written Python code. CPU-heavy Python loops will block a thread. Phase 4 features must be used to bypass the GIL for heavy computing.
```

   ## 5. Performance Slider


   ```

   1.  **Tier 1 (Orchestrator):** Write in **Pure Python** for maximum developer speed.                                                                                                 
   2.  **Tier 2 (Hybrid):** Use **Rust-Native Modules** for heavy I/O and Serialization (The **default mode** for high performance - **Recommended**).                                                      
   3.  **Tier 3 (Turbo):** Write custom **Rust Hot-Paths** for extreme computational requirements.          

   ``` 

## License

MIT
