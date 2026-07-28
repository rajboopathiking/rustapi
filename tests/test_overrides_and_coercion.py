import time
import threading
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8012
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()


def get_db():
    return "production_database"


def get_test_db():
    return "mock_test_database"


@app.get("/db")
def read_db(db=rustapi.Depends(get_db)):
    return {"db": db}


@app.get("/items/{item_id}")
def read_item(item_id: int):
    return {"item_id": item_id}


@app.get("/search")
def search(q: int):
    return {"query_val": q}


@pytest.fixture(scope="module", autouse=True)
def run_server():
    # Set dependency override before server start
    app.dependency_overrides[get_db] = get_test_db

    server_thread = threading.Thread(
        target=lambda: app.run(host=HOST, port=PORT),
        daemon=True,
    )
    server_thread.start()

    connected = False
    for _ in range(20):
        try:
            r = requests.get(f"{BASE}/db", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Server failed to start for test_overrides_and_coercion"


def test_dependency_overrides():
    r = requests.get(f"{BASE}/db")
    assert r.status_code == 200
    assert r.json() == {"db": "mock_test_database"}


def test_strict_type_coercion_valid_int():
    r = requests.get(f"{BASE}/items/123")
    assert r.status_code == 200
    assert r.json() == {"item_id": 123}


def test_strict_type_coercion_invalid_int_returns_422():
    r = requests.get(f"{BASE}/items/not_an_integer")
    assert r.status_code == 422
    assert "must be an integer" in r.json().get("detail", "")


def test_missing_required_query_param_returns_422():
    r = requests.get(f"{BASE}/search")
    assert r.status_code == 422
    assert "Missing required parameter" in r.json().get("detail", "")
