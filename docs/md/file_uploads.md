# 📁 File Uploads & Multipart Handling in RustAPI

RustAPI provides high-performance, native file upload support backed by Rust Tokio worker threads and Hyper HTTP server. Multipart request bodies are parsed in native C/Rust speed directly into zero-copy in-memory streams, eliminating CPython GIL contention and disk I/O bottlenecks.

---

## 📑 Table of Contents

- [Overview & Performance](#overview--performance)
- [Method 1: Request Object (`req.files`)](#method-1-request-object-reqfiles)
- [Method 2: FastAPI-Style Parameter Binding (`File(...)` & `Form(...)`)](#method-2-fastapi-style-parameter-binding-file--form)
- [`UploadFile` API Reference](#uploadfile-api-reference)
- [Handling Text vs Binary Files (Images, PDFs, ML Tensors)](#handling-text-vs-binary-files-images-pdfs-ml-tensors)
- [Multi-File Array Uploads](#multi-file-array-uploads)
- [Recent Uploads Pattern (Database & Memory Storage)](#recent-uploads-pattern-database--memory-storage)
- [OpenAPI / Swagger UI Schema Customization](#openapi--swagger-ui-schema-customization)
- [Complete Executable Example](#complete-executable-example)

---

## Overview & Performance

When a client sends a `multipart/form-data` request, RustAPI's Tokio workers split and parse the incoming body payload in native Rust. Each uploaded file is encapsulated in an `UploadFile` object that supports both synchronous and asynchronous reading without blocking the event loop.

Key advantages:
- **Zero Disk Latency**: Small and medium-sized file streams are stored in-memory buffer (`io.BytesIO`) for maximum processing speed.
- **Dual Sync/Async API**: Handlers can call `doc.read()` synchronously or `await doc.read()` asynchronously using `AwaitableBytes`.
- **FastAPI Drop-in Compatibility**: Functions accepting `file: UploadFile = File(...)` work out-of-the-box.

---

## Method 1: Request Object (`req.files`)

The primary, high-speed way to access uploaded files in RustAPI is via the `req.files` dictionary on the pre-constructed `PyRequest` object. `req.files` maps field names to lists of `UploadFile` objects (`Dict[str, List[UploadFile]]`).

```python
import rustapi

app = rustapi.Engine()

@app.post("/upload")
def upload_file(req):
    # 1. Retrieve list of UploadFile objects for field "file"
    files = req.files.get("file", [])
    if not files:
        return rustapi.JSONResponse({"error": "No file uploaded"}, status_code=400)

    doc = files[0]

    # 2. Read bytes (sync or async)
    content_bytes = doc.read()

    # 3. Read form text fields
    description = req.form.get("description", "No description provided")

    return {
        "filename": doc.filename,
        "content_type": doc.content_type,
        "size_bytes": len(content_bytes),
        "description": description,
    }
```

---

## Method 2: FastAPI-Style Parameter Binding (`File(...)` & `Form(...)`)

If you prefer FastAPI-compatible type annotations, RustAPI inspects handler function signatures and automatically injects `UploadFile` and `Form` parameters:

```python
import rustapi
from rustapi import UploadFile, File, Form

app = rustapi.Engine()

@app.post("/upload/typed")
def upload_file_typed(
    file: UploadFile = File(...),
    description: str = Form("Default description"),
):
    content = file.read()
    return {
        "filename": file.filename,
        "content_type": file.content_type,
        "size_bytes": len(content),
        "description": description,
    }
```

---

## `UploadFile` API Reference

Each uploaded file instance provides standard attributes and methods matching the ASGI / FastAPI spec:

| Attribute / Method | Type | Description |
| :--- | :--- | :--- |
| `filename` | `str` | Original filename sent by client (e.g., `"report.pdf"`). |
| `content_type` | `str` | MIME type of file (e.g., `"application/pdf"`, `"image/png"`). |
| `headers` | `Dict[str, str]` | Header dictionary associated with the upload part. |
| `file` | `io.BytesIO` | Underlying Python binary stream buffer. |
| `read(size=-1)` | `AwaitableBytes` | Reads up to `size` bytes. Returns `bytes` (awaitable via `await file.read()`). |
| `seek(offset=0)` | `AwaitableInt` | Sets stream position offset. |
| `write(data)` | `AwaitableInt` | Writes raw `bytes` to stream buffer. |
| `close()` | `AwaitableNone` | Closes underlying stream buffer. |

---

## Handling Text vs Binary Files (Images, PDFs, ML Tensors)

### 1. Text Files (`.txt`, `.csv`, `.json`)
Decode the byte array using `.decode("utf-8")`:

```python
@app.post("/upload/text")
def upload_text(req):
    files = req.files.get("file", [])
    if not files:
        return {"error": "Missing file"}
    
    text = files[0].read().decode("utf-8", errors="replace")
    return {"preview": text[:200]}
```

### 2. Binary Files & Images (`.png`, `.jpg`, `.pdf`)
> [!IMPORTANT]
> **Never call `.decode("utf-8")` on binary files or images!** Doing so raises `UnicodeDecodeError` or corrupts binary streams. Pass the raw bytes directly to `io.BytesIO`.

#### Image Processing with Pillow (PIL)
```python
import io
from PIL import Image

@app.post("/upload/image")
def upload_image(req):
    files = req.files.get("image", [])
    if not files:
        return {"error": "No image provided"}
    
    raw_bytes = files[0].read()
    # Open image directly from in-memory BytesIO stream
    image = Image.open(io.BytesIO(raw_bytes))
    
    return {
        "filename": files[0].filename,
        "format": image.format,
        "dimensions": f"{image.width}x{image.height}",
        "mode": image.mode,
    }
```

---

## Multi-File Array Uploads

To handle multiple files uploaded under a single form field or across multiple fields, iterate over `req.files`:

```python
@app.post("/upload/multiple")
def upload_multiple(req):
    summary = []
    total_files = 0

    # Iterate over all field names in multipart form payload
    for field_name, file_list in req.files.items():
        for doc in file_list:
            data = doc.read()
            total_files += 1
            summary.append({
                "field": field_name,
                "filename": doc.filename,
                "size_bytes": len(data),
            })

    return {"total_uploaded": total_files, "files": summary}
```

---

## Recent Uploads Pattern (Database & Memory Storage)

Below is an architecture pattern for persisting uploaded file metadata in an embedded SQLite database and maintaining an in-memory queue for recent uploads endpoint:

```python
import rustapi

app = rustapi.Engine()

# SQLite embedded database engine
db = app.connect_db("sqlite::memory:")
db.execute("""
    CREATE TABLE IF NOT EXISTS recent_uploads (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        filename TEXT NOT NULL,
        content_type TEXT,
        size INTEGER,
        description TEXT,
        uploaded_at TEXT DEFAULT CURRENT_TIMESTAMP
    )
""")

recent_uploads_cache = []

@app.post("/upload")
def handle_upload(req):
    files = req.files.get("file", [])
    if not files:
        return rustapi.JSONResponse({"error": "No file uploaded"}, status_code=400)

    doc = files[0]
    content = doc.read()
    desc = req.form.get("description", "Uploaded file")

    # 1. Insert record into Rust-native SQLite pool
    db.execute(
        "INSERT INTO recent_uploads (filename, content_type, size, description) VALUES (?, ?, ?, ?)",
        [doc.filename, doc.content_type, len(content), desc],
    )

    # 2. Append to recent uploads in-memory cache
    record = {
        "filename": doc.filename,
        "content_type": doc.content_type,
        "size": len(content),
        "description": desc,
    }
    recent_uploads_cache.append(record)
    if len(recent_uploads_cache) > 10:
        recent_uploads_cache.pop(0)

    return {"status": "success", "file": record}

@app.get("/uploads/recent")
def get_recent_uploads():
    # Stream JSON directly from native SQLite query
    db_records = db.query_json("SELECT * FROM recent_uploads ORDER BY id DESC LIMIT 10")
    return {
        "recent_in_memory": recent_uploads_cache[::-1],
        "recent_from_db": db_records,
    }
```

---

## OpenAPI / Swagger UI Schema Customization

By default, single-file routes in Swagger UI render binary upload inputs. For complex multi-file array parameters in Swagger UI (`/docs`), customize OpenAPI specifications via `app.add_native_route`:

```python
import json

custom_schema = json.dumps({
    "openapi": "3.0.0",
    "info": {"title": "RustAPI Upload API", "version": "1.0.0"},
    "paths": {
        "/upload/documents": {
            "post": {
                "summary": "Upload Multiple Documents",
                "requestBody": {
                    "content": {
                        "multipart/form-data": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "documents": {
                                        "type": "array",
                                        "items": {"type": "string", "format": "binary"},
                                    }
                                }
                            }
                        }
                    }
                },
                "responses": {"200": {"description": "Upload successful"}},
            }
        }
    },
})

app.add_native_route("/custom_openapi.json", custom_schema, content_type="application/json")
```

---

## Complete Executable Example

You can find a complete, runnable application demonstrating file uploads and recent upload listings in [`examples/app.py`](../../examples/app.py).

Run the example app locally:
```bash
python examples/app.py
```

Test uploading a file with `curl`:
```bash
curl -X POST "http://127.0.0.1:8000/upload" \
  -F "file=@README.md" \
  -F "description=Project README file"
```

Fetch recent uploads:
```bash
curl "http://127.0.0.1:8000/uploads/recent"
```
