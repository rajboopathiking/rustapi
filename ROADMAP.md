# 🚀 RustAPI Engineering Roadmap

This document tracks the verified state and architecture of **RustAPI** — a **Native-Rust First Framework** designed to deliver full feature parity with Python frameworks (like FastAPI) while running core business logic, database queries, security primitives, and high-speed routing directly in native Rust.

---

## §0. Ground Truth & Architecture (Completed & Verified)

RustAPI follows a **3-Tier Architecture** that combines FastAPI's intuitive Python surface with high-performance native Rust execution engines.

### ⚡ 1. Native-Rust First Framework & Business Logic Engine
- [x] **Tier 3 Rust-Native Routes (`app.add_native_route`):** Pure Rust HTTP fast-paths executing 100% inside Tokio & Hyper in compiled machine code, completely bypassing the CPython interpreter and GIL (**50,000+ req/sec**).
- [x] **PyO3 Native Business Logic Support:** Ability to write compiled Rust PyO3 C-extensions (`py.allow_threads`) for heavy CPU-bound business logic, offloading compute out of Python's GIL.
- [x] **Hyper HTTP Engine & Radix Router:** High-concurrency TCP socket handling, zero-copy header parsing, and low-latency URL matching natively in Rust.
- [x] **Sync & Async Handler Offloading:** Native execution of `def` and `async def` Python route handlers managed securely by Tokio worker pools.

---

### 🗄️ 2. Rust-Native Database Engine
- [x] **Native Rust Connection Pooling (`sqlx`):** High-concurrency PostgreSQL and SQLite connection pooling embedded directly inside the `Engine` via `app.connect_db()`.
- [x] **Zero-Copy JSON Streaming (`db.query_json()`):** SQL queries execute natively in Rust and stream UTF-8 JSON bytes directly to client sockets, bypassing Python dict and Pydantic object allocation overhead.
- [x] **Python DB Orchestration API:** Direct methods `db.execute()`, `db.fetch_one()`, and `db.fetch_all()` exposed to Python handlers.

---

### 🔐 3. Embedded Rust Power Modules (Zero Dependency)
- [x] **Rust JWT Engine (`jsonwebtoken`):** Native `encode_jwt()` and `decode_jwt()` functions inside Rust, eliminating Python `pyjwt` latency.
- [x] **High-Speed Password Hashing (`argon2`):** Embedded `hash_password()` and `verify_password()` executing on Tokio blocking worker pools.
- [x] **Native Template Renderer (`minijinja`):** Jinja2-compatible template engine rendering directly inside Rust memory.
- [x] **High-Performance Memory & Concurrent Maps:** Integrated `mimalloc` memory allocator and lock-free `DashMap` for multi-threaded state management.

---

### 🛠️ 4. FastAPI Feature Parity & Ergonomics
- [x] **HTTP Method Routing & APIRouter:** Support for `@app.get`, `@app.post`, `@app.put`, `@app.delete`, `@app.patch`, and modular `APIRouter` mounting.
- [x] **Dependency Injection System (`Depends`):** Full FastAPI-compatible dependency resolution, generator setup/teardown hooks, request-scoped caching, and test overrides (`app.dependency_overrides`).
- [x] **Response Model Filtering & Validation (`response_model=...`):** Schema validation and field filtering using Pydantic models before response serialization.
- [x] **Strict Parameter Type Coercion:** Automatic casting of path and query params (`int`, `float`, `bool`) with structured 422 HTTP validation error responses.
- [x] **Request & Response Ergonomics:** `req.json()`, `req.body`, `req.headers`, `req.cookies`, `JSONResponse`, `HTMLResponse`, `PlainTextResponse`, `RedirectResponse`, `StreamingResponse`, `UploadFile`, and `WebSocket`.
- [x] **OpenAPI & Interactive Docs:** Automatic `/openapi.json` generation and embedded Swagger UI at `/docs`.
- [x] **Swagger UI Security & Interactive Authorize (v0.7.86):** Automatic `components.securitySchemes` generation and interactive top **Authorize** lock button (🔒) in Swagger UI (`/docs`) for all 8 `rustapi.security` schemes.
- [x] **Native Telemetry & Access Logging:** Real-time terminal request access logs (`INFO: 127.0.0.1 - "GET /docs HTTP/1.1" 200 - 0.85ms`).
- [x] **Model Context Protocol (MCP) Server:** Embedded MCP tools (`@app.tool()`), resources (`@app.resource()`), and prompts (`@app.prompt()`) accessible at `POST /mcp`.

---

## 🎯 IMPLEMENTATION PHASES & VERIFICATION (Completed)

### 🟢 Phase A: Quick Wins & Testing Utilities
- [x] **A.1 Dependency Overrides (`app.dependency_overrides`):** Swap dependencies during testing (e.g., mock database connections).
- [x] **A.2 Response Model Filtering (`response_model=...`):** Validate & filter return dictionaries via Pydantic schemas.
- [x] **A.3 Strict Parameter Type Coercion:** Validate path/query types and yield automatic 422 error payloads.

---

### 🟡 Phase B: Rust-Native Database Engine
- [x] **B.1 Native Rust Connection Pool:** Integrated `sqlx` inside `Engine` for PostgreSQL/SQLite.
- [x] **B.2 Zero-Copy JSON Streaming:** Direct socket streaming of SQL query results as JSON.
- [x] **B.3 Python Orchestration API:** Exposed `app.connect_db()` and `db.query_json()` / `db.execute()`.

---

### 🔴 Phase C: Embedded High-Performance Rust Power Modules
- [x] **C.1 Rust JWT Engine (`jsonwebtoken`):** Native `encode_jwt()` / `decode_jwt()`.
- [x] **C.2 High-Speed Password Hashing (`argon2`):** Embedded Argon2 password hashing.
- [x] **C.3 Native Template Renderer (`minijinja`):** Jinja2-compatible template engine in Rust.
- [x] **C.4 High-Performance Allocator & Data Structures:** Integrated `mimalloc` and `DashMap`.

---

### 🔵 Phase D: Native Request Telemetry & Access Logging
- [x] **D.1 Native Hyper Access Logger:** Terminal access logging for remote IP, HTTP method, path, status, and latency.

---

## § Architectural Boundaries & Trade-Offs

- **No ASGI Middleware Overhead:** RustAPI does not run under Uvicorn or Starlette. Core middleware (CORS, Auth, Telemetry, Logging) runs directly in the Rust layer to maximize speed.
- **The Python GIL Ceiling & Native Fast-Paths:** RustAPI eliminates framework overhead. Heavy CPU-bound business logic and database hot-paths can leverage Tier 3 Native Routes (`app.add_native_route`), native DB streaming, or PyO3 C-extensions to completely bypass the GIL.
