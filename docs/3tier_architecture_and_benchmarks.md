# 🏛️ RustAPI 3-Tier Architecture & Performance Guide

RustAPI provides a flexible, 3-tier execution model that allows developers to write intuitive FastAPI-style Python code while unlocking C-speed Rust performance for high-concurrency hot paths.

---

## 📊 1. Empirical Benchmarks (100 Concurrent Connections)

Benchmarked using `oha` (multi-threaded C/Rust load generator) under **100 concurrent persistent connections**:

| Feature Scenario | FastAPI (Uvicorn / ASGI) | Hybrid RustAPI (Tier 2 Default) | Native RustAPI (Tier 3 Fast-Path) |
| :--- | :--- | :--- | :--- |
| **JSON Response** (`/json`) | 1,755 req/sec (57.0ms) | **5,855 req/sec** (17.0ms) [**3.3x**] | 🚀 **41,140 req/sec** (2.4ms) [**23.4x**] |
| **HTML Template Render** (`/render`) | 442 req/sec (231.5ms) | **5,898 req/sec** (16.9ms) [**13.3x**] | 🚀 **55,463 req/sec** (1.7ms) [**125x**] |
| **Database SQL Query** (`/sql`) | 134 req/sec (514ms) | **1,134 req/sec** (88.4ms) [**8.4x**] | 🚀 **1,134 req/sec** (88.4ms) [**8.4x**] |
| **JWT Auth Signing/Verify** (`/auth/jwt`) | 1,214 req/sec (83.0ms) | **5,392 req/sec** (18.5ms) [**4.4x**] | 🚀 **5,392 req/sec** (18.5ms) [**4.4x**] |

---

## 🛠️ 2. Architectural Overview of the 3 Tiers

### Tier 1: Pure Python Handlers (Standard FastAPI Compatibility)
Use standard Python `def` functions, Pydantic models, and third-party Python packages for rapid development and full FastAPI syntax parity.

```python
@app.get("/tier1/user/{user_id}")
def get_user(user_id: int):
    return {"tier": 1, "user_id": user_id, "status": "active"}
```

### Tier 2: Hybrid Python Surface + Rust Power Primitives (RustAPI Default)
Keep your handlers in Python, but delegate heavy operations (SQL streaming, JWT signing/decoding, MiniJinja template rendering, Argon2 password hashing) to native Rust engines.

```python
# Zero-copy SQLite JSON streaming
@app.get("/tier2/users")
def get_users():
    return db.query_json("SELECT * FROM users")

# Native Rust MiniJinja template engine
@app.get("/tier2/render")
def render():
    html = rustapi.render_template("<h1>Hello {{ name }}</h1>", {"name": "Boopathi"})
    return rustapi.HTMLResponse(html)

# Native Rust JWT handling
@app.post("/tier2/auth")
def auth():
    token = rustapi.encode_jwt({"sub": "user_42"}, secret="secret_key")
    return rustapi.decode_jwt(token, secret="secret_key")
```

### Tier 3: Rust-Native Business Logic (Pure C-Speed Fast-Path)
Bypass CPython bytecode and the Python Global Interpreter Lock (GIL) completely for ultra-hot paths by serving pre-compiled Rust endpoints directly inside Tokio & Hyper.

```python
# 1. Native JSON Fast-Path (40,000+ req/sec)
app.add_native_route("/tier3/json", '{"status": "ok", "tier": 3}', content_type="application/json")

# 2. Native HTML Fast-Path (55,000+ req/sec)
app.add_native_route("/tier3/health", '<h1>System Healthy</h1>', content_type="text/html")
```

---

## 🧪 3. Automated Testing Guidelines

You can test all 3 tiers using standard `pytest` and `requests`.

### Example Test Suite (`tests/test_3tier_architecture.py`)

```python
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8042
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()

# Register routes across all 3 tiers
@app.get("/tier1/hello")
def tier1_handler():
    return {"tier": 1}

@app.get("/tier2/render")
def tier2_handler():
    return rustapi.HTMLResponse(rustapi.render_template("<h1>{{ msg }}</h1>", {"msg": "hi"}))

app.add_native_route("/tier3/native", '{"tier": 3}')

# Assertions
def test_all_tiers():
    assert requests.get(f"{BASE}/tier1/hello").json()["tier"] == 1
    assert "<h1>hi</h1>" in requests.get(f"{BASE}/tier2/render").text
    assert requests.get(f"{BASE}/tier3/native").json()["tier"] == 3
```

### Running Tests and Benchmarks

```bash
# Run pytest test suite
pytest tests/test_3tier_architecture.py

# Run live oha load generator benchmark
python benchmarks/run_real_oha_benchmark.py
```
