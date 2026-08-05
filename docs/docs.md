# 📚 RustAPI Documentation & Reference Guide

**RustAPI** is a high-performance, **Native-Rust First Python web framework** backed by a Tokio / Hyper engine with native database streaming, embedded Rust security primitives, Tier 3 Rust-native fast-paths, and a built-in Model Context Protocol (MCP) server.

---

## 🚀 Key Features

- **FastAPI-Compatible Surface**: Familiar syntax with `@app.get()`, `@app.post()`, `@app.websocket()`, `APIRouter`, `Depends`, `app.dependency_overrides`, and Pydantic model validation.
- **Native-Rust First Core Engine**: Built on Tokio multi-threaded runtime and Hyper HTTP server for zero-overhead networking and low latency.
- **Tier 3 Rust-Native Fast-Paths (`app.add_native_route`)**: Serve pre-compiled Rust endpoints directly inside Hyper/Tokio (**50,000+ req/sec**), completely bypassing CPython interpreter and GIL.
- **Rust-Native Database Engine (`sqlx`)**: Native PostgreSQL & SQLite connection pools (`app.connect_db()`) with zero-copy JSON streaming directly to client sockets (`db.query_json()`).
- **Embedded Rust Security & Templating Primitives**: High-speed `jsonwebtoken` (`encode_jwt`/`decode_jwt`), Argon2 password hashing (`hash_password`/`verify_password`), and MiniJinja (`render_template`).
- **Sync & Async Handlers**: Supports both standard `def` and `async def` route handlers dispatched off the main loop to prevent thread blocking.
- **Built-in MCP Server**: Exposes Model Context Protocol tools (`@app.tool()`), resources (`@app.resource()`), and prompts (`@app.prompt()`) at `POST /mcp` (JSON-RPC 2.0).
- **Auto OpenAPI & Swagger UI**: Serves interactive Swagger docs at `/docs` and raw OpenAPI schemas at `/openapi.json`.
- **Advanced I/O & Streaming**: Native chunked `StreamingResponse`, multipart `UploadFile` support, and full-duplex `WebSocket` connections.
- **Production Ergonomics & Telemetry**: Modular `APIRouter`, lifecycle hooks (`startup`, `shutdown`), real-time terminal access logs, and auto-reloader (`reload=True`).

### 📋 Documentation & Architecture Index Matrix

