import asyncio
from contextlib import asynccontextmanager, contextmanager
import pytest
from pydantic import BaseModel
from rustapi import FastAPI, APIRouter, Depends, HTTPException, Body, status

# 1. Models for testing
class PublicUser(BaseModel):
    name: str

class PrivateUser(BaseModel):
    name: str
    password: str
    age: int

class Item(BaseModel):
    name: str
    price: float


# ---------------------------------------------------------------------------
# Test 1: Async handler with response_model
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_async_handler_with_response_model():
    app = FastAPI()

    @app.get("/user", response_model=PublicUser)
    async def get_user():
        return {"name": "alice", "password": "SECRET"}

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/user",
        "headers": [],
        "query_string": b"",
    }

    response_messages = []
    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def send(message):
        response_messages.append(message)

    await app(scope, receive, send)

    start_msg = response_messages[0]
    body_msg = response_messages[1]

    assert start_msg["status"] == 200
    body = body_msg["body"].decode("utf-8")
    assert "<coroutine object" not in body
    import json
    data = json.loads(body)
    assert data == {"name": "alice"}
    assert "password" not in data


# ---------------------------------------------------------------------------
# Test 2: Status code 201 on route registration
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_declared_status_code_201():
    app = FastAPI()

    @app.post("/items", status_code=201)
    def create_item():
        return {"status": "created"}

    @app.post("/items_async", status_code=201)
    async def create_item_async():
        return {"status": "created_async"}

    for path in ["/items", "/items_async"]:
        scope = {
            "type": "http",
            "method": "POST",
            "path": path,
            "headers": [],
            "query_string": b"",
        }
        response_messages = []
        async def receive():
            return {"type": "http.request", "body": b"", "more_body": False}
        async def send(message):
            response_messages.append(message)

        await app(scope, receive, send)
        assert response_messages[0]["status"] == 201


# ---------------------------------------------------------------------------
# Test 3: response_model_exclude and response_model_include
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_response_model_exclude_and_include():
    app = FastAPI()

    @app.get("/user_exclude", response_model=PrivateUser, response_model_exclude={"password", "age"})
    async def get_user_exclude():
        return {"name": "alice", "password": "SECRET", "age": 30}

    @app.get("/user_include", response_model=PrivateUser, response_model_include={"name"})
    def get_user_include():
        return {"name": "bob", "password": "SECRET", "age": 25}

    import json

    # Test exclude
    scope_ex = {"type": "http", "method": "GET", "path": "/user_exclude", "headers": [], "query_string": b""}
    msgs_ex = []
    async def receive_ex():
        return {"type": "http.request", "body": b"", "more_body": False}
    async def send_ex(m):
        msgs_ex.append(m)
    await app(scope_ex, receive_ex, send_ex)
    data_ex = json.loads(msgs_ex[1]["body"].decode("utf-8"))
    assert data_ex == {"name": "alice"}
    assert "password" not in data_ex
    assert "age" not in data_ex

    # Test include
    scope_in = {"type": "http", "method": "GET", "path": "/user_include", "headers": [], "query_string": b""}
    msgs_in = []
    async def receive_in():
        return {"type": "http.request", "body": b"", "more_body": False}
    async def send_in(m):
        msgs_in.append(m)
    await app(scope_in, receive_in, send_in)
    data_in = json.loads(msgs_in[1]["body"].decode("utf-8"))
    assert data_in == {"name": "bob"}
    assert "password" not in data_in


