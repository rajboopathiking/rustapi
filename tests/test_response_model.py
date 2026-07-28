import time
import threading
import pytest
import requests
from pydantic import BaseModel
import rustapi

HOST = "127.0.0.1"
PORT = 8011
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()


class UserOut(BaseModel):
    id: int
    username: str
    email: str


@app.get("/user", response_model=UserOut)
def get_user():
    return {
        "id": 101,
        "username": "boopathi",
        "password_hash": "super_secret_hash",
        "email": "user@example.com",
    }


@app.get("/users", response_model=list[UserOut])
def get_users():
    return [
        {"id": 1, "username": "alice", "password_hash": "p1", "email": "a@ex.com"},
        {"id": 2, "username": "bob", "password_hash": "p2", "email": "b@ex.com"},
    ]


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
            r = requests.get(f"{BASE}/user", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Server failed to start for response_model tests"


def test_single_response_model_filters_private_fields():
    r = requests.get(f"{BASE}/user")
    assert r.status_code == 200
    data = r.json()
    assert data == {"id": 101, "username": "boopathi", "email": "user@example.com"}
    assert "password_hash" not in data


def test_list_response_model_filters_items():
    r = requests.get(f"{BASE}/users")
    assert r.status_code == 200
    data = r.json()
    assert len(data) == 2
    assert data[0] == {"id": 1, "username": "alice", "email": "a@ex.com"}
    assert data[1] == {"id": 2, "username": "bob", "email": "b@ex.com"}
    assert "password_hash" not in data[0]
