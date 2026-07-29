# 📊 Performance & Benchmarks

RustAPI is engineered to deliver zero-overhead performance while maintaining FastAPI's intuitive Python surface.

---

## ⚡ Comprehensive Release-Mode Benchmark: RustAPI vs FastAPI

Tested under identical hardware and concurrency settings (**40 concurrent persistent connections, 1,500 requests per endpoint**):

Full Report: [performance-reports/fastapi_vs_rustapi_comparison.md](../performance-reports/fastapi_vs_rustapi_comparison.md)

| Feature Scenario & Endpoint | FastAPI (Uvicorn / Python Stack) | RustAPI (Release Build / Tokio Core) | RustAPI Performance Advantage |
| :--- | :--- | :--- | :--- |
| **Basic JSON API Routing (`/json`)** | 1,819.49 req/sec (21.98ms) | **5,089.18 req/sec (7.86ms)** | ⚡ **2.80x Faster (280%)** |
| **HTML Template Render (`/render`)** | 355.88 req/sec (112.40ms) | **2,838.38 req/sec (14.09ms)** | ⚡ **7.98x Faster (798%)** |
| **JWT Security Validation (`/auth/jwt`)** | 871.34 req/sec (45.91ms) | **3,236.26 req/sec (12.36ms)** | ⚡ **3.71x Faster (371%)** |
| **Zero-Copy Database Stream (`/sql`)** | 190.89 req/sec (209.55ms) | **910.50 req/sec (43.93ms)** | ⚡ **4.77x Faster (477%)** |

---

## 🔑 Crucial Performance Insights

1. **Release-Mode Compilation (`maturin develop --release`)**:
   - Always compile with `maturin develop --release` for production deployment and benchmarks.
2. **Terminal Access Logging Gating (`RUSTAPI_LOG=0`)**:
   - Gating synchronous terminal stdout locking unleashes Tokio socket concurrency up to **5,089 req/sec**.
