import subprocess
import sys
import time
import os
import re
import json
import requests

FASTAPI_PORT = 8093
RUSTAPI_PORT = 8094
FASTAPI_BASE = f"http://127.0.0.1:{FASTAPI_PORT}"
RUSTAPI_BASE = f"http://127.0.0.1:{RUSTAPI_PORT}"

# --- 1. FastAPI Application Code ---
FASTAPI_APP_CODE = f"""
import uvicorn
import sqlite3
import threading
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse

app = FastAPI()
db_lock = threading.Lock()

# Setup in-memory SQLite with sample rows
conn = sqlite3.connect("file:bench_fa_mem?mode=memory&cache=shared", uri=True, check_same_thread=False)
conn.execute("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
conn.execute("DELETE FROM users")
conn.executemany("INSERT INTO users (name, email) VALUES (?, ?)", [("User " + str(i), f"user{{i}}@example.com") for i in range(100)])
conn.commit()

# Route 1: Standard JSON
@app.get("/json")
def get_json():
    return {{"status": "ok", "framework": "fastapi", "engine": "uvicorn"}}

# Route 2: In-Memory Cache (Simulated in Python Dict)
_cache = {{"data": {{"status": "cached", "cached_at": 1000}}}}
@app.get("/cached")
def get_cached():
    return _cache["data"]

# Route 3: SQL Database Query
@app.get("/sql")
def get_sql():
    with db_lock:
        cursor = conn.cursor()
        cursor.execute("SELECT id, name, email FROM users")
        rows = cursor.fetchall()
        return [{{"id": r[0], "name": r[1], "email": r[2]}} for r in rows]

# Route 4: Rate Limiting Rejection (Python level)
_rate_count = 0
_rate_lock = threading.Lock()
@app.get("/rate-limit")
def get_rate_limit():
    global _rate_count
    with _rate_lock:
        _rate_count += 1
        if _rate_count > 5:
            raise HTTPException(status_code=429, detail="Too Many Requests")
    return {{"status": "allowed"}}

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port={FASTAPI_PORT}, log_level="critical")
"""

# --- 2. RustAPI Application Code ---
RUSTAPI_APP_CODE = f"""
import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi

app = rustapi.Engine()

# Connect native SQLx database pool
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
values_str = ", ".join([f"('User {{i}}', 'user{{i}}@example.com')" for i in range(100)])
db.execute(f"INSERT INTO users (name, email) VALUES {{values_str}}")

# Route 1: Standard JSON (Rust Hyper Ingress + orjson serialization)
@app.get("/json")
def get_json():
    return {{"status": "ok", "framework": "pyrustapi", "engine": "tokio_hyper"}}

# Route 2: Native Rust In-Memory TTL Cache (Direct RAM, zero Python)
@app.get("/cached", cache_ttl=60)
def get_cached():
    return {{"status": "cached", "cached_at": 1000}}

# Route 3: Direct SQL Fast-Path (SQLx -> Hyper bytes, zero Python)
app.add_sql_route("/sql", "SELECT id, name, email FROM users")

# Route 4: Native Token-Bucket Rate Limiter (< 50us 429 rejection in Rust)
@app.get("/rate-limit", rate_limit="5/second")
def get_rate_limit():
    return {{"status": "allowed"}}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port={RUSTAPI_PORT}, reload=False)
"""

def parse_oha_output(output_str):
    rps_match = re.search(r"Requests/sec:\s+([\d\.]+)", output_str)
    lat_match = re.search(r"Average:\s+([\d\.]+)\s*(ms|s|us)", output_str)
    p99_match = re.search(r"99.00%\s+([\d\.]+)\s*(ms|s|us)", output_str)

    rps = float(rps_match.group(1)) if rps_match else 0.0

    def to_ms(val_str, unit):
        v = float(val_str)
        if unit == "s":
            return v * 1000.0
        elif unit == "us":
            return v / 1000.0
        return v

    lat = to_ms(lat_match.group(1), lat_match.group(2)) if lat_match else 0.0
    p99 = to_ms(p99_match.group(1), p99_match.group(2)) if p99_match else 0.0
    return rps, lat, p99

def run_oha_benchmark(url, concurrency=50, duration="3s"):
    cmd = [
        "/Users/boopathiraj/.cargo/bin/oha",
        "-c", str(concurrency),
        "-z", duration,
        "--no-tui",
        url
    ]
    res = subprocess.run(cmd, capture_output=True, text=True)
    return parse_oha_output(res.stdout)

def wait_for_server(url, max_attempts=40):
    for _ in range(max_attempts):
        try:
            r = requests.get(url, timeout=0.5)
            if r.status_code in (200, 404, 429):
                return True
        except Exception:
            time.sleep(0.1)
    return False