| Section | Content Covered | Developer Benefit |
| :--- | :--- | :--- |
| **FastAPI Compatibility** | `from rustapi import FastAPI, Request, status`, `get_swagger_ui_html`, `EventSourceResponse` (SSE), `jsonable_encoder`, `Query`/`Body`/`Path`, `app.frontend()`. | Complete FastAPI drop-in compatibility and SSE/SPA helpers (see [`fastapi_compatibility_and_features.md`](fastapi_compatibility_and_features.md)). |
| **§1. Core Engine & Routing** | `Engine()`, `@app.get`, `@app.post`, `@app.put`, `@app.delete`, `@app.patch`, `sync` and `async def` handlers. | Quick reference for routing syntax & handler types. |
| **§2. Request Metadata & Errors** | `req` (`PyRequest`) object, custom `Response(..., status_code, headers)`, `HTTPException`. | Inspect headers, query/path params, and raise clean HTTP errors. |
| **💡 FastAPI `Request` vs `req`** | Full property comparison table (`method`, `path`, `headers`, `cookies`, `form`, `files`, `body`, `json()`). | Eliminates confusion when migrating from FastAPI to RustAPI. |
| **§3. Dependency Injection** | `Depends(func)`, generator setup/teardowns, and `app.dependency_overrides` for mocking in tests. | Smooth test mocking & request-scoped dependency management. |
| **§4. Advanced I/O & File Uploads** | `StreamingResponse`, `UploadFile` (text `.decode("utf-8")` vs binary images via `io.BytesIO`), and `WebSocket` (`receive_text`/`send_text`). | Prevents common binary file upload errors (`UnicodeDecodeError`, `PIL` crashes). |
| **§5. Rust-Native Database Engine** | SQLite & PostgreSQL connection strings (`app.connect_db()`), parameterized queries (`?1`, `$1`), `db.execute()`, `db.fetch_one()`, `db.fetch_all()`, and `db.query_json()`. | Direct SQL execution & zero-copy JSON socket streaming. |
| **§6. Embedded Rust Power Modules** | `encode_jwt()` / `decode_jwt()`, `hash_password()` / `verify_password()` (Argon2), `render_template()` (MiniJinja), `HTMLResponse`, `JSONResponse`, `PlainTextResponse`, `RedirectResponse`. | High-speed C-extension primitives with zero external Python package dependencies. |
| **§7. Tier 3 Rust Fast-Paths** | Zero-GIL native route fast-paths (`app.add_native_route()`) executing inside Tokio/Hyper at **50,000+ req/sec**, PyO3 `py.allow_threads` CPU offloading. | Enables ultra-high-speed native endpoints when needed. |
| **§8. Real-Time Access Logging** | Real-time terminal request access logging (`INFO: 127.0.0.1 - "GET /docs HTTP/1.1" 200 - 0.85ms`). | Zero-config observability during local dev & production. |
| **§9. Production Ergonomics** | `APIRouter()` modular route mounting (`app.include_router()`), lifecycle hooks (`@app.on_event("startup")`, `@app.on_event("shutdown")`). | Modular app structure for large production codebases. |
| **§10. MCP Server Integration** | Built-in Model Context Protocol server at `POST /mcp` (`@app.tool()`, `@app.resource()`, `@app.prompt()`). | Seamless AI agent tool & prompt integration. |
| **§11. Background Tasks** | `BackgroundTasks` injection & `bg.add_task(func, *args, **kwargs)`. | Non-blocking post-response background task execution. |
| **§12. Type Coercion & 422 Errors** | Strict type coercion (`int`, `float`, `bool`) and structured `422 Unprocessable Entity` JSON payloads. | Clear input validation debugging without uncaught crashes. |
| **§13. Swagger UI & Schema Overrides** | Overriding `/openapi.json` via Tier 3 native routes (`app.add_native_route("/openapi.json", schema)`). | Complete customization of Swagger UI for complex endpoints. |
| **§14. Server Deployment & Hot Reload** | `app.run(host="127.0.0.1", port=8000, reload=True, workers=4)`. | Instant local hot-reloading & multi-process production scaling. |
| **§15. Architecture & Threading Model** | Tokio multi-threaded runtime, Hyper listener, PyO3 GIL semaphore, sync/async handler dispatching. | In-depth understanding of execution flow & GIL contention prevention. |
| **§16. FAQ & Troubleshooting** | Binary file reading (`BytesIO`), synchronous `req.json()`, DB URIs, and unit testing strategies. | Instant resolution for common developer gotchas & debugging. |
| **🛠 API Reference** | Complete table of all exported functions, classes, and methods in the `rustapi` package. | Quick symbol lookup for IDEs and code completion. |

---

## 📖 Quick Start

```python
from rustapi import Engine
from pydantic import BaseModel

app = Engine()

class Item(BaseModel):
    name: str
    price: float

@app.get("/")
def root():
    return {"message": "Welcome to RustAPI!"}

@app.post("/items")
def create_item(item: Item):
    return {"name": item.name, "price": item.price}

# Tier 3 Native Fast-Path Route (50,000+ req/sec)
app.add_native_route("/health", '{"status": "operational"}', content_type="application/json")

@app.tool()
def add_numbers(a: int, b: int) -> int:
    """Add two numbers (Exposed via MCP server at /mcp)."""
    return a + b

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000, reload=True)
```

---

## ⚡ FastAPI vs RustAPI: Migration & Reference Guide

### §1. Core Engine & Basic Routing

#### FastAPI
```python
from fastapi import FastAPI
import asyncio

app = FastAPI()

@app.get("/")
def root():
    return {"message": "Welcome to FastAPI!"}

@app.get("/sync")
def sync_route():
    return {"type": "sync"}

@app.get("/async")
async def async_route():
    await asyncio.sleep(0.1)
    return {"type": "async"}
```

