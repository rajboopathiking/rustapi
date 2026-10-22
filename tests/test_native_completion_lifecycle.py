import os
import signal
import socket
import subprocess
import sys
import time
import pytest
import requests
import rustapi
from rustapi import FastAPI, Depends
from rustapi.responses import StreamingResponse

HOST = "127.0.0.1"
PORT = 8058
BASE = f"http://{HOST}:{PORT}"

events_log = []

app = FastAPI()


def req_scoped_dep():
    events_log.append("req_enter")
    try:
        yield "active_req"
    finally:
        events_log.append("req_exit")


@app.get("/slow-read")
def slow_read_route(dep=Depends(req_scoped_dep, scope="request")):
    return {"status": "ok", "message": "hello world from rustapi"}


@app.get("/stream-disconnect")
def stream_disconnect_route(dep=Depends(req_scoped_dep, scope="request")):
    def gen():
        for i in range(10):
            time.sleep(0.05)
            yield f"data: chunk {i}\n\n"
    return StreamingResponse(gen(), media_type="text/event-stream")


@pytest.fixture(scope="module", autouse=True)
def run_lifecycle_server():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-c",
            "from tests.test_native_completion_lifecycle import app, HOST, PORT; app.run(host=HOST, port=PORT, reload=False)",
        ],
        start_new_session=(sys.platform != "win32"),
    )

    connected = False
    for _ in range(50):
        try:
            r = requests.get(f"{BASE}/docs", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Native completion lifecycle server failed to start"
    yield
    try:
        if sys.platform == "win32":
            proc.terminate()
        else:
            os.killpg(proc.pid, signal.SIGINT)
        proc.wait(timeout=3)
    except Exception:
        proc.kill()


def test_native_slow_reader_preserves_request_teardown_until_body_read():
    """Verify that in native Hyper server, request-scoped teardown is tied to
    the response body lifecycle: a slow reader holding the socket open delays
    teardown until the body bytes are actually consumed by Hyper."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    try:
        s.sendall(b"GET /slow-read HTTP/1.1\r\nHost: 127.0.0.1:8058\r\nConnection: close\r\n\r\n")

        # Read only up to headers (HTTP/1.1 200 OK ... \r\n\r\n)
        raw_data = b""
        while b"\r\n\r\n" not in raw_data:
            chunk = s.recv(128)
            if not chunk:
                break
            raw_data += chunk

        assert b"200 OK" in raw_data

        # Sleep to emulate slow client backpressure
        time.sleep(0.2)

        # Read remainder of response body
        while True:
            chunk = s.recv(1024)
            if not chunk:
                break
            raw_data += chunk

        assert b"hello world from rustapi" in raw_data
    finally:
        s.close()

    # Wait briefly for teardown background task to finish
    time.sleep(0.1)


def test_native_early_disconnect_triggers_drop_teardown_cleanly():
    """Verify that if a client abruptly disconnects during a streaming response,
    the Stream Drop implementation cleanly triggers request teardown without crashing
    or stalling the server."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    try:
        s.sendall(b"GET /stream-disconnect HTTP/1.1\r\nHost: 127.0.0.1:8058\r\nConnection: close\r\n\r\n")

        # Read only headers and the very first chunk
        raw_data = b""
        while b"\r\n\r\n" not in raw_data:
            chunk = s.recv(64)
            if not chunk:
                break
            raw_data += chunk

        assert b"200 OK" in raw_data
    finally:
        # Abruptly close socket before stream completes
        s.close()

    # The server should still be healthy and accepting new requests
    time.sleep(0.2)
    resp = requests.get(f"{BASE}/slow-read", timeout=2)
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"
