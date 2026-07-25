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

## License

MIT