#### RustAPI
```python
from rustapi import Engine
import asyncio

app = Engine()

@app.get("/")
def root():
    return {"message": "Welcome to RustAPI!"}

@app.get("/sync")
def sync_route():
    return {"type": "sync"}

@app.get("/async")
async def async_route():
    await asyncio.sleep(0.1)
    return {"type": "async"}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

### §2. Request Metadata, Custom Responses & Error Handling

#### FastAPI
```python
from fastapi import FastAPI, Request, Response, HTTPException
from pydantic import BaseModel

app = FastAPI()

class Item(BaseModel):
    name: str
    price: float

@app.get("/inspect")
def inspect_req(req: Request):
    return {"user-agent": req.headers.get("user-agent"), "cookie": req.cookies.get("session")}

@app.get("/custom-resp")
def custom_resp(response: Response):
    response.status_code = 201
    response.headers["X-Custom"] = "Header-Value"
    return {"status": "created"}

@app.post("/items")
def create_item(item: Item):
    if item.price < 0:
        raise HTTPException(status_code=400, detail="Price must be non-negative")
    return item
```

#### RustAPI
```python
from rustapi import Engine, Response, HTTPException
from pydantic import BaseModel

app = Engine()

class Item(BaseModel):
    name: str
    price: float

@app.get("/inspect")
def inspect_req(req):
    return {
        "user-agent": req.headers.get("user-agent"),
        "path_params": req.path_params,
        "query_params": req.query_params,
    }

@app.get("/custom-resp")
def custom_resp():
    # Return explicit rustapi.Response object
    return Response({"status": "created"}, status_code=201, headers={"X-Custom": "Header-Value"})

@app.post("/items")
def create_item(item: Item):
    if item.price < 0:
        raise HTTPException(status_code=400, detail="Price must be non-negative")
    return item # Automatic Pydantic model validation & 422 error handling
```

---

### §3. Dependency Injection & Generator Teardowns

#### FastAPI
```python
from fastapi import FastAPI, Depends

app = FastAPI()

def get_db():
    db = "active_db_connection"
    try:
        yield db
    finally:
        pass # Cleanup logic

@app.get("/users")
def get_users(db = Depends(get_db)):
    return {"db": db}
```

#### RustAPI
```python
from rustapi import Engine, Depends

app = Engine()

def get_db():
    db = "active_db_connection"
    yield db
    # Generator teardown is automatically executed post-response transmission

@app.get("/users")
def get_users(db = Depends(get_db)):
    return {"db": db}

# Dependency Overrides for Testing
app.dependency_overrides[get_db] = lambda: "mock_test_db"
```

---

### §4. Advanced I/O: Streaming, File Uploads & WebSockets

#### FastAPI
```python
from fastapi import FastAPI, UploadFile, File, Form, WebSocket
from fastapi.responses import StreamingResponse

app = FastAPI()

@app.get("/stream")
def stream():
    def generate():
        yield "chunk 1\n"
        yield "chunk 2\n"
    return StreamingResponse(generate(), media_type="text/plain")

@app.post("/upload")
def upload(document: UploadFile = File(...), description: str = Form(...)):
    return {"filename": document.filename, "description": description}

@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    while True:
        data = await websocket.receive_text()
        await websocket.send_text(f"echo: {data}")
```

#### RustAPI
```python
from rustapi import Engine, StreamingResponse

app = Engine()

@app.get("/stream")
def stream():
    def generate():
        yield "chunk 1\n"
        yield "chunk 2\n"
    return StreamingResponse(generate(), media_type="text/plain")

@app.post("/upload")
def upload(req):
    # 1. req.files contains field_name -> list of UploadFile objects
    file_list = req.files.get("document", [])
    if not file_list:
        return {"error": "No document uploaded"}
    
    doc = file_list[0]
    filename = doc.filename
    content_type = doc.content_type
    
    # 2. Synchronous raw bytes reading
    raw_bytes = doc.read()
    
    # Text file reading
    text_content = raw_bytes.decode("utf-8")
    
    return {
        "filename": filename,
        "content_type": content_type,
        "size_bytes": len(raw_bytes),
        "description": req.form.get("description"),
        "content_preview": text_content[:100]
    }

