# 📚 RustAPI vs FastAPI: Comprehensive Migration & Reference Guide

This guide provides a comprehensive documentation and code comparison between **FastAPI** and **RustAPI** for all completed phases (Phases 1, 2, and 5, along with Ground Truth and Advanced I/O features).

---

## §0. Ground Truth: Core Engine & Basic Routing

### FastAPI
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def root():
    return {"message": "Welcome to FastAPI!"}

@app.get("/sync")
def sync_route():
    return {"type": "sync"}

@app.get("/async")
async def async_route():
    return {"type": "async"}

```

### RustAPI

```python
import rustapi
import asyncio

app = rustapi.Engine()

@app.get("/")
def root():
    return {"message": "Welcome to RustAPI!"}

@app.get("/sync")
def sync_route():
    return {"type": "sync"}

@app.get("/async")
async def async_route():
    await asyncio.sleep(0.1)
    return {"type": "async"}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)

```

---

## §1. Phase 1: HTTP Metadata, Error Handling & Validation

### FastAPI

```python
from fastapi import FastAPI, Request, Response, HTTPException
from pydantic import BaseModel

app = FastAPI()

class Item(BaseModel):
    name: str
    price: float

@app.get("/inspect")
def inspect_req(req: Request):
    return {"user-agent": req.headers.get("user-agent"), "cookie": req.cookies.get("session")}

@app.get("/custom-resp")
def custom_resp(response: Response):
    response.status_code = 201
    response.headers["X-Custom"] = "Value"
    return {"status": "created"}

@app.post("/items")
def create_item(item: Item):
    if item.price < 0:
        raise HTTPException(status_code=400, detail="Invalid price")
    return item

```

### RustAPI

```python
import rustapi
from pydantic import BaseModel

app = rustapi.Engine()

class Item(BaseModel):
    name: str
    price: float

@app.get("/inspect")
def inspect_req(req):
    return {"user-agent": req.headers.get("user-agent"), "cookie": req.cookies.get("session")}

@app.get("/custom-resp")
def custom_resp():
    # Return explicit rustapi.Response object
    return rustapi.Response({"status": "created"}, status_code=201, headers={"X-Custom": "Value"})

@app.post("/items")
def create_item(item: Item):
    return item # Automatic Pydantic model validation & 422 error serialization

```

---

## §2. Phase 2: Dependency Injection & Generators

### FastAPI

```python
from fastapi import FastAPI, Depends

app = FastAPI()

def get_db():
    db = "active_db_connection"
    yield db
    # Teardown logic here (e.g., db.close())

@app.get("/users")
def get_users(db = Depends(get_db)):
    return {"db": db}

```

### RustAPI

```python
import rustapi

app = rustapi.Engine()

def get_db():
    yield "active_db_connection"
    # Automatic C-managed generator teardown called after response transmission

@app.get("/users")
def get_users(db = rustapi.Depends(get_db)):
    return {"db": db}

```

---

## §3. Phase 5: Production Ergonomics (APIRouter & Lifespan Hooks)

### FastAPI

```python
from fastapi import FastAPI, APIRouter, FastAPI

app = FastAPI(title="App")

router = APIRouter()

@router.get("/ping")
def ping():
    return {"status": "pong"}

app.include_router(router, prefix="/api/v1")

@app.on_event("startup")
def startup_event():
    print("App starting up...")

@app.on_event("shutdown")
def shutdown_event():
    print("App shutting down...")

```

### RustAPI

```python
import rustapi

app = rustapi.Engine()

# Mock or native APIRouter interface compatibility
class APIRouter:
    def __init__(self):
        self.routes = []
    def get(self, path):
        def decorator(func):
            self.routes.append(("GET", path, func))
            return func
        return decorator

router = APIRouter()

@router.get("/ping")
def ping():
    return {"status": "pong"}

app.include_router(router, prefix="/api/v1")

@app.on_event("startup")
def startup_event():
    print("App starting up...")

@app.on_event("shutdown")
async def shutdown_event():
    print("App shutting down...")

```

---

## §4. Advanced I/O: File Uploads & WebSockets

### FastAPI

```python
from fastapi import FastAPI, UploadFile, File, Form, WebSocket

app = FastAPI()

@app.post("/upload")
def upload(document: UploadFile = File(...), description: str = Form(...)):
    return {"filename": document.filename, "description": description}

@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    while True:
        data = await websocket.receive_text()
        await websocket.send_text(f"echo: {data}")

```

### RustAPI

```python
import rustapi

app = rustapi.Engine()

@app.post("/upload")
def upload(req):
    doc = req.files["document"][0]
    content = doc.read().decode("utf-8")
    return {"filename": doc.filename, "description": req.form.get("description"), "content": content}

@app.websocket("/ws")
async def websocket_endpoint(ws):
    while True:
        data = ws.receive_text()
        ws.send_text(f"echo: {data}")

```

The comprehensive migration and reference guide has been successfully generated and saved to **`docs.md`**.

###  It contains side-by-side code blocks comparing **FastAPI** against **RustAPI** across all completed milestones:

1. **§0 Core Engine & Routing** (`def` vs `async def`)
2. **§1 Metadata, Custom Responses, & Pydantic Validation** (`req.headers`, `rustapi.Response`, automatic `422` errors)
3. **§2 Dependency Injection & Generator Teardowns** (`Depends` with auto-cleanup)
4. **§5 Production Ergonomics** (`APIRouter` prefixing and `@app.on_event` lifespan hooks)
5. **Advanced I/O** (Multipart `UploadFile` extraction and native bidirectional `WebSocket` streaming)

