# 📊 RustAPI (pyrustapi) vs. FastAPI: Swagger UI & DX Deviations Guide

This document provides an exhaustive breakdown of the architectural deviations, Developer Experience (DX) differences, and Swagger UI / OpenAPI generation nuances between **FastAPI** and **RustAPI (pyrustapi)**.

---

## Executive Summary Matrix

| Feature / DX Domain | FastAPI Standard | RustAPI (`pyrustapi`) | Architectural Rationale & Fix |
| :--- | :--- | :--- | :--- |
| **Swagger UI OpenAPI Generation** | Auto-inferred from Python function signature annotations (`List[UploadFile]`, `Form(...)`) | Default schema generated; multi-file array & complex form schemas customizable via native route override | Shifting reflection away from every request maximizes performance. `app.add_native_route("/openapi.json", ...)` grants full control over Swagger UI schema rendering. |
| **Request Handler Parameters** | Typed signature parameters (`files: List[UploadFile]`, `user_id: int`) | Unified `req` (`PyRequest`) object or typed signature parameter injection | RustAPI passes a pre-constructed PyO3 request object to Python handlers to minimize CPython reflection & GIL contention overhead. |
| **Multipart File Uploads** | `files: List[UploadFile] = File(...)` parameter injection | `req.files["field_name"]` dictionary lookup | Uploaded multipart files are parsed natively in Rust Tokio worker threads into `req.files` containing `UploadFile` objects. |
| **File Stream Reading** | Asynchronous (`await file.read()`) | Synchronous (`doc.read()`) | Tokio background threads execute file I/O in Rust; buffer extraction in Python is synchronous bytes access. |
| **Binary File Handling (Images/PIL)** | In-memory bytes or temp file streams | Synchronous `doc.read()` returning raw `bytes` | **Do NOT use `.decode("utf-8")` on binary bytes**. Use `io.BytesIO(doc.read())` or `"wb"` mode to prevent `UnidentifiedImageError` and `UnicodeDecodeError`. |
| **Database Query Engine** | External ORMs (SQLAlchemy, SQLModel) | Embedded `sqlx` engine (`app.connect_db()`) | Native Rust zero-copy JSON streaming directly to HTTP client sockets (`db.query_json()`). |

---

## 1. Swagger UI & OpenAPI Auto-Generation Deviations

### The Deviation
In FastAPI, function parameter annotations (such as `files: List[UploadFile] = File(...)`) are inspected via CPython reflection at startup to build an OpenAPI 3.0 specification. This automatically renders multi-file file pickers in Swagger UI (`/docs`).

In RustAPI, route handlers typically use the high-performance `def upload(req):` pattern. Because `req` is a single unified PyO3 object without explicit Python parameter annotations:
* RustAPI's native OpenAPI generator infers path parameters (e.g. `/items/{item_id}`) and query parameters automatically.
* When Pydantic models are used (`def post_item(data: Item):`), RustAPI automatically extracts `.model_json_schema()` and populates `#/components/schemas/` in `/openapi.json`.
* For multipart upload routes using `req`, RustAPI generates a default single-file schema (`type: "string", format: "binary"`).

### How to Achieve 100% Swagger UI Parity for Multi-File Uploads

When you need an array file picker (`type: "array", items: { type: "string", format: "binary" }`) or custom form fields in Swagger UI, register a native `/openapi.json` route override:

```python
import json
import rustapi

app = rustapi.Engine()

# 1. Route Handler using RustAPI req pattern
@app.post("/photos/upload")
def upload(req):
    description = req.form.get("description", "")
    files = [doc for file_list in req.files.values() for doc in file_list]
    return {
        "status": "success",
        "description": description,
        "total_files": len(files)
    }

# 2. Native Route Override for OpenAPI / Swagger UI
openapi_schema = json.dumps({
    "openapi": "3.0.0",
    "info": {
        "title": "RustAPI Skincare & Upload API",
        "version": "1.0.0"
    },
    "paths": {
        "/photos/upload": {
            "post": {
                "summary": "Upload Multiple Photos",
                "requestBody": {
                    "required": True,
                    "content": {
                        "multipart/form-data": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "description": {"type": "string"},
                                    "document": {
                                        "type": "array",
                                        "items": {
                                            "type": "string",
                                            "format": "binary"
                                        },
                                        "description": "Select multiple image files"
                                    }
                                }
                            }
                        }
                    }
                },
                "responses": {
                    "200": {"description": "Successful Upload"}
                }
            }
        }
    }
})

# Register native JSON route override for /openapi.json
app.add_native_route(
    "/openapi.json",
    openapi_schema,
    method="GET",
    status_code=200,
    content_type="application/json"
)

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

Visiting `http://127.0.0.1:8000/docs` will render the customized Swagger UI interface with multi-file drag-and-drop selection buttons.

---

### Swagger UI Security & Interactive Authorize (v1.8.7)

RustAPI automatically detects all 8 security dependencies from `rustapi.security` (`HTTPBearer`, `OAuth2PasswordBearer`, `OAuth2AuthorizationCodeBearer`, `APIKeyHeader`, `APIKeyQuery`, `APIKeyCookie`, `HTTPBasic`, `HTTPDigest`, `OpenIdConnect`) across direct and nested dependencies (`Depends(...)`).

