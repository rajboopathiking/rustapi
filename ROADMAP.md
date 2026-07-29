# 🚀 RustAPI Engineering Roadmap

This document tracks the verified state of RustAPI and the phased implementation plan to reach full feature-parity with production Python frameworks (like FastAPI) without sacrificing Rust's native speed.

---

## §0. Ground Truth (Completed & Verified)

- [x] **Hyper HTTP Engine & Radix Routing:** Core TCP socket binding, high-speed URL matching.
- [x] **Sync/Async Route Offloading:** `def` and `async def` routes executing securely inside Tokio pool.
- [x] **HTTP Essentials & OpenAPI:** All standard methods (`GET`, `POST`, `PUT`, `DELETE`, `PATCH`), CORS, `/docs` (Swagger UI), and `/openapi.json`.
- [x] **Phase 1: HTTP Metadata & Error Handling:** `req.headers`, `req.cookies`, `Response` objects, structured `HTTPException`, and automatic 422 validation via Pydantic.
- [x] **Phase 2: Dependency Injection & Generators:** FastAPI-style `Depends`, generator setup/teardown hooks with request-scoped caching.
- [x] **Phase 5: Production Ergonomics:** Modular routing via `APIRouter` and lifespan hooks (`@app.on_event("startup")` / `"shutdown"`).
- [x] **Advanced I/O & Real-Time:** Chunked `StreamingResponse`, native multipart file uploads (`UploadFile`), and WebSockets (`WebSocket`).
- [x] **MCP Integration:** Model Context Protocol tools (`@app.tool()`), resources (`@app.resource()`), and prompts (`@app.prompt()`) at `POST /mcp`.

---

## 🎯 NEXT IMPLEMENTATION PHASES (Ordered: Simple ➔ Complex)

### 🟢 Phase A: Quick Wins & Testing Utilities (Simple)
*Goal: Bridge remaining API ergonomics for testing and data filtering.*

- [x] **A.1 Dependency Overrides (`app.dependency_overrides`):**
  - Add `engine.dependency_overrides` dictionary allowing tests to swap dependencies (e.g. `app.dependency_overrides[get_db] = get_test_db`).
- [x] **A.2 Response Model Filtering (`response_model=...`):**
  - Support `@app.get("/", response_model=UserOut)` to validate & filter handler return values through a Pydantic schema before serialization.
- [x] **A.3 Strict Parameter Type Coercion:**
  - Auto-convert path/query params to `int`, `float`, `bool` based on Python type hints, returning structured 422 errors on mismatch or missing required parameters.

---

### 🟡 Phase B: Rust-Native Database Engine (Medium-Complex)
*Goal: Bypass the Python GIL and standard ORM overhead for high-concurrency database queries.*

- [x] **B.1 Native Rust Connection Pool:** Embed `sqlx` inside `Engine` for high-concurrency PostgreSQL/SQLite connection pooling.
- [x] **B.2 Zero-Copy JSON Streaming:** Execute SQL natively in Rust and stream UTF-8 JSON bytes directly to the client socket (skipping Python dict & Pydantic allocations entirely).
- [x] **B.3 Python Orchestration API:** Expose `app.connect_db()` and `db.query_json()` / `db.execute()` to the Python layer.

---

### 🔴 Phase C: Embedded High-Performance Rust Power Modules (Complex)
*Goal: Provide instant, zero-dependency, ultra-fast primitives natively in Rust for security, templating, and memory efficiency.*

- [x] **C.1 Rust JWT Engine (`jsonwebtoken`):** Native `encode_jwt()` / `decode_jwt()` inside Rust, bypassing Python `pyjwt` latency.
- [x] **C.2 High-Speed Password Hashing (`argon2`):** Embedded `hash_password()` and `verify_password()` natively running on Tokio blocking worker pools.
- [x] **C.3 Native Template Renderer (`minijinja`):** Jinja2-compatible template engine executing directly in Rust memory.
- [x] **C.4 High-Performance Allocator & Data Structures:** Integrate `mimalloc` and lock-free `DashMap` for maximum multi-threaded throughput.

---

### 🔵 Phase D: Native Request Telemetry & Access Logging (Completed)
*Goal: Provide Uvicorn/FastAPI-style real-time terminal access logs with zero runtime performance cost.*

- [x] **D.1 Native Hyper Access Logger:** Automatically logs remote IP, HTTP method, request path, status code, and latency in milliseconds for every request (`INFO: 127.0.0.1:54321 - "GET /docs HTTP/1.1" 200 - 0.85ms`).

---

## § Architectural Boundaries & Trade-Offs

- **No ASGI Middleware:** RustAPI does not run under Uvicorn/Gunicorn. Core middleware (Auth, CORS, Logging) runs in the Rust layer to preserve speed.
- **The Python GIL Ceiling:** RustAPI eliminates framework overhead. Heavy CPU-bound Python loops should leverage Phase C Rust modules to bypass the GIL.
