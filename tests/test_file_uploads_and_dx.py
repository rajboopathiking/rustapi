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


@app.post("/upload/form_and_file")
def upload_form_and_file(
    label: str = rustapi.Form(...), file: rustapi.UploadFile = rustapi.File(...)
):
    content = file.read()
    if not isinstance(content, bytes):
        content = bytes(content)
    return {
        "label": label,
        "label_is_str": isinstance(label, str),
        "filename": file.filename,
        "file_is_upload_file": isinstance(file, rustapi.UploadFile),
        "size": len(content),
    }


def test_fastapi_form_and_file_parameter_binding():
    files = {"file": ("resume.pdf", b"PDF bytes content", "application/pdf")}
    data = {"label": "My Resume"}
    r = requests.post(f"{BASE}/upload/form_and_file", data=data, files=files)
    assert r.status_code == 200
    res = r.json()
    assert res["label"] == "My Resume"
    assert res["label_is_str"] is True
    assert res["filename"] == "resume.pdf"
    assert res["file_is_upload_file"] is True
    assert res["size"] == len(b"PDF bytes content")


recent_uploads_store = []


@app.post("/upload/recent_demo")
def upload_recent_demo(req):
    files = req.files.get("file", [])
    if not files:
        return {"error": "no file"}
    doc = files[0]
    content = doc.read()
    item = {"filename": doc.filename, "size": len(content)}
    recent_uploads_store.append(item)
    return {"status": "ok", "uploaded": item}


@app.get("/uploads/recent_demo")
def get_recent_uploads_demo():
    return {"recent": list(reversed(recent_uploads_store))}


def test_recent_uploads_flow():
    files = {"file": ("demo.txt", b"Recent upload test content", "text/plain")}
    r1 = requests.post(f"{BASE}/upload/recent_demo", files=files)
    assert r1.status_code == 200
    assert r1.json()["uploaded"]["filename"] == "demo.txt"

    r2 = requests.get(f"{BASE}/uploads/recent_demo")
    assert r2.status_code == 200
    res2 = r2.json()
    assert len(res2["recent"]) >= 1
    assert res2["recent"][0]["filename"] == "demo.txt"


def test_openapi_upload_and_multi_file_schemas():
    from typing import List
    test_app = rustapi.FastAPI(title="Swagger UI Upload Spec Test")

    @test_app.post("/upload/single")
    def single_doc(document: rustapi.UploadFile = rustapi.File(...), desc: str = rustapi.Form("default")):
        return {}

    @test_app.post("/upload/multi")
    def multi_docs(documents: List[rustapi.UploadFile] = rustapi.File(...)):
        return {}

    spec = test_app.openapi()
    single_schema = spec["paths"]["/upload/single"]["post"]["requestBody"]["content"]["multipart/form-data"]["schema"]
    assert single_schema["properties"]["document"] == {"type": "string", "format": "binary"}
    assert single_schema["properties"]["desc"] == {"type": "string"}
    assert "document" in single_schema["required"]

    multi_schema = spec["paths"]["/upload/multi"]["post"]["requestBody"]["content"]["multipart/form-data"]["schema"]
    assert multi_schema["properties"]["documents"] == {
        "type": "array",
        "items": {"type": "string", "format": "binary"},
    }
    assert "documents" in multi_schema["required"]



