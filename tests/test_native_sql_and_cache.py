import time
import subprocess
import signal
import sys
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8062
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()

# Initialize SQLite database
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE IF NOT EXISTS sql_cache_products (id INTEGER PRIMARY KEY, name TEXT, price REAL)")
db.execute("DELETE FROM sql_cache_products")
db.execute("INSERT INTO sql_cache_products (name, price) VALUES ('MacBook Pro', 1999.00), ('Magic Mouse', 79.00), ('AirPods Max', 549.00)")

# 1. Native SQL route registered via add_sql_route
app.add_sql_route("/native/products", "SELECT * FROM sql_cache_products ORDER BY id ASC")

# 2. Native SQL route with path parameter using :param inside query
@app.get("/native/products/by-colon/{id}", sql="SELECT * FROM sql_cache_products WHERE id = :id")
def get_product_colon():
    pass

# 3. Native SQL route with path parameter using {param} inside query
@app.get("/native/products/by-brace/{id}", sql="SELECT * FROM sql_cache_products WHERE id = {id}")
def get_product_brace():
    pass

# 4. Native SQL route with in-memory TTL caching
@app.get("/cached/products", sql="SELECT * FROM sql_cache_products ORDER BY id ASC", cache_ttl=5)
def get_cached_products():
    pass

# 5. Native static route with TTL caching
app.add_native_route("/cached/static", '{"cached": true}', cache_ttl=5)

# 6. Native SQL mutation (INSERT)
@app.post("/native/products/insert", sql="INSERT INTO sql_cache_products (name, price) VALUES ('Studio Display', 1599.00)")
def insert_product():
    pass


@pytest.fixture(scope="module", autouse=True)
def run_sql_cache_server():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-c",
            f"from tests.test_native_sql_and_cache import app, HOST, PORT; app.run(host='{HOST}', port={PORT}, reload=False)",
        ],
        start_new_session=(sys.platform != "win32"),
    )

    connected = False
    for _ in range(40):
        try:
            r = requests.get(f"{BASE}/native/products", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)

    if not connected:
        proc.kill()
        pytest.fail("Native SQL & Cache test server failed to start within 4 seconds")

    yield

    try:
        if sys.platform != "win32":
            import os
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        else:
            proc.terminate()
        proc.wait(timeout=3)
    except Exception:
        proc.kill()


def test_native_sql_route_select():
    """Verify that a native SQL route executes directly in Rust and returns clean JSON."""
    res = requests.get(f"{BASE}/native/products", timeout=2)
    assert res.status_code == 200
    assert res.headers["content-type"] == "application/json"
    data = res.json()
    assert len(data) >= 3
    assert data[0]["name"] == "MacBook Pro"
    assert data[0]["price"] == 1999.0
    assert data[1]["name"] == "Magic Mouse"


def test_native_sql_route_with_path_parameters():
    """Verify that path parameters (:id and {id}) are extracted and safely bound to SQL."""
    # Test with colon placeholder :id
    r1 = requests.get(f"{BASE}/native/products/by-colon/2", timeout=2)
    assert r1.status_code == 200
    d1 = r1.json()
    assert len(d1) == 1
    assert d1[0]["id"] == 2
    assert d1[0]["name"] == "Magic Mouse"

    # Test with brace placeholder {id}
    r2 = requests.get(f"{BASE}/native/products/by-brace/3", timeout=2)
    assert r2.status_code == 200
    d2 = r2.json()
    assert len(d2) == 1
    assert d2[0]["id"] == 3
    assert d2[0]["name"] == "AirPods Max"


def test_native_sql_route_mutation():
    """Verify that native SQL mutations (INSERT) return rows_affected."""
    res = requests.post(f"{BASE}/native/products/insert", timeout=2)
    assert res.status_code == 200
    data = res.json()
    assert data["rows_affected"] == 1


def test_in_memory_cache_hit_and_miss():
    """Verify that cached routes return x-cache: MISS on first access and x-cache: HIT on subsequent access."""
    # 1. First request -> MISS
    t0 = time.time()
    r1 = requests.get(f"{BASE}/cached/products", timeout=2)
    d1 = time.time() - t0
    assert r1.status_code == 200
    assert r1.headers.get("x-cache") == "MISS"
    data1 = r1.json()

    # 2. Immediate second request -> HIT (ultra-fast from Rust RAM)
    t1 = time.time()
    r2 = requests.get(f"{BASE}/cached/products", timeout=2)
    d2 = time.time() - t1
    assert r2.status_code == 200
    assert r2.headers.get("x-cache") == "HIT"
    data2 = r2.json()
    assert data1 == data2

    # 3. Native static route caching
    s1 = requests.get(f"{BASE}/cached/static", timeout=2)
    assert s1.headers.get("x-cache") == "MISS"
    assert s1.json()["cached"] is True

    s2 = requests.get(f"{BASE}/cached/static", timeout=2)
    assert s2.headers.get("x-cache") == "HIT"
    assert s2.json()["cached"] is True
