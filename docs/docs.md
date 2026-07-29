# 📚 RustAPI Documentation & Reference Guide

**RustAPI** is a high-performance Python web framework backed by a native Rust (`tokio` / `hyper`) core engine with a built-in Model Context Protocol (MCP) server.

---

## 🚀 Key Features

- **FastAPI-Compatible Surface**: Familiar syntax with `@app.get()`, `@app.post()`, `@app.websocket()`, and Pydantic model validation.
- **Rust Core Engine**: Built on Tokio multi-threaded runtime and Hyper HTTP server for maximum performance and low latency.
- **Async & Sync Handlers**: Supports both standard `def` and `async def` route handlers dispatched off the main loop to prevent thread blocking.
- **Built-in MCP Server**: Exposes Model Context Protocol tools, resources, and prompts at `POST /mcp` (JSON-RPC 2.0).
- **Auto OpenAPI & Swagger UI**: Serves interactive Swagger docs at `/docs` and raw OpenAPI schemas at `/openapi.json`.
- **Advanced I/O & Streaming**: Native chunked `StreamingResponse`, multipart `UploadFile` support, and full-duplex `WebSocket` connections.
- **Production Ergonomics**: Modular `APIRouter`, lifecycle hooks (`startup`, `shutdown`), multi-worker process management (`workers=N`), and auto-reloader (`reload=True`).

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

@app.tool()
def add_numbers(a: int, b: int) -> int:
    """Add two numbers (Exposed via MCP server at /mcp)."""
    return a + b

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000, reload=True)
```

---

## ⚡ FastAPI vs RustAPI: Migration & Comparison Guide

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
    doc = req.files["document"][0]
    content = doc.read().decode("utf-8")
    return {"filename": doc.filename, "description": req.form.get("description"), "content": content}

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
db = app.connect_db("sqlite::memory:") # or "postgres://user:pass@localhost/db"

db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob')")

@app.get("/users")
def get_users():
    # Executes SQL in Rust and streams JSON directly to HTTP response
    return db.query_json("SELECT * FROM users")
```

---

### §6. Embedded Rust Power Primitives (JWT, Argon2, MiniJinja)

RustAPI exposes native, zero-dependency C-speed security and templating primitives directly to Python:

```python
from rustapi import encode_jwt, decode_jwt, hash_password, verify_password, render_template

# 1. Native Rust JWT Engine (HS256, HS384, HS512)
token = encode_jwt({"user_id": 42, "role": "admin"}, secret="secret_key")
claims = decode_jwt(token, secret="secret_key")

# 2. Native Argon2 Password Hashing (releasing GIL on Tokio worker pool)
pw_hash = hash_password("MySecurePassword123!")
is_valid = verify_password("MySecurePassword123!", pw_hash)

# 4. Native Template Rendering & Specialized Responses
rendered = render_template("<h1>Hello {{ name }}!</h1>", {"name": "Boopathi"})

# Dedicated Response Wrappers
html_resp = HTMLResponse(rendered)            # Content-Type: text/html
json_resp = JSONResponse({"status": "ok"})     # Content-Type: application/json
text_resp = PlainTextResponse("Hello")        # Content-Type: text/plain
redir_resp = RedirectResponse("/docs")        # Status: 307, Location: /docs
```

#### Request JSON & Form Parsing (`req.json()`, `req.form`)

Handlers can accept incoming requests and access both JSON bodies and form fields easily:

```python
@app.post("/auth/login")
def login(req):
    # Option 1: Parse JSON request body
    data = req.json()
    username = data.get("username")
    
    # Option 2: Parse Form or Query parameters
    form = req.form
    password = form.get("password", "")
    
    # Process security & return response
    h = hash_password(password)
    token = encode_jwt({"sub": username}, secret="key")
    return JSONResponse({"token": token, "hash": h})
```

---

### §7. Real-Time Access Logging & Telemetry

RustAPI automatically outputs structured, high-speed terminal access logs for every incoming HTTP request with zero overhead:

```text
INFO:     Started server process [80839]
INFO:     RustAPI server running on http://127.0.0.1:8000 (Press CTRL+C to quit)
INFO:     127.0.0.1:54321 - "GET /docs HTTP/1.1" 200 - 0.85ms
INFO:     127.0.0.1:54322 - "POST /auth/login HTTP/1.1" 200 - 4.12ms
INFO:     127.0.0.1:54323 - "GET /invalid HTTP/1.1" 404 - 0.15ms
```

---

### §5. Production Ergonomics (APIRouter & Lifespan Hooks)

#### FastAPI
```python
from fastapi import FastAPI, APIRouter

app = FastAPI()
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

#### RustAPI
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

### §6. Model Context Protocol (MCP) Server Integration

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
- `StreamingResponse`: Generator-backed HTTP chunked streaming response.
- `HTTPException`: Standard HTTP exception with `status_code` and `detail`.
- `Depends`: Dependency injection helper.
- `BackgroundTasks`: Helper for scheduling background task execution.
- `UploadFile`: Wrapper for multipart uploaded file streams (`read()`, `filename`, `content_type`).
- `WebSocket`: Full-duplex WebSocket object (`receive_text()`, `send_text()`).
