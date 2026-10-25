import os
import signal
import subprocess
import sys
import time

import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8063
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO items (id, name) VALUES (1, 'first'), (2, 'second')")

app.add_sql_route(
    "/raw/json",
    "SELECT json_object('ok', 1)",
    raw=True,
)

app.add_sql_route(
    "/raw/item/{item_id}",
    "SELECT json_object('item', json_object('id', i.id, 'name', i.name)) FROM items i WHERE i.id = :item_id",
    raw=True,
    param_types={"item_id": "posint"},
    missing_status=404,
    missing_body='{"error":"item not found"}',
    invalid_status=400,
    invalid_body='{"error":"invalid item id"}',
)


@pytest.fixture(scope="module", autouse=True)
def run_server():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-c",
            f"from tests.test_native_sql_binding import app, HOST, PORT; app.run(host='{HOST}', port={PORT}, reload=False)",
        ],
        start_new_session=(sys.platform != "win32"),
    )
    try:
        for _ in range(60):
            try:
                if requests.get(f"{BASE}/raw/json", timeout=1).status_code == 200:
                    break
            except Exception:
                time.sleep(0.1)
        else:
            pytest.fail("Native SQL binding test server failed to start")
        yield
    finally:
        if sys.platform != "win32":
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        else:
            proc.terminate()
        proc.wait(timeout=5)


def test_raw_single_value_is_returned_verbatim():
    r = requests.get(f"{BASE}/raw/json", timeout=5)
    assert r.status_code == 200
    assert r.text == '{"ok":1}'


def test_raw_bound_parameter():
    r = requests.get(f"{BASE}/raw/item/1", timeout=5)
    assert r.status_code == 200
    assert r.json() == {"item": {"id": 1, "name": "first"}}


def test_missing_row_uses_configured_response():
    r = requests.get(f"{BASE}/raw/item/999", timeout=5)
    assert r.status_code == 404
    assert r.text == '{"error":"item not found"}'


@pytest.mark.parametrize("value", ["0", "-1", "abc", "1.5", "1%20OR%201=1"])
def test_invalid_typed_parameter_uses_configured_response(value):
    r = requests.get(f"{BASE}/raw/item/{value}", timeout=5)
    assert r.status_code == 400
    assert r.text == '{"error":"invalid item id"}'
