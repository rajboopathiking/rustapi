import time
import subprocess
import signal
import sys
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8064
BASE = f"http://{HOST}:{PORT}"
WS_BASE = f"ws://{HOST}:{PORT}"

app = rustapi.Engine()


@app.websocket("/ws/echo-json")
def ws_echo_json(ws: rustapi.WebSocket):
    # Test JSON echo
    msg = ws.receive_json()
    ws.send_json({"echo": msg, "server": "rustapi"})


@app.websocket("/ws/echo-bytes")
def ws_echo_bytes(ws: rustapi.WebSocket):
    # Test Binary bytes echo
    b = ws.receive_bytes()
    ws.send_bytes(b + b"_processed_in_rust")


@app.websocket("/ws/broadcast-sub")
def ws_sub(ws: rustapi.WebSocket):
    # Subscribe to a topic
    ws.subscribe("telemetry")
    # Send acknowledgment
    ws.send_text("subscribed_ok")
    # Keep socket open to receive broadcast stream
    try:
        while True:
            time.sleep(0.5)
    except Exception:
        pass


@app.post("/api/publish")
def publish_message():
    # Trigger native Rust broadcast to all subscribers
    recipients = app.broadcast("telemetry", {"temperature": 72.5, "status": "nominal"})
    return {"recipients": recipients}


@pytest.fixture(scope="module", autouse=True)
def run_ws_server():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-c",
            f"from tests.test_websocket_broadcaster import app, HOST, PORT; app.run(host='{HOST}', port={PORT}, reload=False)",
        ],
        start_new_session=(sys.platform != "win32"),
    )

    connected = False
    for _ in range(40):
        try:
            r = requests.get(f"{BASE}/docs", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)

    if not connected:
        proc.kill()
        pytest.fail("WebSocket test server failed to start within 4 seconds")

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


def test_websocket_json_send_receive():
    """Verify WebSocket JSON serialization and deserialization in Rust."""
    import socket
    import base64
    import os

    # Connect raw HTTP socket and perform websocket handshake
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    key = base64.b64encode(os.urandom(16)).decode("utf-8")
    req = (
        f"GET /ws/echo-json HTTP/1.1\r\n"
        f"Host: {HOST}:{PORT}\r\n"
        f"Upgrade: websocket\r\n"
        f"Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        f"Sec-WebSocket-Version: 13\r\n\r\n"
    )
    s.sendall(req.encode())

    # Read handshake response
    resp = s.recv(1024)
    assert b"101 Switching Protocols" in resp

    # Send client text frame with JSON: {"client": "test_agent"}
    # Masked client frame: opcode 1 (text), mask bit 1
    payload = b'{"client":"test_agent"}'
    mask = os.urandom(4)
    masked_payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    header = bytes([0x81, 0x80 | len(payload)]) + mask
    s.sendall(header + masked_payload)

    # Read server response frame (unmasked)
    data = s.recv(1024)
    # Server frame: opcode 1, length >= 2
    assert len(data) > 2
    payload_len = data[1] & 0x7F
    resp_payload = data[2:2 + payload_len]
    import json
    parsed = json.loads(resp_payload.decode("utf-8"))
    assert parsed["echo"]["client"] == "test_agent"
    assert parsed["server"] == "rustapi"
    s.close()


def test_native_broadcast_to_subscribers():
    """Verify that app.broadcast(...) delivers messages to subscribed WebSocket clients."""
    import socket
    import base64
    import os

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    key = base64.b64encode(os.urandom(16)).decode("utf-8")
    req = (
        f"GET /ws/broadcast-sub HTTP/1.1\r\n"
        f"Host: {HOST}:{PORT}\r\n"
        f"Upgrade: websocket\r\n"
        f"Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        f"Sec-WebSocket-Version: 13\r\n\r\n"
    )
    s.sendall(req.encode())
    resp = s.recv(1024)
    assert b"101 Switching Protocols" in resp

    # Read "subscribed_ok"
    ack = s.recv(1024)
    assert b"subscribed_ok" in ack

    time.sleep(0.1)
    # Publish via HTTP endpoint
    pub_res = requests.post(f"{BASE}/api/publish", timeout=2)
    assert pub_res.status_code == 200
    assert pub_res.json()["recipients"] >= 1

    # Client socket receives the broadcasted JSON directly from Rust Tokio task!
    broadcast_data = s.recv(1024)
    assert b"nominal" in broadcast_data
    assert b"72.5" in broadcast_data
    s.close()


def test_websocket_bytes_send_receive():
    """Verify WebSocket binary frame serialization and deserialization."""
    import socket
    import base64
    import os

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((HOST, PORT))
    key = base64.b64encode(os.urandom(16)).decode("utf-8")
    req = (
        f"GET /ws/echo-bytes HTTP/1.1\r\n"
        f"Host: {HOST}:{PORT}\r\n"
        f"Upgrade: websocket\r\n"
        f"Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        f"Sec-WebSocket-Version: 13\r\n\r\n"
    )
    s.sendall(req.encode())
    resp = s.recv(1024)
    assert b"101 Switching Protocols" in resp

    payload = b"raw_binary_input"
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    # Opcode 2 is binary
    header = bytes([0x82, 0x80 | len(payload)]) + mask
    s.sendall(header + masked)

    data = s.recv(1024)
    assert len(data) > 2
    length = data[1] & 0x7F
    resp_body = data[2:2 + length]
    assert resp_body == b"raw_binary_input_processed_in_rust"
    s.close()
