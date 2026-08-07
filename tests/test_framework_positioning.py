import io
import time
import threading
import pytest
import requests
from concurrent.futures import ThreadPoolExecutor
from pydantic import BaseModel
import rustapi
from rustapi import FastAPI, UploadFile, File, Form, Depends, HTTPException
from rustapi.security import HTTPBearer, HTTPAuthorizationCredentials

HOST = "127.0.0.1"
PORT = 8044
BASE = f"http://{HOST}:{PORT}"

app = FastAPI(title="Production High-Concurrency Framework Test")
bearer = HTTPBearer()

db = app.connect_db("sqlite::memory:")
db.execute("""
    CREATE TABLE IF NOT EXISTS benchmark_items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        category TEXT NOT NULL,
        price REAL NOT NULL
    )
""")
db.execute("INSERT INTO benchmark_items (name, category, price) VALUES ('Item A', 'Tech', 99.99)")
db.execute("INSERT INTO benchmark_items (name, category, price) VALUES ('Item B', 'Home', 49.50)")


class ItemSchema(BaseModel):
    name: str
    category: str
    price: float


class ItemOut(BaseModel):
    id: int
    name: str
    category: str
    price: float


def get_current_user(auth: HTTPAuthorizationCredentials = Depends(bearer)):
    if auth.credentials != "secret_token_123":
        raise HTTPException(status_code=401, detail="Invalid token")
    return {"user_id": 42, "role": "admin"}


@app.get("/health")
def health_check():
    return {"status": "healthy", "engine": "Tokio Hyper Rust Core"}


@app.post("/items", response_model=ItemOut, status_code=201)
def create_item(item: ItemSchema, user: dict = Depends(get_current_user)):
    sql = f"INSERT INTO benchmark_items (name, category, price) VALUES ('{item.name}', '{item.category}', {item.price})"
    db.execute(sql)
    return ItemOut(id=99, name=item.name, category=item.category, price=item.price)


@app.get("/items/stream/db")
def stream_items_zero_copy():
    # Direct Rust zero-copy JSON socket streaming from SQLite DB
    return db.query_json("SELECT * FROM benchmark_items")


@app.post("/upload/binary")
def upload_binary_data(file: UploadFile = File(...), description: str = Form("no desc")):
    contents = file.read()
    return {
        "filename": file.filename,
        "content_type": file.content_type,
        "bytes_len": len(contents),
        "description": description,
    }


@pytest.fixture(scope="module", autouse=True)
def run_positioning_server():
    server_thread = threading.Thread(
        target=lambda: app.run(host=HOST, port=PORT),
        daemon=True,
    )
    server_thread.start()

    connected = False
    for _ in range(25):
        try:
            r = requests.get(f"{BASE}/health", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Positioning server failed to launch"


def test_high_concurrency_requests():
    def make_request(idx):
        res = requests.get(f"{BASE}/health", timeout=2)
        assert res.status_code == 200
        assert res.json()["status"] == "healthy"
        return idx

    with ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(make_request, i) for i in range(50)]
        results = [f.result() for f in futures]

    assert len(results) == 50


def test_zero_copy_sqlite_json_streaming():
    r = requests.get(f"{BASE}/items/stream/db")
    assert r.status_code == 200
    data = r.json()
    assert isinstance(data, list)
    assert len(data) >= 2
    assert data[0]["name"] == "Item A"
    assert data[1]["name"] == "Item B"


def test_authenticated_post_and_pydantic_filtering():
    headers = {"Authorization": "Bearer secret_token_123"}
    payload = {"name": "HighPerf Widget", "category": "Engine", "price": 299.99}

    r = requests.post(f"{BASE}/items", json=payload, headers=headers)
    assert r.status_code in (200, 201)
    data = r.json()
    assert data["name"] == "HighPerf Widget"
    assert data["category"] == "Engine"
    assert data["price"] == 299.99
    assert data["id"] == 99


def test_unauthorized_post_returns_401():
    headers = {"Authorization": "Bearer invalid_token"}
    payload = {"name": "Test", "category": "Test", "price": 10.0}

    r = requests.post(f"{BASE}/items", json=payload, headers=headers)
    assert r.status_code == 401


def test_binary_file_upload_concurrency():
    def upload_file(idx):
        content = f"sample binary content block {idx}".encode("utf-8")
        files = {"file": (f"test_{idx}.bin", io.BytesIO(content), "application/octet-stream")}
        data = {"description": f"batch test {idx}"}

        r = requests.post(f"{BASE}/upload/binary", files=files, data=data, timeout=3)
        assert r.status_code == 200
        res = r.json()
        assert res["filename"] == f"test_{idx}.bin"
        assert res["bytes_len"] == len(content)
        assert res["description"] == f"batch test {idx}"
        return idx

    with ThreadPoolExecutor(max_workers=5) as executor:
        futures = [executor.submit(upload_file, i) for i in range(15)]
        results = [f.result() for f in futures]

    assert len(results) == 15
