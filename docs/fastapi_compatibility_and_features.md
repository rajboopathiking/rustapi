# 🚀 FastAPI Compatibility & Feature Guide in RustAPI (`pyrustapi`)

RustAPI (`pyrustapi`) provides **1:1 FastAPI compatibility**, enabling developers to migrate existing FastAPI applications or write clean FastAPI-style code backed by a high-performance **Rust Tokio / Hyper core**.

---

## 📌 Summary of Features & Compatibility Layer

| Feature | Import Path | Description |
| :--- | :--- | :--- |
| **`FastAPI` Alias** | `from rustapi import FastAPI` | Class alias for `Engine` for 100% drop-in FastAPI compatibility. |
| **`Request` Alias** | `from rustapi import Request` | Class alias for `PyRequest`. |
| **Status Codes** | `from rustapi import status` | Starlette/FastAPI status code constants (`status.HTTP_200_OK`, `status.HTTP_404_NOT_FOUND`, etc.). |
| **OpenAPI & Docs UI** | `from rustapi.openapi import get_swagger_ui_html, get_redoc_html` | Interactive Swagger UI (`/docs`) and ReDoc (`/redoc`) HTML generators. |
| **Response Classes** | `from rustapi.responses import FileResponse, HTMLResponse, JSONResponse, PlainTextResponse, RedirectResponse, StreamingResponse` | Full range of FastAPI response classes. |
| **CORS Middleware** | `from rustapi.middleware.cors import CORSMiddleware` | Middleware class & `app.add_middleware()` for configuring cross-origin requests. |
| **Server-Sent Events** | `from rustapi import EventSourceResponse, ServerSentEvent, format_sse_event` | Real-time SSE streaming for AI/LLM tokens and Model Context Protocol (MCP). |
| **Data Encoders** | `from rustapi import jsonable_encoder` | Convert Pydantic models, Dataclasses, Datetime, UUID, and dicts to JSON-serializable primitives. |
| **Parameter Markers** | `from rustapi import Body, Query, Path, Header, Cookie, Form, File, Security` | Location markers for dependency injection and parameter validation. |
| **Security Modules** | `from rustapi.security import OAuth2PasswordBearer, HTTPBearer, APIKeyHeader` | Pre-built security authentication dependency helpers. |
| **Frontend Serving** | `app.frontend("/", directory="dist")` / `router.frontend(...)` | Serve built React, Vue, Svelte, or Vite single-page apps with client-side routing fallback. |
| **WebSocket Exceptions**| `from rustapi import WebSocketDisconnect, WebSocketException` | Exception classes for handling WebSocket disconnection events. |

---

## 1. FastAPI Aliases & Status Codes

Drop-in FastAPI imports work directly out of the box:

```python
from rustapi import FastAPI, Request, status, HTTPException

app = FastAPI()

@app.get("/health", status_code=status.HTTP_200_OK)
def health_check(request: Request):
    if not request:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Bad request")
    return {"status": "healthy"}
```

---

## 2. Interactive API Documentation (`Swagger UI` & `ReDoc`)

Serve custom interactive documentation endpoints using `get_swagger_ui_html` and `get_redoc_html`:

```python
from rustapi import FastAPI
from rustapi.openapi import get_swagger_ui_html, get_redoc_html

app = FastAPI()

@app.get("/docs", include_in_schema=False)
def custom_swagger_ui():
    return get_swagger_ui_html(
        openapi_url="/openapi.json",
        title="My Custom API Docs",
    )

@app.get("/redoc", include_in_schema=False)
def custom_redoc_ui():
    return get_redoc_html(
        openapi_url="/openapi.json",
        title="My ReDoc API",
    )
```

---

## 3. Server-Sent Events (SSE) for AI & Streaming

Stream AI/LLM tokens or real-time event updates over HTTP using `EventSourceResponse` and `ServerSentEvent`:

