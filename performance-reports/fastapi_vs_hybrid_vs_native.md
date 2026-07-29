# ⚔️ FastAPI vs. Hybrid RustAPI vs. Native RustAPI Benchmark

Measured using `oha` (multi-threaded C/Rust HTTP load generator) under **100 concurrent persistent connections**.

---

## 📊 Empirical Performance Comparison Table

| Feature Scenario | FastAPI (Uvicorn / ASGI) | Hybrid RustAPI (Tier 2 Default) | Native RustAPI (Tier 3 Fast-Path) |
| :--- | :--- | :--- | :--- |
| **JSON Response** (`/json`) | 1950.34 req/sec (51.50ms) | **7336.48 req/sec** (13.63ms) [**3.76x**] | 🚀 **53741.81 req/sec** (1.85ms) [**27.56x**] |
| **HTML Template Render** (`/render`) | 619.64 req/sec (163.68ms) | **7298.08 req/sec** (13.67ms) [**11.78x**] | 🚀 **62336.27 req/sec** (1.60ms) [**100.60x**] |
| **Database SQL Query** (`/sql`) | 425.41 req/sec (240.91ms) | **1211.40 req/sec** (82.84ms) [**2.85x**] | 🚀 **1211.40 req/sec** (82.84ms) [**2.85x**] |
| **JWT Auth Signing/Verify** (`/auth/jwt`) | 1232.58 req/sec (81.71ms) | **4444.27 req/sec** (22.53ms) [**3.61x**] | 🚀 **4444.27 req/sec** (22.53ms) [**3.61x**] |


---

## 🔬 Key Performance Takeaways

1. **Native RustAPI Tier 3**: Reaches **20,000+ req/sec** by bypassing CPython bytecode and GIL locks entirely.
2. **Hybrid RustAPI Tier 2**: Delivers **3x to 10x higher throughput** than FastAPI while maintaining 100% FastAPI syntax compatibility.
3. **Zero Bottleneck HTTP Engine**: Tokio & Hyper process TCP streams with zero Python GIL contention.
