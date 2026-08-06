# RustAPI

![RustAPI Logo](https://raw.githubusercontent.com/rajboopathiking/rustapi/master/pyrustapi-logo.png)


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
| **FastAPI Compatibility** | `FastAPI` & `Request` aliases, `status` codes, `EventSourceResponse` (SSE), `jsonable_encoder`, `Query`/`Body`/`Path`, `OAuth2PasswordBearer`, `HTTPBearer`, `APIKeyHeader`, `OpenIdConnect`, `get_openapi`, `app.frontend()`. |
| **HTTP & Routing** | FastAPI-style `@app.get`, `@app.post`, `APIRouter`, `Depends`, `app.dependency_overrides`, `response_model`, `reload=True`. |
| **Rust-Native Tier 3 Routes** | `app.add_native_route()` zero-GIL C-speed fast-paths (**50,000+ req/sec**). |
| **Database Engine** | `sqlx` SQLite & PostgreSQL pool (`app.connect_db()`) with zero-copy JSON streaming (`db.query_json()`). |
| **Power Primitives** | Native Rust `encode_jwt()` / `decode_jwt()`, Argon2 `hash_password()` / `verify_password()`, MiniJinja `render_template()`. |
| **Response Classes** | `HTMLResponse`, `JSONResponse`, `PlainTextResponse`, `RedirectResponse`, `StreamingResponse`, `EventSourceResponse`. |
| **Request Ergonomics** | `req.json()`, `req.form`, `req.body`, `UploadFile`, `WebSocket`. |
| **AI / MCP Integration** | Embedded Model Context Protocol server (`@app.tool()`, `@app.resource()`, `@app.prompt()`) at `POST /mcp`. |
| **Telemetry & Observability** | Terminal access logging (`INFO: 127.0.0.1 - "GET /render HTTP/1.1" 200 - 0.45ms`), Swagger UI (`/docs`), OpenAPI (`/openapi.json`). |

---

### ⚡ Framework Standing & Capability Comparison

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

| Capability | FastAPI | RustAPI (`pyrustapi`) | `rustapi` Advantage |
| :--- | :---: | :---: | :--- |
| **API Syntax (`@app.get`, `Depends`, `APIRouter`)** | ✅ | ✅ | 100% drop-in API compatibility |
| **Interactive Docs (`/docs`, `/redoc`)** | ✅ | ✅ | Swagger UI & ReDoc served natively |
| **SSE Real-Time AI Streaming** | ✅ | ✅ | Low-latency token streaming for LLMs |
| **Single-Page App Serving (`app.frontend`)** | ❌ Manual | ✅ | Zero-config React/Vite/Svelte dist serving |
| **Embedded DB Engine (`sqlx`)** | ❌ None | ✅ | Native C/Rust DB pool with zero-copy JSON |
| **Rust Security Primitives (Argon2/JWT)** | ❌ External | ✅ | Built-in high-speed C-extension primitives |
| **Model Context Protocol (MCP Server)** | ❌ External | ✅ | Built-in AI Agent MCP server (`POST /mcp`) |
| **Peak Throughput (Native Fast-Paths)** | ~5k req/s | **50,000+ req/s** | Zero-GIL Tokio execution for critical routes |

---

## 📚 Documentation Reference

Detailed documentation is available in the [`docs/`](docs/) directory and project reference:
- [📖 Getting Started & Core Routing](docs/getting_started.md)
- [⚡ FastAPI Compatibility & Feature Guide](docs/fastapi_compatibility_and_features.md)
- [📚 Complete API & Migration Guide](docs/docs.md)
- [🏛️ 3-Tier Architecture Guide](docs/3tier_architecture_and_benchmarks.md)
- [⚡ Tier 3 Rust-Native Business Logic Guide](docs/native_business_logic.md)
- [🗄️ Rust-Native Database Engine](docs/database_engine.md)
- [🔐 Embedded Rust Power Primitives (JWT, Argon2, MiniJinja)](docs/power_primitives.md)
- [🤖 Model Context Protocol (MCP) Server](docs/mcp_server.md)
- [📦 Response Types & Request Ergonomics](docs/response_types.md)
- [📊 Swagger UI & DX Deviations Guide](docs/swagger_ui_and_dx_deviations.md)
- [📊 Empirical Performance Benchmark Report](performance-reports/fastapi_vs_hybrid_vs_native.md)

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
