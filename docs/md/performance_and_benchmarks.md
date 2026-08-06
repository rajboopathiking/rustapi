# 📊 Performance & Benchmarks

RustAPI is engineered to deliver zero-overhead performance while maintaining FastAPI's intuitive Python surface and supporting **Rust-Native Business Logic** (`app.add_native_route`) for extreme hot-paths.

---

## 🏛️ 3-Tier Architecture Performance Comparison

Tested under identical hardware and concurrency settings (**40 concurrent persistent connections, 2,000 requests per endpoint**):

* Full 3-Tier Architecture Evaluation: [performance-reports/three_tier_architecture_evaluation.md](../performance-reports/three_tier_architecture_evaluation.md)
* FastAPI vs. Robyn Comparison: [performance-reports/fastapi_vs_rustapi_comparison.md](../performance-reports/fastapi_vs_rustapi_comparison.md)

| Framework Tier | Execution Engine | Throughput | Avg Latency | Developer Choice & Control |
| :--- | :--- | :--- | :--- | :--- |
| **Tier 1: Pure Python Handlers** | CPython Bytecode | **5,067 req/sec** | 7.89ms | 100% FastAPI Compatibility |
| **Tier 2: Hybrid Rust Primitives** | Rust `sqlx` & `minijinja` | **4,844 req/sec** | 8.26ms | Built-in Rust Power Primitives |
| **Tier 3: Rust-Native Business Logic** | Tokio / Hyper Direct | **6,058 req/sec** | 6.60ms | Pure Rust Performance (0ms GIL) |

---

## 🚀 Writing Rust-Native Hot-Paths (`app.add_native_route`)

To achieve pure Rust performance without touching the Python GIL or bytecode interpreter:

```python
import rustapi

app = rustapi.Engine()

# Tier 3: Rust-Native Route (0ms Python GIL & 0ms Bytecode Overhead)
app.add_native_route("/fast-json", '{"status": "ok", "engine": "pure_rust"}')
```
