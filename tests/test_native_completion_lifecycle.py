import os
import signal
import socket
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
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


@app.get("/stream-scoped")
def stream_scoped_route(dep=Depends(req_scoped_dep, scope="request")):
    def gen():
        for i in range(5):
            events_log.append(f"yield_{i}")
            yield f"data: chunk {i}\n\n"
            time.sleep(0.08)
    return StreamingResponse(gen(), media_type="text/event-stream")


@app.get("/slow-stream-backpressure")
def slow_stream_backpressure_route():
    def gen():
        for i in range(25):
            time.sleep(0.04)
            yield f"chunk_{i}\n"
    return StreamingResponse(gen(), media_type="text/plain")


@app.get("/fast-json")
def fast_json_route():
    return {"fast": "ok", "time": time.time()}


@app.get("/events")
def get_events():
    return list(events_log)


@app.post("/events/clear")
def clear_events():
    events_log.clear()
    return {"cleared": True}


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


def test_streaming_preserves_request_teardown_until_stream_completion():
    """Verify that in native Hyper server, request-scoped teardown is owned by
    StreamingBodyStream and NOT triggered prematurely by the producer while chunks
    remain buffered or in-flight."""
    requests.post(f"{BASE}/events/clear")

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    try:
        s.sendall(b"GET /stream-scoped HTTP/1.1\r\nHost: 127.0.0.1:8058\r\nConnection: close\r\n\r\n")

        # Read only headers and the very first chunk
        raw_data = b""
        while b"data: chunk 0\n\n" not in raw_data:
            chunk = s.recv(64)
            if not chunk:
                break
            raw_data += chunk

        assert b"200 OK" in raw_data
        assert b"data: chunk 0\n\n" in raw_data

        # 1. Query server events while stream is active: chunk 0 has been delivered,
        # but subsequent chunks are still being streamed over the channel.
        # Teardown MUST NOT execute while the stream is in flight!
        r1 = requests.get(f"{BASE}/events")
        ev1 = r1.json()
        assert "req_enter" in ev1
        assert "req_exit" not in ev1, f"CRITICAL: req_exit fired prematurely while stream was still active: {ev1}"

        # 2. Simulate slow client reading chunks one by one
        while True:
            chunk = s.recv(128)
            if not chunk:
                break
            raw_data += chunk

        # Verify all 5 chunks were received by client
        for i in range(5):
            assert f"data: chunk {i}\n\n".encode() in raw_data
    finally:
        s.close()

    # 3. After stream EOF is reached by Hyper and socket closes, teardown MUST execute exactly once
    time.sleep(0.15)
    r2 = requests.get(f"{BASE}/events")
    ev2 = r2.json()
    assert "req_exit" in ev2, f"Expected req_exit after completion, got {ev2}"
    assert ev2.count("req_enter") == 1
    assert ev2.count("req_exit") == 1
    # Verify order: enter -> yields -> exit at end
    assert ev2[0] == "req_enter"
    assert ev2[-1] == "req_exit"


def test_streaming_early_disconnect_triggers_drop_teardown_cleanly():
    """Verify that if a client abruptly disconnects during a streaming response,
    the Stream Drop implementation cleanly triggers request teardown exactly once."""
    requests.post(f"{BASE}/events/clear")

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    try:
        s.sendall(b"GET /stream-scoped HTTP/1.1\r\nHost: 127.0.0.1:8058\r\nConnection: close\r\n\r\n")

        # Read only headers and the first chunk
        raw_data = b""
        while b"data: chunk 0\n\n" not in raw_data:
            chunk = s.recv(64)
            if not chunk:
                break
            raw_data += chunk

        assert b"200 OK" in raw_data
        time.sleep(0.05)
        ev_start = requests.get(f"{BASE}/events").json()
        assert "req_enter" in ev_start
        assert "req_exit" not in ev_start
    finally:
        # Abruptly close socket while stream producer still has remaining chunks
        s.close()

    # Drop MUST trigger teardown exactly once without crashing or hanging the server
    time.sleep(0.25)
    ev_end = requests.get(f"{BASE}/events").json()
    assert ev_end.count("req_enter") == 1
    assert ev_end.count("req_exit") == 1
    assert ev_end[0] == "req_enter"
    # Stream producer aborted early upon broken channel, so yield_4 was never reached
    assert "yield_4" not in ev_end

    # Server remains completely healthy and accepts new requests
    resp = requests.get(f"{BASE}/fast-json", timeout=2)
    assert resp.status_code == 200
    assert resp.json()["fast"] == "ok"


def test_slow_client_backpressure_does_not_block_concurrent_requests():
    """Verify that a slow client blocking on a streaming response does not hold
    the Python GIL or freeze concurrent request handling."""
    slow_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    slow_sock.connect((HOST, PORT))
    try:
        slow_sock.sendall(b"GET /slow-stream-backpressure HTTP/1.1\r\nHost: 127.0.0.1:8058\r\nConnection: close\r\n\r\n")
        # Read only headers, leave body buffer unread to trigger channel backpressure
        header_data = b""
        while b"\r\n\r\n" not in header_data:
            header_data += slow_sock.recv(64)
        assert b"200 OK" in header_data

        # While the slow stream is blocked on channel capacity, fire concurrent requests from multiple threads
        def send_fast_request(idx):
            t0 = time.time()
            res = requests.get(f"{BASE}/fast-json", timeout=1)
            duration = time.time() - t0
            assert res.status_code == 200
            assert res.json()["fast"] == "ok"
            return duration

        with ThreadPoolExecutor(max_workers=5) as pool:
            futures = [pool.submit(send_fast_request, i) for i in range(15)]
            durations = [f.result() for f in futures]

        assert len(durations) == 15
        # None of the concurrent fast requests should be blocked by the GIL
        avg_duration = sum(durations) / len(durations)
        assert avg_duration < 0.1, f"Concurrent requests blocked by GIL (avg: {avg_duration:.3f}s)"
    finally:
        slow_sock.close()