# Binary / Image Upload Handling (PIL Example)
import io
from PIL import Image

@app.post("/upload-avatar")
def upload_avatar(req):
    files = req.files.get("avatar", [])
    if not files:
        return {"error": "Avatar required"}
    
    # DO NOT use .decode("utf-8") on binary images! Use BytesIO directly:
    avatar_bytes = files[0].read()
    img = Image.open(io.BytesIO(avatar_bytes))
    
    return {
        "filename": files[0].filename,
        "format": img.format,
        "size": f"{img.width}x{img.height}"
    }

@app.websocket("/ws")
async def websocket_endpoint(ws):
    while True:
        data = ws.receive_text()
        ws.send_text(f"echo: {data}")
```

---

### §5. Rust-Native Database Engine (Zero-Copy SQL)

RustAPI includes an embedded high-concurrency `sqlx` connection pool that executes database queries natively in Rust, formatting and streaming UTF-8 JSON bytes directly to the client while bypassing Python GIL and Pydantic object allocation overhead.

```python
from rustapi import Engine

app = Engine()

# 1. Connect to SQLite or PostgreSQL
db = app.connect_db("sqlite::memory:") 
# For PostgreSQL: db = app.connect_db("postgres://user:pass@localhost:5432/mydb")

# 2. Execute DDL & Statements
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")

# 3. Parameterized Statements (SQLite uses ?1, ?2 | Postgres uses $1, $2)
db.execute("INSERT INTO users (name, email) VALUES (?1, ?2)", ["Alice", "alice@example.com"])
db.execute("INSERT INTO users (name, email) VALUES (?1, ?2)", ["Bob", "bob@example.com"])

# 4. Single Record Lookup (returns dict or None)
user = db.fetch_one("SELECT * FROM users WHERE id = ?1", [1])

# 5. Multiple Records Lookup (returns list of dicts)
users_list = db.fetch_all("SELECT * FROM users WHERE name = ?1", ["Alice"])

# 6. Zero-Copy JSON Streaming Route (Streams UTF-8 JSON directly to HTTP socket)
@app.get("/users")
def get_users():
    return db.query_json("SELECT * FROM users")
```

---

### §6. Embedded Rust Power Primitives (JWT, Argon2, MiniJinja)

RustAPI exposes native, zero-dependency C-speed security and templating primitives directly to Python:

```python
from rustapi import encode_jwt, decode_jwt, hash_password, verify_password, render_template, HTMLResponse, JSONResponse, PlainTextResponse, RedirectResponse

# 1. Native Rust JWT Engine (HS256, HS384, HS512)
token = encode_jwt({"user_id": 42, "role": "admin"}, secret="secret_key")
claims = decode_jwt(token, secret="secret_key")

# 2. Native Argon2 Password Hashing (releasing GIL on Tokio worker pool)
pw_hash = hash_password("MySecurePassword123!")
is_valid = verify_password("MySecurePassword123!", pw_hash)

# 3. Native Template Rendering & Specialized Responses
rendered = render_template("<h1>Hello {{ name }}!</h1>", {"name": "Boopathi"})

# Dedicated Response Wrappers
html_resp = HTMLResponse(rendered)            # Content-Type: text/html
json_resp = JSONResponse({"status": "ok"})     # Content-Type: application/json
text_resp = PlainTextResponse("Hello")        # Content-Type: text/plain
redir_resp = RedirectResponse("/docs")        # Status: 307, Location: /docs
```

#### Request JSON & Form Parsing (`req.json()`, `req.form`)

```python
@app.post("/auth/login")
def login(req):
    data = req.json()
    username = data.get("username")
    password = data.get("password", "")
    
    h = hash_password(password)
    token = encode_jwt({"sub": username}, secret="key")
    return JSONResponse({"token": token, "hash": h})
```

---

### §7. Tier 3: Rust-Native Business Logic & Route Fast-Paths

Tier 3 routes execute 100% in Rust inside Hyper and Tokio, completely bypassing CPython interpreter overhead and the Global Interpreter Lock (GIL).

```python
from rustapi import Engine

