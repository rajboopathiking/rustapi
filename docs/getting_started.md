# 🚀 Getting Started with RustAPI

RustAPI provides a FastAPI-compatible surface backed by a high-performance Tokio / Hyper core in Rust with embedded database streaming, security primitives, and Tier 3 native fast-paths.

---

## 📌 Core Routing & Handlers

Handlers can be synchronous `def` or asynchronous `async def`. Sync handlers are executed safely inside Tokio's thread-pool without blocking incoming requests.

```python
from rustapi import Engine

app = Engine()

@app.get("/")
def sync_root():
    return {"message": "Sync handler routed by Rust"}

@app.get("/async")
async def async_root():
    return {"message": "Async handler routed by Rust"}
```

---

## ⚡ Tier 3 Rust-Native Fast-Path Routes (`app.add_native_route`)

For extreme performance hot paths (50,000+ req/sec), register pre-compiled Rust endpoints that completely bypass the CPython bytecode interpreter and GIL:

```python
# 1. Native JSON Fast-Path
app.add_native_route("/fast-json", '{"status": "ok", "tier": 3}', content_type="application/json")

# 2. Native HTML Fast-Path
app.add_native_route("/health", '<h1>System Operational</h1>', content_type="text/html")
```

---

## 🗄️ Rust-Native Database Engine

Query databases with zero-copy JSON streaming directly to the client socket:

```python
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE items (id INT, name TEXT)")
db.execute("INSERT INTO items VALUES (1, 'Laptop')")

@app.get("/db-items")
def db_items():
    # Direct zero-copy JSON stream from Rust to HTTP response
    return db.query_json("SELECT * FROM items")
```

---

## 🔍 Path & Query Parameters

Parameters are automatically coerced into `int`, `float`, `bool`, or `str` based on Python type hints:

```python
@app.get("/items/{item_id}")
def read_item(item_id: int, q: str = "default"):
    return {"item_id": item_id, "query": q}
```

If a required parameter is missing or invalid, RustAPI automatically returns a structured `422 Unprocessable Entity` error.

---

## 🛡️ Pydantic Request Validation & `response_model`

Request payloads are validated using Pydantic models. Return values can be filtered through a `response_model`:

```python
from pydantic import BaseModel
from rustapi import Engine

class UserCreate(BaseModel):
    username: str
    email: str

class UserOut(BaseModel):
    id: int
    username: str

@app.post("/users", response_model=UserOut)
def create_user(user: UserCreate):
    return {
        "id": 100,
        "username": user.username,
        "password_hash": "secret",  # Automatically filtered out by UserOut schema
        "email": user.email,
    }
```

---

## 💉 Dependency Injection & Overrides (`app.dependency_overrides`)

FastAPI-style `Depends` with request-scoped caching is supported out of the box:

```python
from rustapi import Engine, Depends

app = Engine()

def get_db():
    return {"db": "production_sqlite"}

@app.get("/data")
def get_data(db = Depends(get_db)):
    return {"db": db}

# Swapping dependencies in tests:
app.dependency_overrides[get_db] = lambda: {"db": "test_sqlite"}
```

---

## 📖 OpenAPI & Swagger UI

Interactive OpenAPI documentation is generated automatically:
* **OpenAPI JSON**: `GET /openapi.json`
* **Swagger UI**: `GET /docs`
