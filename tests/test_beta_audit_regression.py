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
    lifecycle_state = {"startup": False, "shutdown": False}

    @asynccontextmanager
    async def app_lifespan(app):
        lifecycle_state["startup"] = True
        yield
        lifecycle_state["shutdown"] = True

    app = FastAPI(lifespan=app_lifespan)

    # Lifespan via ASGI transport
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test"):
        assert lifecycle_state["startup"] is True

    assert lifecycle_state["shutdown"] is True