app = Engine()

# 1. Register a Tier 3 JSON Fast-Path (50,000+ req/sec)
app.add_native_route(
    path="/fast-json",
    body='{"status": "ok", "tier": 3}',
    method="GET",
    status_code=200,
    content_type="application/json"
)

# 2. Register a Tier 3 HTML Fast-Path
app.add_native_route(
    path="/health",
    body="<h1>System Operational</h1>",
    method="GET",
    status_code=200,
    content_type="text/html"
)
```

For heavy CPU-bound custom calculations, implement PyO3 C-extensions that release the GIL via `py.allow_threads()`:

```rust
// In src/lib.rs (PyO3 Rust module)
#[pyfunction]
fn compute_heavy_logic(py: Python<'_>, data: Vec<f64>) -> PyResult<f64> {
    py.allow_threads(move || {
        let result = data.iter().map(|v| v * 1.05).sum();
        Ok(result)
    })
}
```

---

### §8. Real-Time Access Logging & Telemetry

RustAPI automatically outputs structured, high-speed terminal access logs for every incoming HTTP request with zero overhead:

```text
INFO:     Started server process [80839]
INFO:     RustAPI server running on http://127.0.0.1:8000 (Press CTRL+C to quit)
INFO:     127.0.0.1:54321 - "GET /docs HTTP/1.1" 200 - 0.85ms
INFO:     127.0.0.1:54322 - "POST /auth/login HTTP/1.1" 200 - 4.12ms
INFO:     127.0.0.1:54323 - "GET /invalid HTTP/1.1" 404 - 0.15ms
```

---

### §9. Production Ergonomics (APIRouter & Lifespan Hooks)

```python
from rustapi import Engine, APIRouter

app = Engine()
router = APIRouter()

@router.get("/ping")
def ping():
    return {"status": "pong"}

app.include_router(router, prefix="/api/v1")

@app.on_event("startup")
def startup_event():
    print("App starting up...")

@app.on_event("shutdown")
def shutdown_event():
    print("App shutting down...")
```

---

### §10. Model Context Protocol (MCP) Server Integration

RustAPI features a built-in MCP server that handles JSON-RPC 2.0 requests over HTTP at `POST /mcp`.

```python
from rustapi import Engine

app = Engine()

# 1. Register MCP Tool
@app.tool(name="calculator", description="Performs basic calculation")
def calculate(expression: str) -> str:
    return str(eval(expression))

# 2. Register MCP Resource
@app.resource(uri="config://app", mime_type="application/json")
def get_config():
    return '{"env": "production", "debug": false}'

# 3. Register MCP Prompt
@app.prompt(name="summarize", description="Summarization template")
def summarize_prompt(text: str):
    return f"Please summarize the following text:\n\n{text}"

if __name__ == "__main__":
    app.run(port=8000)
```

---

### §11. Background Tasks Execution (`BackgroundTasks`)

RustAPI lets you schedule background tasks that execute asynchronously after an HTTP response has been transmitted to the client.

```python
from rustapi import Engine, BackgroundTasks

app = Engine()

def send_audit_log(user_id: int, action: str):
    # Executed after HTTP response is transmitted
    print(f"Audit log: User {user_id} performed {action}")

@app.post("/users/{user_id}/action")
def perform_action(user_id: int, bg: BackgroundTasks):
    bg.add_task(send_audit_log, user_id, action="CREATE_RECORD")
    return {"status": "accepted", "user_id": user_id}
```

---

### §12. Parameter Type Coercion & Structured 422 Errors

RustAPI automatically casts path and query parameters to annotated Python types (`int`, `float`, `bool`, `str`). If type casting fails, RustAPI returns a structured `422 Unprocessable Entity` HTTP error payload without throwing uncaught Python exceptions.

```python
@app.get("/items/{item_id}")
def get_item(item_id: int, quantity: int = 1, discount: float = 0.0):
    return {"item_id": item_id, "quantity": quantity, "discount": discount}
