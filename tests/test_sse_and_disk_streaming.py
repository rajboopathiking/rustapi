import os
import sys
import time
import signal
import subprocess
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8066
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()


# 1. Native SSE Route directly connected to Rust pub/sub broadcast channel
@app.get("/live/telemetry", sse="telemetry")
def live_telemetry():
    pass


# 2. HTTP Route to publish to the SSE topic
@app.post("/publish/telemetry")
def publish_telemetry():
    count = app.broadcast("telemetry", {"device": "sensor-1", "temperature": 75.4})
    return {"delivered_to": count}


# 3. HTTP File Upload route testing native zero-copy save_to
@app.post("/upload/save")
async def upload_and_save(file: rustapi.UploadFile):
    dest_path = f"/tmp/test_uploaded_{file.filename}"
    written = await file.save_to(dest_path)
    return {"filename": file.filename, "size": file.size, "written": written, "path": dest_path}


@pytest.fixture(scope="module", autouse=True)
def run_sse_server():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-c",
            f"from tests.test_sse_and_disk_streaming import app, HOST, PORT; app.run(host='{HOST}', port={PORT}, reload=False)",
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
        pytest.fail("SSE and disk streaming test server failed to start within 4 seconds")

    yield

    try:
        if sys.platform != "win32":
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        else:
            proc.terminate()
        proc.wait(timeout=3)
    except Exception:
        proc.kill()


def test_native_sse_streaming_and_broadcast():
    """Verify that a client connecting to an SSE route receives real-time broadcast events."""
    # Connect with streaming enabled
    session = requests.Session()
    stream_resp = session.get(f"{BASE}/live/telemetry", stream=True, timeout=5)
    assert stream_resp.status_code == 200
    assert "text/event-stream" in stream_resp.headers.get("content-type", "")

    lines_iter = stream_resp.iter_lines(decode_unicode=True)

    # First event received is the initial comment
    initial_line = next(lines_iter)
    assert ": connected" in initial_line

    time.sleep(0.1)
    # Trigger broadcast from another endpoint
    pub_res = requests.post(f"{BASE}/publish/telemetry", timeout=2)
    assert pub_res.status_code == 200
    assert pub_res.json()["delivered_to"] >= 1

    # Read next SSE data line
    data_line = next(lines_iter)
    while not data_line:  # skip empty lines between SSE events
        data_line = next(lines_iter)

    assert "data: " in data_line
    assert "sensor-1" in data_line
    assert "75.4" in data_line
    stream_resp.close()


def test_zero_copy_upload_file_save_to(tmp_path):
    """Verify that UploadFile.save_to writes directly to disk and reports file size."""
    raw_content = b"ZERO_COPY_RUST_DISK_STREAMING_DATA_PAYLOAD_" * 500
    files = {"file": ("dataset.bin", raw_content, "application/octet-stream")}

    res = requests.post(f"{BASE}/upload/save", files=files, timeout=5)
    assert res.status_code == 200
    body = res.json()
    assert body["filename"] == "dataset.bin"
    assert body["size"] == len(raw_content)
    assert body["written"] == len(raw_content)

    dest_file = body["path"]
    assert os.path.exists(dest_file)
    with open(dest_file, "rb") as f:
        saved_bytes = f.read()
    assert saved_bytes == raw_content
    os.remove(dest_file)


def test_upload_file_unit_methods(tmp_path):
    """Verify unit methods on UploadFile (size, save_to sync/async)."""
    dest = tmp_path / "unit_out.bin"
    upload = rustapi.UploadFile("test.txt", "text/plain", b"RustAPI Native Disk Stream")
    assert upload.size == len(b"RustAPI Native Disk Stream")

    written = upload.save_to(str(dest))
    assert written == len(b"RustAPI Native Disk Stream")
    assert dest.read_bytes() == b"RustAPI Native Disk Stream"


def test_load_onnx_api_behavior():
    """Verify that load_onnx provides clear actionable error when onnxruntime is absent."""
    try:
        import onnxruntime
        # If onnxruntime is installed, app.load_onnx should accept a model
        assert hasattr(app, "load_onnx")
    except ImportError:
        with pytest.raises(ImportError) as exc_info:
            app.load_onnx("non_existent.onnx")
        assert "pyrustapi[ai]" in str(exc_info.value)
