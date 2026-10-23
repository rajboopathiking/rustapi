# PyRustAPI Maximum Native Rust Architecture Benchmark Report

This report documents empirical, reproducible performance benchmarks comparing **Standard FastAPI (Uvicorn + Starlette)** against **PyRustAPI (Maximum Native Rust Release Build)** under high concurrency (50 parallel workers) using `oha`.

---

## 📊 Benchmark Summary Matrix

| Workload Scenario | FastAPI (Uvicorn) | PyRustAPI (Native Rust) | **Real-World Speedup** | Notes |
| :--- | :---: | :---: | :---: | :--- |
| **DDoS / Rate Limit Rejection (`/rate-limit`)** | **2,092.4 req/s** *(24.0 ms)* | **62,723.6 req/s** *(0.79 ms)* | **🚀 +2,897.6% (30x faster)** | Handled in Tokio before Python wakes up |
| **SIMD Vector Search (10,000 vectors x 128 dim)** | **247.45 ms** *(Python loop)* | **23.36 ms** *(AVX2 registers)* | **🧠 +959.0% (10.5x faster)** | Native SIMD vector distance |
| **Database Query (`/sql` SQLx vs SQLite)** | **258.2 req/s** *(201.5 ms)* | **1,524.0 req/s** *(32.8 ms)* | **⚡ +490.2% (6x higher throughput)** | Zero-copy wire protocol streaming |
| **Standard JSON Response (`/json`)** | **2,609.5 req/s** *(19.2 ms)* | **7,732.2 req/s** *(6.4 ms)* | **🔥 +196.3% (3x faster)** | Hyper Ingress + orjson serialization |
| **In-Memory Cache Hit (`/cached`)** | **2,394.2 req/s** *(20.9 ms)* | **5,754.9 req/s** *(8.6 ms)* | **💾 +140.4% (2.4x faster)** | Lock-free DashMap in-memory cache |

---

## 🛠️ Key Architectural Drivers

1. **Direct SQL Fast-Paths (`@app.get(path, sql=...)`)**:
   - Eliminates intermediate Python object allocations and ORM overhead.
   - Database rows from SQLite/PostgreSQL are serialized directly into Hyper's network response buffer.

2. **Native Token-Bucket Rate Limiter (`rate_limit="100/minute"`)**:
   - Enforces per-IP token buckets directly in Rust.
   - Rejects abusive traffic with `429 Too Many Requests` in **under 50 microseconds**, shielding Python from DDoS traffic.

3. **In-Memory TTL Caching (`cache_ttl=...`)**:
   - Cache hits return directly from Rust RAM with `x-cache: HIT` header in **< 100 microseconds**.

4. **SIMD Vector Similarity Engine (`vector_search`, `cosine_similarity`)**:
   - Leverages CPU vector instructions (AVX2 / ARM NEON) to calculate multiple float dot products simultaneously per CPU cycle.

5. **High-Concurrency WebSockets & Pub/Sub Broadcaster**:
   - Native `receive_json()`, `send_json()`, `receive_bytes()`, `send_bytes()`, `close()`, and `subscribe(topic)`.
   - `app.broadcast(topic, data)` fans out messages natively via Tokio broadcast channels.

6. **Zero-Copy Disk Streaming (`file.save_to`)**:
   - Streams file uploads directly to disk using buffered OS I/O while releasing the Python GIL (`py.allow_threads`).
   - Keeps RAM consumption flat at **~15 MB** even for multi-gigabyte uploads.

7. **Native Server-Sent Events (SSE)**:
   - `@app.get(path, sse="topic")` streams broadcast events over `text/event-stream` with zero Python loop overhead.

---

## 🔬 Reproducing These Benchmarks

Run the benchmark script directly from your repository:

```bash
# Build release wheel
maturin build --release -i python3.11
pip install dist/*.whl --force-reinstall

# Run benchmark
python3.11 benchmarks/run_max_architecture_benchmark.py
```
