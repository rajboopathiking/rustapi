import pytest
from pydantic import BaseModel, Field
from rustapi import FastAPI, APIRouter
from rustapi.responses import JSONResponse
from rustapi.exceptions import ResponseValidationError
from httpx import ASGITransport, AsyncClient
from contextlib import asynccontextmanager


class ItemModel(BaseModel):
    name: str
    secret: str = "default_secret"
    alias_field: str = Field(alias="aliasField")


class StrictModel(BaseModel):
    name: str
    age: int


@pytest.mark.asyncio
async def test_response_model_by_alias_preserves_alias():
    app = FastAPI()

    @app.get("/alias", response_model=ItemModel, response_model_by_alias=True)
    def get_alias():
        return {"name": "test", "secret": "shh", "aliasField": "my_alias_val"}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/alias")
        assert resp.status_code == 200
        data = resp.json()
        assert "aliasField" in data
        assert "alias_field" not in data
        assert data["aliasField"] == "my_alias_val"


@pytest.mark.asyncio
async def test_response_model_exclude_with_defaults_does_not_reappear():
    app = FastAPI()

    @app.get("/exclude", response_model=ItemModel, response_model_exclude={"secret"})
    def get_excluded():
        return {"name": "test", "secret": "custom_secret", "aliasField": "val"}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/exclude")
        assert resp.status_code == 200
        data = resp.json()
        assert "secret" not in data
        assert data["name"] == "test"


@pytest.mark.asyncio
async def test_response_validation_failure_returns_500():
    app = FastAPI()

    @app.get("/invalid", response_model=StrictModel)
    def get_invalid():
        # Missing required 'age' field -> must raise ResponseValidationError -> 500
        return {"name": "only_name"}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/invalid")
        assert resp.status_code == 500


@pytest.mark.asyncio
async def test_explicit_json_response_with_response_model_bypasses_validation():
    app = FastAPI()

    @app.get("/explicit", response_model=StrictModel)
    def get_explicit():
        # Handler returns custom JSONResponse directly; should not fail validation or return 500
        return JSONResponse(content={"custom": "override"}, status_code=200)

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/explicit")
        assert resp.status_code == 200
        assert resp.json() == {"custom": "override"}


@pytest.mark.asyncio
async def test_router_response_model_preserves_exclusions():
    router = APIRouter()

    @router.get("/sub-exclude", response_model=ItemModel, response_model_exclude={"secret"})
    def router_exclude():
        return {"name": "sub", "secret": "shh", "aliasField": "val"}

    app = FastAPI()
    app.include_router(router, prefix="/api")

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/api/sub-exclude")
        assert resp.status_code == 200
        data = resp.json()
        assert "secret" not in data
        assert data["name"] == "sub"


@pytest.mark.asyncio
async def test_lifespan_context_startup_and_shutdown():
    import asyncio
    lifecycle_state = {"startup": False, "shutdown": False}

    @asynccontextmanager
    async def app_lifespan(app):
        lifecycle_state["startup"] = True
        yield
        lifecycle_state["shutdown"] = True

    app = FastAPI(lifespan=app_lifespan)

    # Send ASGI lifespan startup and shutdown messages
    receive_queue = asyncio.Queue()
    await receive_queue.put({"type": "lifespan.startup"})

    async def receive():
        return await receive_queue.get()

    async def send(msg):
        if msg["type"] == "lifespan.startup.complete":
            await receive_queue.put({"type": "lifespan.shutdown"})

    await app({"type": "lifespan"}, receive, send)

    assert lifecycle_state["startup"] is True
    assert lifecycle_state["shutdown"] is True


@pytest.mark.asyncio
async def test_user_exact_ml_model_lifespan():
    import json
    ml_models = {}

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        # Load the ML model or connect to database on startup
        ml_models["hero_model"] = "Loaded Model Instance"
        yield
        # Clean up the resources and release the memory on shutdown
        ml_models.clear()

    app = FastAPI(lifespan=lifespan)

    @app.get("/")
    async def read_root():
        return {"model": ml_models.get("hero_model")}

    # 1. Simulate ASGI startup
    receive_queue = asyncio.Queue()
    await receive_queue.put({"type": "lifespan.startup"})

    async def receive():
        return await receive_queue.get()

    async def send(msg):
        pass

    await app({"type": "lifespan"}, receive, send)
    assert ml_models.get("hero_model") == "Loaded Model Instance"

    # 2. HTTP Request to read_root endpoint
    scope = {"type": "http", "method": "GET", "path": "/", "headers": [], "query_string": b""}
    http_resps = []

    async def http_recv():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def http_send(m):
        http_resps.append(m)

    await app(scope, http_recv, http_send)
    assert http_resps[0]["status"] == 200
    data = json.loads(http_resps[1]["body"].decode("utf-8"))
    assert data == {"model": "Loaded Model Instance"}

    # 3. Simulate ASGI shutdown
    await receive_queue.put({"type": "lifespan.shutdown"})
    await app({"type": "lifespan"}, receive, send)
    assert len(ml_models) == 0