```

* Requesting `/items/42?quantity=5` returns `200 OK`: `{"item_id": 42, "quantity": 5, "discount": 0.0}`
* Requesting `/items/not_an_int` returns `422 Unprocessable Entity`:
  ```json
  {
    "detail": [
      {
        "loc": ["path", "item_id"],
        "msg": "invalid integer value",
        "type": "type_error.integer"
      }
    ]
  }
  ```

---

### §13. Swagger UI & Custom OpenAPI Schema Overrides

RustAPI automatically generates `/openapi.json` and embeds Swagger UI at `/docs`. For complex endpoints (such as array file pickers or multi-field form schemas), you can register a Tier 3 native JSON route override for `/openapi.json`:

```python
import json
import rustapi

app = rustapi.Engine()

# Custom OpenAPI 3.0 specification for array file uploads
custom_openapi = json.dumps({
    "openapi": "3.0.0",
    "info": {"title": "My Production API", "version": "1.0.0"},
    "paths": {
        "/upload-documents": {
            "post": {
                "summary": "Upload Multiple Documents",
                "requestBody": {
                    "required": True,
                    "content": {
                        "multipart/form-data": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "documents": {
                                        "type": "array",
                                        "items": {"type": "string", "format": "binary"}
                                    }
                                }
                            }
                        }
                    }
                },
                "responses": {"200": {"description": "Upload Success"}}
            }
        }
    }
})

# Register Tier 3 Native Route Override
app.add_native_route("/openapi.json", custom_openapi, content_type="application/json")
```

---

### §14. Production Server Deployment & Hot Reload (`reload=True`, `workers=N`)

RustAPI includes a built-in supervisor process manager and hot-reload watcher powered by `notify`.

```python
if __name__ == "__main__":
    # Local Development Mode with Hot Reloading
    app.run(host="127.0.0.1", port=8000, reload=True)

    # Production Mode with Multi-Process Worker Pool (e.g. 4 CPU Workers)
    # app.run(host="0.0.0.0", port=8000, workers=4)
```

---

### §15. Architecture & Threading Model Deep-Dive

RustAPI combines a multi-threaded Tokio Rust core with CPython worker pools:

1. **Hyper TCP Socket Listener**: Hyper runs on Tokio worker threads to handle thousands of concurrent TCP sockets, header parsing, and Radix URL matching without invoking CPython.
2. **Synchronous `def` Handlers**: Standard `def` Python handlers run on Tokio blocking worker pools (`spawn_blocking`). This ensures synchronous operations (like file reads or external sync calls) don't block the main event loop.
3. **Asynchronous `async def` Handlers**: Async handlers run on the event loop via coroutine callbacks.
4. **PyO3 & GIL Semaphore Management**: RustAPI limits GIL contention by dynamically acquiring PyO3 GIL semaphores sized to your CPU core count (`num_cpus * 2`), preventing GIL thrashing under extreme concurrency.
5. **Tier 3 Native Routes**: Pure Rust closures executing 100% inside Hyper with zero GIL acquisition or PyO3 call overhead (**50,000+ req/sec**).

---

### §16. Frequently Asked Questions (FAQ) & Troubleshooting

#### Q1: Why do I get `UnidentifiedImageError` or `UnicodeDecodeError` when processing uploaded images/files?
**Cause**: Calling `doc.read().decode("utf-8")` on binary image bytes corrupts binary data.  
**Fix**: Raw bytes must be passed directly into a byte stream (`io.BytesIO(doc.read())`) without decoding as UTF-8 text:
```python
import io
from PIL import Image

file_obj = req.files["photo"][0]
# CORRECT: Load raw bytes directly into BytesIO
img = Image.open(io.BytesIO(file_obj.read()))
```

#### Q2: Why does `req.json()` or `req.form` not require `await`?
**Cause**: In Starlette/FastAPI, reading request bodies is asynchronous (`await request.json()`).  
**Fix**: In RustAPI, the Rust Tokio engine reads and parses HTTP request bodies, forms, cookies, and files into native C-structs *before* calling your Python handler. Therefore, `req.json()` and `req.form` are fast, synchronous calls.

#### Q3: How do I handle SQLite in-memory databases vs file databases or PostgreSQL?
```python
# In-Memory SQLite (shared cache across Tokio worker threads)
db = app.connect_db("sqlite::memory:")

