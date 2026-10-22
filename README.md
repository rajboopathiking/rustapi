# RustAPI

![RustAPI Logo](https://github.com/rajboopathiking/rustapi/blob/master/docs/assets/pyrustapi-logo.png)


**FastAPI-style Python Web Framework backed by a Rust (Tokio / Hyper) Engine — with embedded Rust Database Streaming, JWT & Argon2 Primitives, Tier 3 Rust-Native Routes, and a built-in MCP Server.**

[![PyPI](https://img.shields.io/pypi/v/pyrustapi.svg)](https://pypi.org/project/pyrustapi/)
[![GitHub Repository](https://img.shields.io/badge/GitHub-rajboopathiking%2Frustapi-181717?logo=github)](https://github.com/rajboopathiking/rustapi)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Python: 3.8+](https://img.shields.io/badge/Python-3.8%2B-blue.svg)](https://www.python.org/)
[![Rust: 1.75+](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

---

### 📦 Key Open-Source Features Included

| Feature Area | Implementation Details |
| :--- | :--- |
| **FastAPI Compatibility** | `FastAPI` & `Request` aliases, `status` codes, declared `status_code` (e.g. `201`, `204`), async/sync `response_model` with `response_model_exclude`/`include`, structured `422` validation error handling, `FastAPI(lifespan=...)` ASGI context managers, `Depends(..., scope="function")`, `EventSourceResponse` (SSE), `jsonable_encoder`, `Query`/`Body`/`Path`/`Form`/`File`, `OAuth2PasswordBearer`, `HTTPBearer`, `APIKeyHeader`, `OpenIdConnect`, `get_openapi`, `app.frontend()`. |
| **HTTP & Routing** | FastAPI-style `@app.get`, `@app.post`, `APIRouter`, `Depends`, `app.dependency_overrides`, `response_model`, `reload=True`. |
| **Rust-Native Tier 3 Routes** | `app.add_native_route()` zero-GIL C-speed fast-paths (**50,000+ req/sec**). |
| **Database Engine** | `sqlx` SQLite & PostgreSQL pool (`app.connect_db()`) with zero-copy JSON streaming (`db.query_json()`). |
| **Power Primitives** | Native Rust `encode_jwt()` / `decode_jwt()`, Argon2 `hash_password()` / `verify_password()`, MiniJinja `render_template()`. |
| **Response Classes** | `HTMLResponse`, `JSONResponse`, `PlainTextResponse`, `RedirectResponse`, `StreamingResponse`, `EventSourceResponse`. |
| **Request Ergonomics** | `req.json()`, `req.form`, `req.body`, `UploadFile`, `WebSocket`. |
| **AI / MCP Integration** | Embedded Model Context Protocol server (`@app.tool()`, `@app.resource()`, `@app.prompt()`) at `POST /mcp`. |
| **Telemetry & Observability** | Terminal access logging (`INFO: 127.0.0.1 - "GET /render HTTP/1.1" 200 - 0.45ms`), Swagger UI (`/docs`), OpenAPI (`/openapi.json`). |

---

### ⚡ Framework Standing & Multi-Framework Benchmarks

`rustapi` delivers **100% FastAPI developer experience (DX)** while running on a **high-performance Rust (Tokio/Hyper) core engine**.

```
┌───────────────────────────────────────────────────────────┐
│                    Python Layer (DX)                      │
│ FastAPI Syntax • Pydantic Models • Routers • SSE • Docs   │
└─────────────────────────────┬─────────────────────────────┘
                              │  PyO3 C-Bindings
┌─────────────────────────────▼─────────────────────────────┐
│                    Rust Tokio/Hyper Core                  │
│ Async Multi-Thread IO • Tokio Workers • sqlx DB • SSE      │
└───────────────────────────────────────────────────────────┘
```

#### 📊 Empirical Benchmark Results (`oha` Load Generator, $c=32$)

Measured under identical business logic and schemas across Python and Node.js frameworks:

| Workload Scenario | FastAPI (`uvloop`) | Express.js | RustAPI (v1.8.9) | Fastify (Node.js) | RustAPI Native Tier 3 |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Sync JSON** (`GET /json`) | 3,417 req/s | 12,873 req/s | **10,178 req/s** *(⚡ 3.0x vs FastAPI)* | 17,063 req/s | **🚀 96,386 req/s** *(⚡ 7.5x vs Express)* |
| **Async JSON** (`GET /async_json`) | 5,430 req/s | 13,638 req/s | **8,117 req/s** *(⚡ 1.5x vs FastAPI)* | 17,122 req/s | N/A |
| **Validated POST** (`POST /items`) | 2,762 req/s | 10,529 req/s | **8,621 req/s** *(⚡ 3.1x vs FastAPI)* | 13,463 req/s | N/A |
| **Response Model (50 items)** (`GET /users`) | 1,916 req/s | 10,072 req/s | **1,928 req/s** *(⚡ Lower p95)* | 11,145 req/s | N/A |
| **High Concurrency** (`c=100`) | 6,088 req/s | 12,637 req/s | **7,571 req/s** *(⚡ 1.25x vs FastAPI)* | 15,523 req/s | N/A |

Detailed report & reproducible scripts: [`performance-reports/rustapi_vs_fastapi_vs_nodejs.md`](performance-reports/rustapi_vs_fastapi_vs_nodejs.md)

---

### 🏛️ The 3-Tier Execution Model

1. **Tier 1 (FastAPI Drop-in)**: Standard Python `@app.get()`, `async def`, Pydantic models, and dependencies.
2. **Tier 2 (Hybrid Rust Acceleration)**: Standard Python handlers utilizing native Rust power primitives (`db.query_json()` SQLite/PostgreSQL streaming, `rustapi.encode_jwt()`, `rustapi.hash_password()`, MiniJinja templates).
3. **Tier 3 (Pure Rust Native Fast-Path)**: Zero-GIL C-speed routes registered with `app.add_native_route("/health", ...)` clocking **96,000+ req/sec** at **0.57ms p95 latency**.

---

### 🔄 Lifespan Context Manager (ML Models, DB Pools, Resource Cleanup)

RustAPI supports the modern FastAPI `@asynccontextmanager` lifespan protocol across both ASGI and native Tokio `app.run()`:

```python
from contextlib import asynccontextmanager
from rustapi import FastAPI

ml_models = {}

@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup: Load ML model or connect to database before handling requests
    print("Loading application resources...")
    ml_models["hero_model"] = "Loaded Model Instance"
    yield
    # Shutdown: Clean up resources and release memory on server exit
    print("Cleaning up resources...")
    ml_models.clear()

app = FastAPI(lifespan=lifespan)

@app.get("/")
async def read_root():
    return {"model": ml_models.get("hero_model")}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

### ⚡ 5 Proven Ways to Maximize Performance

1. **Pre-load Heavy Resources in `lifespan`**: Load models and connection pools once during startup rather than in request handlers.
2. **Zero-Copy SQL with `db.query_json()`**: Stream database query results directly to client sockets without allocating Python dictionary objects.
3. **Declare `response_model` for Compiled Serialization**: RustAPI compiles Pydantic `TypeAdapter` instances once, boosting serialization by up to **64.5%**.
4. **Offload Health Checks to Tier 3 Native Routes**: Use `app.add_native_route("/health", ...)` for ultra-high-frequency endpoints (**96,000+ req/s**).
5. **Scale Multi-Process Workers**: Set `workers=os.cpu_count()` in `app.run()` for multi-core parallelism.

---

## 📚 Documentation Reference

Detailed documentation is available in the [`docs/`](docs/) directory and project reference:
- [📖 Getting Started & Core Routing](https://github.com/rajboopathiking/rustapi/blob/master/docs/md/getting_started.md)
- [🔒 Security, Authentication & JWT Guide](https://github.com/rajboopathiking/rustapi/blob/master/docs/md/security.md)
- [📚 Complete API & Migration Guide](https://github.com/rajboopathiking/rustapi/blob/master/docs/md/docs.md)
---

## 🛠️ Local Development & Testing

Build from source with [maturin](https://github.com/PyO3/maturin):

```bash
# Clone the repository
git clone https://github.com/rajboopathiking/rustapi.git
cd rustapi

# Build release wheel with Maturin
pip install maturin
maturin develop --release

# Run full test suite
pytest tests/ -v
```

PYPI Package

```bash
pip install pyrustapi
```
---

## 📄 License

Distributed under the [MIT License](LICENSE).