def main():
    print("=" * 75)
    print(" 🚀 PYRUSTAPI VS FASTAPI (UVICORN): MAXIMUM NATIVE RUST BENCHMARK")
    print("=" * 75)

    os.makedirs("scratch", exist_ok=True)
    fa_file = "scratch/bench_fastapi_app.py"
    ra_file = "scratch/bench_rustapi_app.py"

    with open(fa_file, "w") as f:
        f.write(FASTAPI_APP_CODE)
    with open(ra_file, "w") as f:
        f.write(RUSTAPI_APP_CODE)

    # 1. Start FastAPI Process
    print("\n[1/4] Starting FastAPI (Uvicorn) on port", FASTAPI_PORT, "...")
    proc_fa = subprocess.Popen([sys.executable, fa_file])
    if not wait_for_server(f"{FASTAPI_BASE}/json"):
        proc_fa.kill()
        sys.exit("FastAPI failed to start")
    print("      FastAPI is ready.")

    # 2. Start PyRustAPI Process
    print("\n[2/4] Starting PyRustAPI (Hyper/Tokio) on port", RUSTAPI_PORT, "...")
    proc_ra = subprocess.Popen([sys.executable, ra_file])
    if not wait_for_server(f"{RUSTAPI_BASE}/json"):
        proc_ra.kill()
        proc_fa.kill()
        sys.exit("PyRustAPI failed to start")
    print("      PyRustAPI is ready.")

    # Warmup
    print("\n[3/4] Warming up both servers...")
    requests.get(f"{FASTAPI_BASE}/json")
    requests.get(f"{RUSTAPI_BASE}/json")
    requests.get(f"{FASTAPI_BASE}/sql")
    requests.get(f"{RUSTAPI_BASE}/sql")
    time.sleep(1.0)

    # 3. Execute Benchmarks
    print("\n[4/4] Executing High-Concurrency Benchmarks with `oha` (50 workers)...")
    results = []

    tests = [
        ("1. Standard JSON Endpoint", "/json"),
        ("2. In-Memory Cache Hit", "/cached"),
        ("3. Database Query (SQLx vs SQLite)", "/sql"),
        ("4. DDoS / Rate Limiter Rejection", "/rate-limit"),
    ]

    for title, endpoint in tests:
        print(f"\n  ---> Benchmarking: {title} ({endpoint})")
        
        # Test FastAPI
        fa_rps, fa_lat, fa_p99 = run_oha_benchmark(f"{FASTAPI_BASE}{endpoint}", concurrency=50, duration="3s")
        print(f"       FastAPI (Uvicorn):  {fa_rps:9,.1f} req/s  | Avg: {fa_lat:5.2f} ms | p99: {fa_p99:5.2f} ms")

        # Test PyRustAPI
        ra_rps, ra_lat, ra_p99 = run_oha_benchmark(f"{RUSTAPI_BASE}{endpoint}", concurrency=50, duration="3s")
        print(f"       PyRustAPI (Rust):   {ra_rps:9,.1f} req/s  | Avg: {ra_lat:5.2f} ms | p99: {ra_p99:5.2f} ms")

        gain_pct = ((ra_rps - fa_rps) / fa_rps * 100.0) if fa_rps > 0 else 0.0
        results.append({
            "test": title,
            "endpoint": endpoint,
            "fastapi_rps": fa_rps,
            "fastapi_lat": fa_lat,
            "fastapi_p99": fa_p99,
            "rustapi_rps": ra_rps,
            "rustapi_lat": ra_lat,
            "rustapi_p99": ra_p99,
            "gain_pct": gain_pct
        })

    # Kill servers
    proc_fa.kill()
    proc_ra.kill()

    # 4. Benchmark In-Process SIMD Vector Search vs Pure Python
    print("\n  ---> Benchmarking: 5. SIMD Vector Similarity Search (10,000 vectors x 128 dim)")
    import rustapi
    import random
    dim = 128
    query = [random.random() for _ in range(dim)]
    candidates = [[random.random() for _ in range(dim)] for _ in range(10000)]

    # Pure Python Cosine Similarity
    import math
    def py_cosine(a, b):
        dot = sum(x * y for x, y in zip(a, b))
        na = math.sqrt(sum(x * x for x in a))
        nb = math.sqrt(sum(x * x for x in b))
        return dot / (na * nb)

    t0 = time.time()
    for c in candidates:
        py_cosine(query, c)
    py_vec_time = (time.time() - t0) * 1000.0 # ms

    # Rust SIMD Vector Search
    t0 = time.time()
    rustapi.vector_search(query, candidates, top_k=5)
    rust_vec_time = (time.time() - t0) * 1000.0 # ms

    vec_gain_pct = ((py_vec_time - rust_vec_time) / rust_vec_time * 100.0) if rust_vec_time > 0 else 0.0
    print(f"       Pure Python Loop:   {py_vec_time:5.2f} ms")
    print(f"       Rust SIMD Engine:   {rust_vec_time:5.2f} ms (Gain: +{vec_gain_pct:,.0f}%)")

    # 5. Output Beautiful Final Report Table
    print("\n" + "=" * 85)
    print(" 📊 FINAL BENCHMARK RESULTS MATRIX")
    print("=" * 85)
    print(f" {'Workload':<36} | {'FastAPI (req/s)':<16} | {'PyRustAPI (req/s)':<18} | {'Speedup':<12}")
    print("-" * 85)
    for r in results:
        speedup_str = f"+{r['gain_pct']:,.1f}%"
        print(f" {r['test']:<36} | {r['fastapi_rps']:12,.1f}     | {r['rustapi_rps']:14,.1f}     | {speedup_str:<12}")
    
    vec_speedup_str = f"+{vec_gain_pct:,.0f}%"
    print(f" {'5. SIMD Vector Search (10k items)':<36} | {py_vec_time:9.2f} ms     | {rust_vec_time:11.2f} ms     | {vec_speedup_str:<12}")
    print("=" * 85)

if __name__ == "__main__":
    main()
