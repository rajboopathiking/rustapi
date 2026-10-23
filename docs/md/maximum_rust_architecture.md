# ⚡ Maximum Native Rust Architecture & Performance Guide

PyRustAPI represents the next evolution of Python backend web frameworks. It allows developers to write standard, clean, familiar **FastAPI-compatible Python code** while pushing high-throughput, low-latency hot paths down into compiled, multithreaded **Rust (Tokio, Hyper, SQLx, and SIMD registers)**.

---

## 📊 1. Empirical Benchmark Results (PyRustAPI vs. FastAPI / Uvicorn)

Benchmarked under high concurrency (**50 parallel workers**) on identical hardware using the [`oha`](https://github.com/hatoo/oha) HTTP load generator:

| Workload Scenario | Standard FastAPI (Uvicorn) | PyRustAPI (Native Rust) | **Empirical Performance Boost** | Key Architectural Mechanism |
| :--- | :---: | :---: | :---: | :--- |
| **DDoS / Rate Limit Rejection** (`/rate-limit`) | **2,092.4 req/s** *(24.0 ms)* | **62,723.6 req/s** *(0.79 ms)* | **🚀 +2,897.6% (30x faster)** | Rejection in Tokio before Python coroutine starts |
| **SIMD Vector Search** (10,000 vectors × 128 dim) | **247.45 ms** *(Python loop)* | **23.36 ms** *(AVX2 registers)* | **🧠 +959.0% (10.5x faster)** | Native SIMD auto-vectorized dot-products |
| **Direct Database Query** (`/sql` SQLx vs SQLite) | **258.2 req/s** *(201.5 ms)* | **1,524.0 req/s** *(32.8 ms)* | **⚡ +490.2% (6x throughput)** | Direct socket wire-protocol streaming |
| **Standard JSON Response** (`/json`) | **2,609.5 req/s** *(19.2 ms)* | **7,732.2 req/s** *(6.4 ms)* | **🔥 +196.3% (3x faster)** | Hyper Ingress + PyO3 C-bindings |
| **In-Memory Cache Hit** (`/cached`) | **2,394.2 req/s** *(20.9 ms)* | **5,754.9 req/s** *(8.6 ms)* | **💾 +140.4% (2.4x faster)** | Lock-free DashMap cache (sub-100µs) |

---

## 🏛️ 2. The Core Architecture Pillars

```
┌────────────────────────────────────────────────────────────────────────┐
│                   Python Application Layer (FastAPI DX)                │
│  FastAPI Syntax • Pydantic Models • Dependency Injection • Routers     │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ PyO3 / GIL Control
┌───────────────────────────────────▼────────────────────────────────────┐
│                    PyRustAPI Tokio / Hyper Core Engine                 │
│                                                                        │
│  ┌───────────────────────┐  ┌───────────────────────┐                  │
│  │ Direct SQL Fast-Path  │  │ Lock-free TTL Cache   │                  │
│  │ (SQLx Socket Stream)  │  │ (DashMap < 100µs HIT) │                  │
│  └───────────────────────┘  └───────────────────────┘                  │
│  ┌───────────────────────┐  ┌───────────────────────┐                  │
│  │ Rate Limiter (DDoS)   │  │ SIMD Vector Search    │                  │
│  │ (Token Bucket < 50µs) │  │ (AVX2 / ARM NEON)     │                  │
│  └───────────────────────┘  └───────────────────────┘                  │
│  ┌───────────────────────┐  ┌───────────────────────┐                  │
│  │ Real-time WebSockets  │  │ Zero-Copy Disk Stream │                  │
│  │ & Pub/Sub Broadcaster │  │ (Flat 15MB RAM I/O)   │                  │
│  └───────────────────────┘  └───────────────────────┘                  │
│  ┌───────────────────────┐  ┌───────────────────────┐                  │
│  │ Native Server-Sent    │  │ Optional Hardware AI  │                  │
│  │ Events (SSE Broadcast)│  │ (ONNX Runtime Engine) │                  │
│  └───────────────────────┘  └───────────────────────┘                  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 🚀 3. Features & Code Examples

### 1. Direct SQL Fast Paths (Zero-GIL Streaming)
Bypass Python serialization overhead entirely for high-volume database queries. SQL results stream directly from the database driver into client TCP sockets.

```python
from rustapi import FastAPI

app = FastAPI()
app.connect_db("sqlite://app.db")

# Declarative route decorator
@app.get("/users", sql="SELECT id, username, email FROM users WHERE active = 1")
def get_users_fallback():
    # Never called if db is connected; executed directly in Tokio/Hyper
    pass

# Or programmatic route registration
app.add_sql_route("/products", "SELECT id, title, price FROM products LIMIT 100")
```

---

### 2. Lock-Free In-Memory TTL Caching
Caches JSON responses directly in the Rust core memory. Repeated requests return `x-cache: HIT` in **less than 100 microseconds**.

```python
@app.get("/market-rates", cache_ttl=60)
async def get_market_rates():
    # Expensive computation or third-party API call
    return {"EUR_USD": 1.0845, "BTC_USD": 94320.00}

# Programmatic cache management
app.clear_cache()
total_cached_items = app.cache_size()
```

---

### 3. Native Token-Bucket Rate Limiter (DDoS Shield)
Protect sensitive endpoints (e.g. login, payment, LLM inference). Unauthorized or excessive requests receive `429 Too Many Requests` in **under 50 microseconds**, never consuming Python thread pool or GIL time.

```python
# Format: "<count>/second", "<count>/minute", or "<count>/hour"
@app.post("/auth/login", rate_limit="10/minute")
async def login(credentials: LoginRequest):
    return {"token": "jwt_token"}
```

Response headers automatically included on rate-limited endpoints:
- `X-RateLimit-Limit`: Maximum requests allowed in the window.
- `X-RateLimit-Remaining`: Remaining capacity.
- `Retry-After`: Seconds until quota refreshes.

---

### 4. SIMD-Accelerated Vector Search & Similarity
Perform high-speed embedding similarity matching using auto-vectorized CPU SIMD instructions (AVX2, AVX-512, ARM NEON).

```python
import rustapi

# Compute cosine similarity between two 1D float vectors
similarity = rustapi.cosine_similarity([0.1, 0.5, 0.9], [0.2, 0.4, 0.8])

# Parallel top-k vector search across 10,000+ candidate embeddings
top_matches = rustapi.vector_search(
    query=[0.1, 0.2, 0.3],
    corpus=[[0.1, 0.2, 0.3], [0.9, 0.8, 0.7], [0.12, 0.22, 0.31]],
    top_k=2
)
# Returns list of tuples: [(index, score), ...]
```

---

### 5. High-Concurrency WebSockets & Pub/Sub Broadcaster
Full-duplex WebSocket communication with built-in topic subscription and multi-client broadcast channels.

```python
from rustapi import FastAPI, WebSocket

app = FastAPI()

@app.websocket("/ws/notifications")
async def ws_notifications(ws: WebSocket):
    await ws.accept()
    await ws.subscribe("system-alerts")
    
    while True:
        data = await ws.receive_json()
        await ws.send_json({"echo": data})

# Broadcast to all connected clients across threads without GIL overhead
app.broadcast("system-alerts", '{"alert": "High system load detected"}')
```

---

### 6. Zero-Copy Disk Streaming File Uploads
Save incoming file uploads directly to persistent storage via buffered OS I/O while releasing the Python GIL. Memory usage remains flat at **~15MB** regardless of file size.

```python
from rustapi import FastAPI, UploadFile, File

app = FastAPI()

@app.post("/upload")
async def upload_dataset(file: UploadFile = File(...)):
    dest_path = f"/var/data/{file.filename}"
    
    # Zero-copy stream directly to disk without loading into Python RAM
    bytes_written = await file.save_to(dest_path)
    
    return {
        "filename": file.filename,
        "bytes_written": bytes_written,
        "size": file.size
    }
```

---

### 7. Native Server-Sent Events (SSE)
Stream real-time updates to browser `EventSource` clients directly from Tokio broadcast channels.

```python
from rustapi import FastAPI

app = FastAPI()

# Stream topic events natively over HTTP text/event-stream
@app.get("/events/crypto", sse="crypto-feed")
def crypto_feed():
    pass

# Or programmatic registration
app.add_sse_route("/events/live", topic="live-updates")

# Emit events to all SSE subscribers
app.broadcast("crypto-feed", '{"symbol": "BTC", "price": 94100}')
```

---

### 8. Optional Hardware-Accelerated AI Inference
Optionally package ONNX Runtime with zero footprint on base installations:

```bash
pip install "pyrustapi[ai]"
```

```python
from rustapi import FastAPI

app = FastAPI()
session = app.load_onnx("models/transformer.onnx")
# Or: rustapi.load_onnx("models/transformer.onnx")
```

---

## 🧪 4. Running the Benchmark Suite

To reproduce the benchmark figures locally:

```bash
# 1. Install Maturin
pip install maturin

# 2. Compile optimized release build
maturin develop --release

# 3. Run full test gate
pytest tests/ -v

# 4. Run automated benchmark against FastAPI
python benchmarks/run_max_architecture_benchmark.py
```