```python
from collections.abc import AsyncIterable
from rustapi import FastAPI, EventSourceResponse, ServerSentEvent

app = FastAPI()

@app.get("/ai-stream", response_class=EventSourceResponse)
async def stream_ai_tokens():
    tokens = ["Hello", " world", " from", " RustAPI", " SSE!"]
    for token in tokens:
        yield ServerSentEvent(data={"token": token}, event="message")
```

Or format raw SSE wire bytes using `format_sse_event`:

```python
from rustapi import format_sse_event

raw_bytes = format_sse_event(
    data_str='{"status": "processing"}',
    event="ping",
    id="evt_001",
    retry=3000,
)
```

---

## 4. Frontend Single-Page App (SPA) Serving

Serve compiled React, Vue, Svelte, or Vite frontend build directories directly from `rustapi`:

```python
from rustapi import FastAPI

app = FastAPI()

# Serves 'dist/index.html' for '/' and static assets from 'dist/'
app.frontend("/", directory="dist")

# Or attach frontend serving to sub-routers
from rustapi import APIRouter
admin_router = APIRouter(prefix="/admin")
admin_router.frontend("/", directory="admin-dist")
app.include_router(admin_router)
```

---

## 5. Security & Authentication Dependencies

Use pre-packaged security helpers for OAuth2, HTTP Bearer, and API Keys:

```python
from rustapi import FastAPI, Depends, HTTPException, status
from rustapi.security import OAuth2PasswordBearer, HTTPBearer, APIKeyHeader

app = FastAPI()

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")
bearer_scheme = HTTPBearer()
api_key_scheme = APIKeyHeader(name="X-API-Key")

@app.get("/users/me")
def read_current_user(token: str = Depends(oauth2_scheme)):
    if not token:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")
    return {"user": "alice", "token": token}
```

---

## 6. Parameter Markers & `jsonable_encoder`

Use standard FastAPI parameter placement markers and data converters:

```python
from dataclasses import dataclass
from rustapi import FastAPI, Query, Path, Body, jsonable_encoder

app = FastAPI()

@dataclass
class User:
    id: int
    username: str

@app.get("/items/{item_id}")
def get_item(
    item_id: int = Path(..., ge=1),
    q: str = Query(None, max_length=50),
):
    user = User(id=1, username="bob")
    # Convert dataclass/Pydantic model to JSON dict
    user_dict = jsonable_encoder(user)
    return {"item_id": item_id, "query": q, "user": user_dict}


---

## 7. Python Ecosystem Compatibility Matrix

`rustapi` is built to seamlessly integrate with standard Python database, machine learning, security, and utility libraries:

| Library Domain | Package | Verified Feature & Usage | Status |
| :--- | :--- | :--- | :--- |
| **Data Validation** | `pydantic` v2 | `BaseModel`, `Field`, `validator`, `EmailStr` in route bodies | ✅ Verified |
| **HTTP Clients** | `httpx`, `requests` | `httpx.AsyncClient` & `requests` testing, multi-part uploads & streams | ✅ Verified |
| **Databases & ORMs** | `sqlite3`, `sqlalchemy` | Session context managers & query execution inside `Depends(get_db)` | ✅ Verified |
| **Image Processing** | `PIL` / `Pillow` | Image decoding & matrix transformations on `UploadFile` streams | ✅ Verified |
| **JWT & Security** | `pyjwt`, `passlib`, `argon2` | `jwt.encode`, `jwt.decode`, password hashing inside auth endpoints | ✅ Verified |
| **Numeric & ML** | `numpy` | `np.array` operations on request payloads & image byte arrays | ✅ Verified |

```python
import io
import jwt
import numpy as np
from PIL import Image
from pydantic import BaseModel, Field
from rustapi import FastAPI, Request, Depends, HTTPException
from rustapi.uploads import UploadFile

app = FastAPI()

class AnalysisModel(BaseModel):
    project_name: str = Field(..., min_length=2)