When security dependencies are declared on route handlers:
1. **OpenAPI SecuritySchemes**: RustAPI auto-populates `components.securitySchemes` in `/openapi.json`.
2. **Path Operation Security**: Each protected route receives `"security": [{ "<scheme_name>": [] }]`.
3. **FastAPI-Parity Authorize Button (🔒)**: Swagger UI served at `/docs` leverages the standard `BaseLayout` to perfectly match FastAPI's UI. It displays the interactive **Authorize** lock button, automatically injects `initOAuth` configuration, and supports `/docs/oauth2-redirect` callbacks for seamless OAuth2 flows!

```python
from rustapi import FastAPI, Depends
from rustapi.security import OAuth2PasswordBearer, HTTPBearer, APIKeyHeader

app = FastAPI(title="Secure API", version="0.7.86")

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="/token")
bearer_scheme = HTTPBearer()
api_key_scheme = APIKeyHeader(name="X-API-Key")

@app.get("/users/me")
def read_user(token: str = Depends(oauth2_scheme)):
    return {"token": token}
```

---

## 2. Route Handler Signature (req vs Parameter Injection)

### FastAPI DX
FastAPI uses CPython signature inspection on every request to resolve parameters and dependencies:

```python
# FastAPI
@app.post("/photos/upload")
async def upload(
    description: str = Form(...),
    files: List[UploadFile] = File(...)
):
    return {"count": len(files)}
```

### RustAPI DX
RustAPI gives developers two options:

#### Option A: Unified `req` Parameter (Recommended for Speed)
Passing `req` avoids CPython parameter reflection overhead and GIL contention:

```python
# RustAPI (Unified PyRequest)
@app.post("/photos/upload")
def upload(req):
    description = req.form.get("description")
    files = req.files.get("document", [])
    return {"count": len(files), "description": description}
```

#### Option B: Typed Signature Parameters & Pydantic Validation
RustAPI also supports Pydantic models, typed path parameters, and `Depends(...)`:

```python
# RustAPI (Typed Pydantic Signature)
from pydantic import BaseModel
import rustapi

class UploadMetadata(BaseModel):
    title: str
    tags: list[str]

@app.post("/metadata")
def create_metadata(data: UploadMetadata):
    return {"status": "created", "title": data.title}
```

---

## 3. Multipart File Upload & Stream I/O Deviations

### Synchronous `doc.read()` vs `await file.read()`

* **FastAPI**: Requires asynchronous file reading using `await file.read()`.
* **RustAPI**: File extraction is handled asynchronously in Tokio worker threads before calling Python. In Python, calling `doc.read()` is a **synchronous method call returning bytes**.

```python
# RustAPI File Upload Handler
@app.post("/upload")
def upload_files(req):
    # req.files is a dictionary mapping field names -> List[UploadFile]
    for field_name, file_list in req.files.items():
        for doc in file_list:
            content_bytes = doc.read()  # Synchronous read returning bytes (NO await)
            print(f"File: {doc.filename}, Content Type: {doc.content_type}, Size: {len(content_bytes)}")
    return {"status": "uploaded"}
```

> **Warning**: Do NOT use `await doc.read()` in RustAPI; doing so will raise a `TypeError` because `doc.read()` returns `bytes` directly.

---

## 4. Binary Image Handling (Avoiding `UnidentifiedImageError`)

When working with uploaded binary files (such as JPEG/PNG skin image uploads):

1. **DO NOT** attempt to decode binary bytes with `.decode("utf-8")`. UTF-8 decoding corrupts multi-byte image data.
2. **DO** pass raw byte buffers to memory processors such as Pillow (`PIL.Image`) using `io.BytesIO`.

### Production Code Example: Processing Binary Image Uploads

```python
import io
from PIL import Image
import rustapi

app = rustapi.Engine()

@app.post("/analyze-skin")
def analyze_skin(req):
    images = req.files.get("image", [])
    if not images:
        return {"error": "No image uploaded"}, 400

    doc = images[0]
    content_bytes = doc.read()  # Sync bytes extraction

    # Open image directly from in-memory bytes without saving to disk
    try:
        image = Image.open(io.BytesIO(content_bytes))
        width, height = image.size
        img_format = image.format
        return {
            "filename": doc.filename,
            "format": img_format,
            "dimensions": {"width": width, "height": height},
            "status": "analyzed"
        }
    except Exception as e:
        return {"error": f"Invalid image format: {str(e)}"}, 400
```

---

## 5. Summary Matrix & Developer Best Practices

1. **Use `req` for High Throughput**: Access `req.form`, `req.query_params`, `req.files`, `req.headers`, `req.json()`, and `req.path_params` directly on `req` to maximize execution speed.
2. **Synchronous File Reads**: Always call `doc.read()` without `await`.
3. **In-Memory Binary I/O**: Wrap `doc.read()` in `io.BytesIO(...)` when handing binary images to Pillow, OpenCV, or PyTorch.
4. **Custom Swagger UI Schemas**: Register `app.add_native_route("/openapi.json", custom_spec, ...)` whenever specialized Swagger UI inputs (like multi-file array pickers) are required for your API endpoints.


```python

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


```