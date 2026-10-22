import asyncio
import json
from typing import List, Optional
from contextlib import asynccontextmanager
import pytest
from pydantic import BaseModel, Field
from rustapi import FastAPI, APIRouter
from rustapi.responses import JSONResponse
from rustapi.exceptions import ResponseValidationError
from httpx import ASGITransport, AsyncClient


class ItemModel(BaseModel):
    name: str
    secret: str = "default_secret"
    alias_field: str = Field(alias="aliasField")


class StrictModel(BaseModel):
    name: str
    age: int


# Subclass models for testing field stripping
class UserOut(BaseModel):
    id: int
    name: str


class UserInDB(UserOut):
    password: str
    salt: str = "random_salt"


class RoleOut(BaseModel):
    role_name: str


class RoleInDB(RoleOut):
    role_secret: str


class MemberOut(BaseModel):
    user: UserOut
    role: RoleOut


class MemberInDB(BaseModel):
    user: UserInDB
    role: RoleInDB


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
async def test_subclass_single_instance_extra_fields_stripped():
    """Returning a model subclass must not expose additional fields such as password."""
    app = FastAPI()

    @app.get("/user", response_model=UserOut)
    def get_user():
        return UserInDB(id=1, name="Alice", password="super_secret_password")

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/user")
        assert resp.status_code == 200
        data = resp.json()
        assert data == {"id": 1, "name": "Alice"}
        assert "password" not in data
        assert "salt" not in data


@pytest.mark.asyncio
async def test_subclass_list_instances_extra_fields_stripped():
    """Returning a list of model subclasses must not expose additional fields."""
    app = FastAPI()

    @app.get("/users", response_model=List[UserOut])
    def get_users():
        return [
            UserInDB(id=1, name="Alice", password="pwd1"),
            UserInDB(id=2, name="Bob", password="pwd2"),
        ]

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/users")
        assert resp.status_code == 200
        data = resp.json()
        assert data == [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]
        for item in data:
            assert "password" not in item
            assert "salt" not in item


@pytest.mark.asyncio
async def test_subclass_nested_model_extra_fields_stripped():
    """Returning nested model subclasses must not expose additional fields."""
    app = FastAPI()

    @app.get("/member", response_model=MemberOut)
    def get_member():
        return MemberInDB(
            user=UserInDB(id=42, name="Charlie", password="nested_secret"),
            role=RoleInDB(role_name="Admin", role_secret="top_secret_role"),
        )

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/member")
        assert resp.status_code == 200
        data = resp.json()
        assert data == {
            "user": {"id": 42, "name": "Charlie"},
            "role": {"role_name": "Admin"},
        }
        assert "password" not in data["user"]
        assert "role_secret" not in data["role"]


@pytest.mark.asyncio
async def test_non_nullable_response_model_returning_none_yields_500():
    """A non-nullable response model returning None must produce HTTP 500."""
    app = FastAPI()

    @app.get("/none-strict", response_model=UserOut)
    def get_none():
        return None

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/none-strict")
        assert resp.status_code == 500


@pytest.mark.asyncio
async def test_explicitly_nullable_response_model_returning_none_yields_200():
    """Explicitly nullable models must accept None and return 200."""
    app = FastAPI()

    @app.get("/none-nullable", response_model=Optional[UserOut])
    def get_none_nullable():
        return None

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/none-nullable")
        assert resp.status_code == 200
        assert resp.json() is None


@pytest.mark.asyncio
async def test_request_validation_error_is_422_distinct_from_response_500():
    """Invalid request input must return 422; invalid server output must return 500."""
    app = FastAPI()

    @app.post("/items", response_model=UserOut)
    def create_item(item: StrictModel):
        return UserOut(id=item.age, name=item.name)

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        # Invalid client input (missing required 'age') -> 422
        resp_input_invalid = await client.post("/items", json={"name": "Alice"})
        assert resp_input_invalid.status_code == 422
        assert "detail" in resp_input_invalid.json()

        # Valid client input -> 200
        resp_valid = await client.post("/items", json={"name": "Alice", "age": 25})
        assert resp_valid.status_code == 200
        assert resp_valid.json() == {"id": 25, "name": "Alice"}


@pytest.mark.asyncio
async def test_lifespan_context_startup_and_shutdown():
    lifecycle_state = {"startup": False, "shutdown": False}

    @asynccontextmanager
    async def app_lifespan(app):
        lifecycle_state["startup"] = True
        yield
        lifecycle_state["shutdown"] = True

    app = FastAPI(lifespan=app_lifespan)

    # Coordinate ASGI lifespan concurrently with timeout
    receive_queue = asyncio.Queue()
    startup_complete_event = asyncio.Event()

    async def receive():
        return await receive_queue.get()

    async def send(msg):
        if msg["type"] == "lifespan.startup.complete":
            startup_complete_event.set()

    lifespan_task = asyncio.create_task(app({"type": "lifespan"}, receive, send))

    # Send startup message
    await receive_queue.put({"type": "lifespan.startup"})
    await asyncio.wait_for(startup_complete_event.wait(), timeout=5.0)
    assert lifecycle_state["startup"] is True

    # Send shutdown message and await task completion
    await receive_queue.put({"type": "lifespan.shutdown"})
    await asyncio.wait_for(lifespan_task, timeout=5.0)
    assert lifecycle_state["shutdown"] is True


@pytest.mark.asyncio
async def test_user_exact_ml_model_lifespan():
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

    # Coordinate lifespan concurrently:
    receive_queue = asyncio.Queue()
    startup_complete_event = asyncio.Event()

    async def receive():
        return await receive_queue.get()

    async def send(msg):
        if msg["type"] == "lifespan.startup.complete":
            startup_complete_event.set()

    lifespan_task = asyncio.create_task(app({"type": "lifespan"}, receive, send))

    # 1. Startup
    await receive_queue.put({"type": "lifespan.startup"})
    await asyncio.wait_for(startup_complete_event.wait(), timeout=5.0)
    assert ml_models.get("hero_model") == "Loaded Model Instance"

    # 2. HTTP Request while server is live
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

    # 3. Shutdown
    await receive_queue.put({"type": "lifespan.shutdown"})
    await asyncio.wait_for(lifespan_task, timeout=5.0)
    assert len(ml_models) == 0
