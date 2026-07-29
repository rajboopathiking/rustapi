import time
import threading
import pytest
import requests
import rustapi
import math

HOST = "127.0.0.1"
PORT = 8042
BASE = f"http://{HOST}:{PORT}"

SECRET = "three_tier_test_secret"

app = rustapi.Engine()

# Initialize Tier 2 SQLite DB
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE tier_items (id INTEGER PRIMARY KEY, name TEXT, price REAL)")
db.execute("INSERT INTO tier_items (name, price) VALUES ('Widget A', 19.99), ('Widget B', 29.99)")

# ==== TIER 1: Pure Python Handler ====
@app.get("/tier1/python-math")
def tier1_handler():
    total = sum([math.sqrt(i * 1.5) for i in range(50)])
    return {"tier": 1, "result": total}

# ==== TIER 2: Hybrid Rust Primitives ====
@app.get("/tier2/rust-db")
def tier2_db():
    return db.query_json("SELECT * FROM tier_items")

@app.get("/tier2/rust-template")
def tier2_template():
    html = rustapi.render_template("<h1>Product List</h1><p>User: {{ name }}</p>", {"name": "Boopathi"})
    return rustapi.HTMLResponse(html)

@app.post("/tier2/rust-jwt")
def tier2_jwt():
    token = rustapi.encode_jwt({"sub": "user_100", "role": "admin"}, secret=SECRET)
    claims = rustapi.decode_jwt(token, secret=SECRET)
    return claims

# ==== TIER 3: Rust-Native Business Logic (Fast-Path Route) ====
app.add_native_route("/tier3/native-json", '{"tier": 3, "status": "c_speed_ok"}', content_type="application/json")
app.add_native_route("/tier3/native-html", '<h1>Native Rust HTML</h1>', content_type="text/html")


@pytest.fixture(scope="module", autouse=True)
def run_3tier_server():
    server_thread = threading.Thread(
        target=lambda: app.run(host=HOST, port=PORT),
        daemon=True,
    )
    server_thread.start()

    connected = False
    for _ in range(20):
        try:
            r = requests.get(f"{BASE}/tier3/native-json", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "3-Tier server failed to start"


def test_tier1_pure_python():
    r = requests.get(f"{BASE}/tier1/python-math")
    assert r.status_code == 200
    data = r.json()
    assert data["tier"] == 1
    assert "result" in data
    assert isinstance(data["result"], float)


def test_tier2_hybrid_rust_db():
    r = requests.get(f"{BASE}/tier2/rust-db")
    assert r.status_code == 200
    data = r.json()
    assert isinstance(data, list)
    assert len(data) == 2
    assert data[0]["name"] == "Widget A"


def test_tier2_hybrid_rust_template():
    r = requests.get(f"{BASE}/tier2/rust-template")
    assert r.status_code == 200
    assert "text/html" in r.headers["content-type"]
    assert "<h1>Product List</h1>" in r.text
    assert "User: Boopathi" in r.text


def test_tier2_hybrid_rust_jwt():
    r = requests.post(f"{BASE}/tier2/rust-jwt")
    assert r.status_code == 200
    data = r.json()
    assert data["sub"] == "user_100"
    assert data["role"] == "admin"


def test_tier3_native_rust_routes():
    # Test Tier 3 JSON Fast-Path
    r_json = requests.get(f"{BASE}/tier3/native-json")
    assert r_json.status_code == 200
    data = r_json.json()
    assert data["tier"] == 3
    assert data["status"] == "c_speed_ok"

    # Test Tier 3 HTML Fast-Path
    r_html = requests.get(f"{BASE}/tier3/native-html")
    assert r_html.status_code == 200
    assert "<h1>Native Rust HTML</h1>" in r_html.text
