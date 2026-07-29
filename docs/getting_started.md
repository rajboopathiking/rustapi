# 🚀 Getting Started with RustAPI

RustAPI provides a FastAPI-compatible surface backed by a high-performance Tokio / Hyper core in Rust.

---

## 📌 Core Routing & Handlers

Handlers can be synchronous `def` or asynchronous `async def`. Sync handlers are executed safely inside Tokio's thread-pool without blocking incoming requests.

```python
from rustapi import Engine

app = Engine()

@app.get("/")
def sync_root():
    return {"message": "Sync handler routed by Rust"}

@app.get("/async")
async def async_root():
    return {"message": "Async handler routed by Rust"}
```

---

## 🔍 Path & Query Parameters

Parameters are automatically coerced into `int`, `float`, `bool`, or `str` based on Python type hints:

```python
@app.get("/items/{item_id}")
def read_item(item_id: int, q: str = "default"):
    return {"item_id": item_id, "query": q}
```

If a required parameter is missing or invalid, RustAPI automatically returns a structured `422 Unprocessable Entity` error.

---

## 🛡️ Pydantic Request Validation & `response_model`

Request payloads are validated using Pydantic models. Return values can be filtered through a `response_model`:

```python
from pydantic import BaseModel
from rustapi import Engine

class UserCreate(BaseModel):
    username: str
    email: str

class UserOut(BaseModel):
    id: int
    username: str

@app.post("/users", response_model=UserOut)
def create_user(user: UserCreate):
    return {
        "id": 100,
        "username": user.username,
        "password_hash": "secret",  # Automatically filtered out by UserOut schema
        "email": user.email,
    }
```

---

## 💉 Dependency Injection & Overrides (`app.dependency_overrides`)

FastAPI-style `Depends` with request-scoped caching is supported out of the box:

```python
from rustapi import Engine, Depends

app = Engine()

def get_db():
    return {"db": "production_sqlite"}

@app.get("/data")
def get_data(db = Depends(get_db)):
    return {"db": db}

# Swapping dependencies in tests:
app.dependency_overrides[get_db] = lambda: {"db": "test_sqlite"}
```

---

## 📖 OpenAPI & Swagger UI

Interactive OpenAPI documentation is generated automatically:
* **OpenAPI JSON**: `GET /openapi.json`
* **Swagger UI**: `GET /docs`