# File-based SQLite
db = app.connect_db("sqlite://app.db")

# PostgreSQL Connection Pool
db = app.connect_db("postgres://user:password@localhost:5432/dbname")
```

#### Q4: How do I write unit tests for my RustAPI routes?
Use `pytest` with standard HTTP client libraries (`httpx` or `requests`), or mock dependencies using `app.dependency_overrides`:
```python
def test_user_endpoint():
    app.dependency_overrides[get_db] = lambda: MockDB()
    # Execute test requests against running test server
```

---

## 🛠 API Reference

### `rustapi.Engine`
The primary application class representing the server and router engine.

| Method / Property | Description |
| :--- | :--- |
| `@app.get(path)` | Registers a GET HTTP route. |
| `@app.post(path)` | Registers a POST HTTP route. |
| `@app.put(path)` | Registers a PUT HTTP route. |
| `@app.delete(path)` | Registers a DELETE HTTP route. |
| `@app.patch(path)` | Registers a PATCH HTTP route. |
| `@app.websocket(path)` | Registers a WebSocket route. |
| `add_native_route(path, body, ...)` | Registers a Tier 3 zero-GIL Rust fast-path route. |
| `connect_db(uri)` | Connects to PostgreSQL or SQLite database using `sqlx`. |
| `include_router(router, prefix="")` | Mounts an `APIRouter` instance under an optional path prefix. |
| `@app.on_event("startup" \| "shutdown")` | Registers lifecycle startup or shutdown handlers. |
| `@app.tool(name=None, description=None)` | Registers an MCP Tool endpoint. |
| `@app.resource(uri, mime_type=None)` | Registers an MCP Resource endpoint. |
| `@app.prompt(name=None, description=None)` | Registers an MCP Prompt endpoint. |
| `run(host="127.0.0.1", port=8000, reload=False, workers=1)` | Starts the Tokio/Hyper server instance. |

### Module Exports (`rustapi`)

- `Engine`: Main server application class.
- `APIRouter`: Modular route grouping class.
- `Response`: Custom response object with custom `status_code` and `headers`.
- `HTMLResponse`, `JSONResponse`, `PlainTextResponse`, `RedirectResponse`: Pre-built response type helpers.
- `StreamingResponse`: Generator-backed HTTP chunked streaming response.
- `HTTPException`: Standard HTTP exception with `status_code` and `detail`.
- `Depends`: Dependency injection helper.
- `BackgroundTasks`: Helper for scheduling background task execution.
- `UploadFile`: Wrapper for multipart uploaded file streams (`read()`, `filename`, `content_type`).
- `WebSocket`: Full-duplex WebSocket object (`receive_text()`, `send_text()`).
- `encode_jwt(claims, secret, algorithm="HS256")`: Native Rust JWT encoder.
- `decode_jwt(token, secret, algorithm="HS256")`: Native Rust JWT decoder.
- `hash_password(password)`: Native Argon2 password hasher.
- `verify_password(password, hash)`: Native Argon2 password verifier.
- `render_template(template_str, context)`: Native MiniJinja template renderer.

#### 💡 FastAPI `Request` vs RustAPI `req` (`PyRequest`) Mapping

| Feature / Attribute | FastAPI (`starlette.requests.Request`) | RustAPI (`req` / `PyRequest`) |
| :--- | :--- | :--- |
| **HTTP Method** | `request.method` | `req.method` |
| **Request Path** | `request.url.path` | `req.path` |
| **Path Params** | `request.path_params` | `req.path_params` |
| **Query Params** | `request.query_params` | `req.query_params` |
| **Headers** | `request.headers` | `req.headers` |
| **Cookies** | `request.cookies` | `req.cookies` |
| **Form Data** | `await request.form()` | `req.form` (Synchronous dictionary) |
| **Uploaded Files** | `await request.form()` | `req.files` (Dict of `UploadFile` objects) |
| **Body String** | `await request.body()` | `req.body` (Synchronous string) |
| **JSON Parsing** | `await request.json()` | `req.json()` (Synchronous method, no `await`) |

#### Quick Code Example in RustAPI

```python
@app.post("/api/user")
def create_user(req):
    # 1. Parse JSON body synchronously (no await required!)
    data = req.json()
    
    # 2. Inspect headers & query params
    auth_header = req.headers.get("authorization")
    api_key = req.query_params.get("key")
    
    # 3. Access uploaded files if multipart form
    avatar = req.files.get("avatar")
    
    return {
        "status": "created",
        "method": req.method,
        "path": req.path,
        "data": data
    }
