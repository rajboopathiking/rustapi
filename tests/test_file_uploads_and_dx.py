import io
import json
import threading
import time
import pytest
import requests
from PIL import Image
import rustapi

HOST = "127.0.0.1"
PORT = 8015
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()


@app.post("/upload/single")
def upload_single_file(req):
    files = req.files.get("file", [])
    if not files:
        return {"status": "error", "message": "No file uploaded"}
    doc = files[0]
    content = doc.read()  # Sync read returns bytes
    assert isinstance(content, bytes)
    return {
        "filename": doc.filename,
        "content_type": doc.content_type,
        "size": len(content),
        "content_preview": content.decode("utf-8", errors="ignore")[:50],
    }


@app.post("/upload/binary_image")
def upload_binary_image(req):
    files = req.files.get("image", [])
    if not files:
        return {"status": "error", "message": "No image uploaded"}
    doc = files[0]
    content_bytes = doc.read()

    # Open image directly from in-memory bytes without crashing
    image = Image.open(io.BytesIO(content_bytes))
    return {
        "filename": doc.filename,
        "format": image.format,
        "size": image.size,
        "mode": image.mode,
    }


@app.post("/upload/multiple")
def upload_multiple_files(req):
    total_files = 0
    file_summary = []
    for field_name, file_list in req.files.items():
        for doc in file_list:
            content_bytes = doc.read()
            total_files += 1
            file_summary.append({
                "field": field_name,
                "filename": doc.filename,
                "bytes_len": len(content_bytes),
            })
    return {"total_files": total_files, "files": file_summary}


# Custom OpenAPI specification registered via add_native_route
custom_openapi_spec = json.dumps({
    "openapi": "3.0.0",
    "info": {"title": "RustAPI Custom Upload API", "version": "1.0.0"},
    "paths": {
        "/upload/multiple": {
            "post": {
                "summary": "Upload Multiple Files",
                "requestBody": {
                    "content": {
                        "multipart/form-data": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "document": {
                                        "type": "array",
                                        "items": {"type": "string", "format": "binary"},
                                    }
                                }
                            }
                        }
                    }
                },
                "responses": {"200": {"description": "Upload Success"}},
            }
        }
    },
})

app.add_native_route(
    "/custom_openapi.json",
    custom_openapi_spec,
    method="GET",
    status_code=200,
    content_type="application/json",
)


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
            r = requests.get(f"{BASE}/custom_openapi.json", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Server failed to start for test_file_uploads_and_dx"


def test_single_file_upload():
    file_data = b"Hello, RustAPI multi-file upload test!"
    files = {"file": ("test.txt", file_data, "text/plain")}
    r = requests.post(f"{BASE}/upload/single", files=files)
    assert r.status_code == 200
    res = r.json()
    assert res["filename"] == "test.txt"
    assert res["size"] == len(file_data)
    assert "Hello, RustAPI" in res["content_preview"]


def test_binary_image_upload():
    # Create a 100x100 RGB image in memory
    img = Image.new("RGB", (100, 100), color="red")
    img_byte_arr = io.BytesIO()
    img.save(img_byte_arr, format="PNG")
    img_bytes = img_byte_arr.getvalue()

    files = {"image": ("skin_photo.png", img_bytes, "image/png")}
    r = requests.post(f"{BASE}/upload/binary_image", files=files)
    assert r.status_code == 200
    res = r.json()
    assert res["filename"] == "skin_photo.png"
    assert res["format"] == "PNG"
    assert res["size"] == [100, 100]


def test_multiple_file_upload():
    files = [
        ("document", ("doc1.txt", b"First file content", "text/plain")),
        ("document", ("doc2.txt", b"Second file content", "text/plain")),
    ]
    r = requests.post(f"{BASE}/upload/multiple", files=files)
    assert r.status_code == 200
    res = r.json()
    assert res["total_files"] == 2
    assert len(res["files"]) == 2


def test_custom_openapi_route():
    r = requests.get(f"{BASE}/custom_openapi.json")
    assert r.status_code == 200
    data = r.json()
    assert data["info"]["title"] == "RustAPI Custom Upload API"
    assert "/upload/multiple" in data["paths"]
