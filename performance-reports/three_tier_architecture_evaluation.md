# 🏛️ RustAPI 3-Tier Architecture Evaluation & Benchmark

This report documents the empirical evaluation of **RustAPI's 3-Tier Architecture**, demonstrating how developers can transition from standard Python code to pure C-speed Rust performance based on application needs.

## 🛠️ How Developers Use Each Tier in RustAPI

### 1. Tier 1: Pure Python Business Logic (FastAPI Compatibility)
Use standard Python `def` functions, Pydantic schemas, and ORMs for rapid feature iteration:
```python
@app.get("/users/{user_id}")
def get_user(user_id: int):
    return {"user_id": user_id, "status": "active"}
```

### 2. Tier 2: Hybrid Python Surface + Rust Power Primitives (RustAPI Default)
Write Python handlers but delegate heavy tasks (SQL streaming, JWT signing, password hashing, HTML rendering) to built-in Rust engines:
```python
@app.get("/users")
def list_users():
    # Executes query in Rust sqlx and streams JSON bytes directly to TCP socket
    return db.query_json("SELECT * FROM users")

@app.get("/render")
def render():
    # MiniJinja template rendering in Rust memory
    return rustapi.HTMLResponse(rustapi.render_template("<h1>Hello { name }</h1>", {"name": "Boopathi"}))
```

### 3. Tier 3: Rust-Native Business Logic (Pure Rust C-Speed Performance)
Write hot-path business logic in Rust (using PyO3 or native Rust routes) for full developer control and zero Python GIL overhead:
```python
# 1. Native Fast-Path Route
app.add_native_route("/fast-path", '{"status": "success", "tier": 3}')

# 2. PyO3 Native C-Extension Business Logic
import my_rust_extension
score = my_rust_extension.compute_heavy_analytics(data)
```