# ---------------------------------------------------------------------------
# Test 4: FastAPI(lifespan=...) startup and shutdown via ASGI
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_fastapi_lifespan_startup_and_shutdown():
    events = []

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        events.append("startup")
        yield
        events.append("shutdown")

    app = FastAPI(lifespan=lifespan)

    # Simulate ASGI lifespan messages
    sent_messages = []
    receive_queue = asyncio.Queue()
    await receive_queue.put({"type": "lifespan.startup"})

    async def receive():
        return await receive_queue.get()

    async def send(msg):
        sent_messages.append(msg)
        if msg["type"] == "lifespan.startup.complete":
            # Now trigger shutdown
            await receive_queue.put({"type": "lifespan.shutdown"})

    await app({"type": "lifespan"}, receive, send)

    assert events == ["startup", "shutdown"]
    assert sent_messages[0]["type"] == "lifespan.startup.complete"
    assert sent_messages[1]["type"] == "lifespan.shutdown.complete"


# ---------------------------------------------------------------------------
# Test 5: Depends(..., scope="function") compatibility
# ---------------------------------------------------------------------------
def test_depends_scope_parameter():
    def dummy_dep():
        return "dep_val"

    dep1 = Depends(dummy_dep, scope="function")
    assert dep1.dependency == dummy_dep
    assert dep1.scope == "function"
    assert dep1.use_cache is True

    dep2 = Depends(dummy_dep, use_cache=False, scope="request")
    assert dep2.scope == "request"
    assert dep2.use_cache is False


# ---------------------------------------------------------------------------
# Test 6: Invalid request body returns 422 with structured JSON
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_invalid_request_body_returns_422():
    app = FastAPI()

    @app.post("/items")
    def create_item(item: Item):
        return {"name": item.name, "price": item.price}

    # Pass invalid JSON / missing required fields
    scope = {
        "type": "http",
        "method": "POST",
        "path": "/items",
        "headers": [(b"content-type", b"application/json")],
        "query_string": b"",
    }

    import json
    invalid_body = json.dumps({"name": "widget"}).encode("utf-8")  # missing price

    msgs = []
    async def receive():
        return {"type": "http.request", "body": invalid_body, "more_body": False}
    async def send(msg):
        msgs.append(msg)

    await app(scope, receive, send)

    assert msgs[0]["status"] == 422
    body_str = msgs[1]["body"].decode("utf-8")
    data = json.loads(body_str)
    assert "detail" in data
    assert isinstance(data["detail"], list)
    # Ensure error identifies the missing 'price' field
    assert any("price" in str(err) for err in data["detail"])


# ---------------------------------------------------------------------------
# Test 7: Malformed JSON returns 422 with structured error
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_malformed_json_returns_422():
    app = FastAPI()

    @app.post("/items")
    def create_item(item: Item):
        return {"name": item.name}

    scope = {
        "type": "http",
        "method": "POST",
        "path": "/items",
        "headers": [(b"content-type", b"application/json")],
        "query_string": b"",
    }

    malformed_body = b"{"  # Syntax error

    msgs = []
    async def receive():
        return {"type": "http.request", "body": malformed_body, "more_body": False}
    async def send(msg):
        msgs.append(msg)

    await app(scope, receive, send)

    assert msgs[0]["status"] == 422
    import json
    data = json.loads(msgs[1]["body"].decode("utf-8"))
    assert "detail" in data


# ---------------------------------------------------------------------------
# Test 8: APIRouter response_model and status_code forwarding
# ---------------------------------------------------------------------------
@pytest.mark.asyncio
async def test_apirouter_response_model_and_status_code():
    router = APIRouter(prefix="/api/v1")

    @router.post("/user", response_model=PublicUser, status_code=201)
    async def create_user_route():
        return {"name": "charlie", "password": "HIDDEN"}

    app = FastAPI()
    app.include_router(router)

    scope = {
        "type": "http",
        "method": "POST",
        "path": "/api/v1/user",
        "headers": [],
        "query_string": b"",
    }

    msgs = []
    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}
    async def send(msg):
        msgs.append(msg)

    await app(scope, receive, send)

    assert msgs[0]["status"] == 201
    import json
    data = json.loads(msgs[1]["body"].decode("utf-8"))
    assert data == {"name": "charlie"}
    assert "password" not in data