```

### 📖 Complete Structure :
                                                                                                                                  
  1. 🚀 Key Features & 📋 Index Matrix: High-level feature list and comprehensive section index matrix.                                                                              
  2. 📖 Quick Start: Minimal working example with Pydantic validation, Tier 3 native routes, and MCP server endpoints.                                                               
  3. §1. Core Engine & Basic Routing: Engine(), @app.get, @app.post, @app.put, @app.delete, @app.patch, sync and async def handlers.                                                 
  4. §2. Request Metadata, Custom Responses & Error Handling: Unified req (PyRequest) object, custom status codes, headers, and HTTPException.                                       
  5. 💡 FastAPI Request vs RustAPI req Mapping: Exhaustive property comparison table (method, path, headers, cookies, form, files, body, json()).                                    
  6. §3. Dependency Injection & Generator Teardowns: Depends(func), generator cleanup hooks, and app.dependency_overrides.                                                           
  7. §4. Advanced I/O: Streaming, File Uploads & WebSockets: StreamingResponse, UploadFile (text .decode("utf-8") vs binary images via io.BytesIO), and WebSocket                    
  (receive_text/send_text).                                                                                                                                                          
  8. §5. Rust-Native Database Engine (Zero-Copy SQL): PostgreSQL & SQLite setup (app.connect_db()), parameterized SQL queries (?1, $1), db.execute(), db.fetch_one(), db.fetch_all(),
  and db.query_json().                                                                                                                                                               
  9. §6. Embedded Rust Power Primitives: encode_jwt() / decode_jwt(), hash_password() / verify_password() (Argon2), render_template() (MiniJinja), HTMLResponse, JSONResponse,       
  PlainTextResponse, RedirectResponse.                                                                                                                                               
  10. §7. Tier 3 Rust-Native Business Logic: Zero-GIL native route fast-paths (app.add_native_route()) at 50,000+ req/sec, PyO3 py.allow_threads CPU offloading.                     
  11. §8. Real-Time Access Logging & Telemetry: Zero-overhead HTTP terminal access logging.                                                                                          
  12. §9. Production Ergonomics: APIRouter() modular route mounting and @app.on_event("startup") / shutdown lifecycle hooks.                                                         
  13. §10. Model Context Protocol (MCP) Server: Built-in MCP server at POST /mcp (@app.tool(), @app.resource(), @app.prompt()).                                                      
  14. §11. Background Tasks Execution: BackgroundTasks injection & bg.add_task(func, *args, **kwargs).                                                                               
  15. §12. Parameter Type Coercion & 422 Errors: Automatic type coercion (int, float, bool) and structured 422 Unprocessable Entity JSON error payloads.                             
  16. §13. Swagger UI & Custom OpenAPI Schema Overrides: Registering Tier 3 native route overrides (app.add_native_route("/openapi.json", schema)).                                  
  17. §14. Production Server Deployment & Hot Reload: app.run(host=..., port=..., reload=True, workers=4) configuration.                                                             
  18. §15. Architecture & Threading Model Deep-Dive: Tokio multi-threaded runtime, Hyper listener, PyO3 GIL semaphore sizing, sync/async handler dispatching.                        
  19. §16. Frequently Asked Questions (FAQ) & Troubleshooting: Instant solutions for binary file reading (BytesIO), synchronous req.json(), database URIs, and unit testing          
  strategies.                                                                                                                                                                        
  20. 🛠 API Reference: Complete reference table of all exported classes, methods, and functions in the rustapi package.
  ──────
  ### 🧪 Verification
  
  All 51 test suites were re-run and passed cleanly (1.11s).