@app.post("/analyze")
async def analyze(req: Request):
    if "photo" not in req.files:
        raise HTTPException(status_code=400, detail="No photo uploaded")
    
    file_obj: UploadFile = req.files["photo"][0]
    img_bytes = await file_obj.read()  # Both await file.read() and file.read() work
    
    # Process image with Pillow & NumPy
    image = Image.open(io.BytesIO(img_bytes)).convert("L")
    array = np.array(image)
    brightness = float(np.mean(array))
    
    # Encode JWT response
    token = jwt.encode({"brightness": brightness}, "secret", algorithm="HS256")
    return {"status": "success", "token": token}
```

---

## 9. Full Python Ecosystem Interoperability & Release v0.8.6

`pyrustapi` features zero-limitation compatibility with the Python library ecosystem:

| Library Category | Supported Packages | Integration Behavior |
| :--- | :--- | :--- |
| **Databases & Async ORMs** | `SQLAlchemy`, `asyncpg`, `aiosqlite`, `tortoise-orm`, `peewee` | Context managers & `yield` generator sessions in `Depends(get_db)` |
| **Data Validation** | `pydantic` v2, `msgspec`, `attrs`, `dataclasses` | Full request body parsing, field validation & 422 coercion |
| **Security & Auth** | `pyjwt`, `python-jose`, `passlib`, `argon2-cffi`, `cryptography` | Bearer token validation, password hashing & custom claims |
| **Machine Learning & AI** | `torch`, `tensorflow`, `scikit-learn`, `onnxruntime`, `xgboost` | Model inference & tensor computation on upload byte streams |
| **HTTP & Async Testing** | `httpx`, `requests`, `aiohttp`, `starlette.testclient` | ASGI 3.0 (`app(scope, receive, send)`) & `ASGITransport` support |
| **Async Task Queues** | `celery`, `redis`, `rq`, `dramatiq` | Background task enqueueing inside async route handlers |
| **Domain Specific & Rare** | `biopython`, `rdkit`, `geopandas`, `cv2`, `librosa`, `numba` | Native C-extensions, CUDA acceleration & JIT compiled functions |

### Support for Rare & Native C-Extension Libraries

`pyrustapi` is **not** an interpreter transpile layer; it executes directly inside the native CPython runtime via PyO3 C-bindings. Therefore, **100% of Python libraries work with zero limitations**:

- **C / C++ / Cython Extensions** (`ctypes`, `cffi`, `pybind11`, `.so` / `.dylib`): Linked directly to standard CPython ABI memory.
- **Scientific & ML Accelerators** (`torch`, `cuda`, `scikit-learn`, `numba`, `scipy`): GPU tensor execution & JIT function evaluation operate seamlessly.
- **Domain-Specific Packages** (`biopython`, `rdkit`, `astropy`, `cv2`, `librosa`): Native binary dependencies load and execute without modification.

### Key Improvements in Release v0.8.6:

1. **Native OpenAPI `securitySchemes` & Swagger UI "Authorize 🔓"**:
   Automatically detects `HTTPBearer`, `OAuth2PasswordBearer(tokenUrl=...)`, `APIKeyHeader`, `APIKeyQuery`, and `HTTPBasic` dependencies to output `components.securitySchemes` in `/openapi.json`, activating interactive auth testing in Swagger UI (`/docs`).
2. **HTTPException Status Code Preservation**:
   Directly propagates custom status codes (`401 Unauthorized`, `403 Forbidden`, `404 Not Found`, `422 Unprocessable Entity`) without falling back to 500 Internal Server Errors.
3. **Recursive Dependency & Request Injection**:
   Resolves nested `Depends(sub_dep)` chains and injects `request: Request` automatically across all dependency tiers.
4. **Dual Async / Sync Payload Helpers**:
   Provides `AwaitableDict` for `req.json()` and `AwaitableBytes` for `UploadFile.read()`, allowing both `await req.json()` and `req.json()` inside sync/async ML route handlers.

