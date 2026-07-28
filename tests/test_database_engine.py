import time
import threading
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8013
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()
db = app.connect_db("sqlite::memory:")

# Initialize database schema & data
db.execute("CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)")
db.execute("INSERT INTO products (name, price) VALUES ('Rust Book', 29.99), ('FastAPI Guide', 19.99)")


@app.get("/products")
def get_products():
    # Native zero-copy Rust query returning JSON response directly
    return db.query_json("SELECT * FROM products ORDER BY id ASC")


@pytest.fixture(scope="module", autouse=True)
def run_server():
    server_thread = threading.Thread(
        target=lambda: app.run(host=HOST, port=PORT),
        daemon=True,
    )
    server_thread.start()

    connected = False
    for _ in range(20):
        try:
            r = requests.get(f"{BASE}/products", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Server failed to start for Phase B Database Engine test"


def test_native_database_execute_and_query():
    r = requests.get(f"{BASE}/products")
    assert r.status_code == 200
    data = r.json()
    assert len(data) == 2
    assert data[0]["name"] == "Rust Book"
    assert data[0]["price"] == 29.99
    assert data[1]["name"] == "FastAPI Guide"
    assert data[1]["price"] == 19.99


def test_direct_db_methods():
    count = db.execute("INSERT INTO products (name, price) VALUES ('Python Tricks', 39.99)")
    assert count == 1
    resp = db.query_json("SELECT COUNT(*) as total FROM products")
    assert "total" in resp.content
