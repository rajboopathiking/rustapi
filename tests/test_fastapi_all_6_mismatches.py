import pytest
import requests
import threading
import time
import asyncio
from rustapi import FastAPI, APIRouter, Request, Depends, HTTPException, status
from rustapi.security import HTTPBearer, HTTPAuthorizationCredentials, OAuth2PasswordBearer
from rustapi.uploads import UploadFile
from rustapi.responses import JSONResponse


def test_1_apirouter_include_router():
    sub_router = APIRouter(prefix="/v1", tags=["sub"])

    @sub_router.get("/ping")
    def ping():
        return {"ping": "pong"}

    parent_router = APIRouter(prefix="/api", tags=["parent"])
    parent_router.include_router(sub_router, prefix="/nested")

    app = FastAPI()
    app.include_router(parent_router)

    # Check that route was registered under /api/nested/v1/ping
    route_paths = [r[1] for r in parent_router.routes]
    assert "/nested/v1/ping" in route_paths


def test_2_fastapi_constructor_kwargs():
    app = FastAPI(
        title="My Custom API",
        description="A great API",
        version="2.0.0",
        openapi_url="/custom_openapi.json",
        docs_url="/custom_docs",
        redoc_url="/custom_redoc",
    )
    assert app.title == "My Custom API"
    assert app.description == "A great API"
    assert app.version == "2.0.0"


def test_3_exception_handlers():
    app = FastAPI()

    class CustomError(Exception):
        def __init__(self, message: str):
            self.message = message

    @app.exception_handler(CustomError)
    def handle_custom_error(request: Request, exc: CustomError):
        return JSONResponse(status_code=418, content={"custom_detail": exc.message})

    assert CustomError in app.exception_handlers


def test_4_http_bearer_and_credentials():
    bearer_scheme = HTTPBearer()

    # Fake request with headers
    class FakeRequest:
        headers = {"authorization": "Bearer secret_token_123"}

    creds = bearer_scheme(FakeRequest())
    assert isinstance(creds, HTTPAuthorizationCredentials)
    assert creds.scheme == "Bearer"
    assert creds.credentials == "secret_token_123"


def test_5_dependency_resolver_with_request_and_nested_depends():
    from rustapi.resolver import solve_dependency

    oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

    async def get_current_user(request: Request, token: str = Depends(oauth2_scheme)):
        assert token == "secret_token_abc"
        return {"username": "alice", "path": request.path}

    async def get_admin_user(request: Request, user=Depends(get_current_user)):
        return {"admin": user["username"], "path": user["path"]}

    class FakeReq:
        path = "/admin/dashboard"
        headers = {"authorization": "Bearer secret_token_abc"}

    res = asyncio.run(solve_dependency(get_admin_user, FakeReq()))
    assert res == {"admin": "alice", "path": "/admin/dashboard"}


def test_6_upload_file_sync_and_async_methods():
    import io

    file_obj = UploadFile(file=io.BytesIO(b"Hello World Content"), filename="photo.png", content_type="image/png")
    assert file_obj.filename == "photo.png"
    assert file_obj.content_type == "image/png"

    async def _test():
        # Async read
        content_async = await file_obj.read()
        assert content_async == b"Hello World Content"
        assert isinstance(content_async, bytes)

        # Sync seek & read
        file_obj.seek(0)
        content_sync = file_obj.read()
        assert content_sync == b"Hello World Content"

        # Async seek & close
        await file_obj.seek(0)
        await file_obj.close()

    asyncio.run(_test())


def test_7_openapi_security_schemes_and_http_exception_status_codes():
    app = FastAPI()
    oauth2_scheme = OAuth2PasswordBearer(tokenUrl="/api/v1/auth/login")
    bearer_scheme = HTTPBearer()

    @app.get("/secure/me")
    def secure_me(token: str = Depends(oauth2_scheme)):
        if not token:
            raise HTTPException(status_code=401, detail="Unauthorized")
        return {"status": "ok"}

    @app.get("/admin/logs")
    def admin_logs(token: HTTPAuthorizationCredentials = Depends(bearer_scheme)):
        return {"logs": []}

    def run_server():
        app.run(host="127.0.0.1", port=8994)

    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1.5)

    # 1. Test OpenAPI securitySchemes generation
    res_openapi = requests.get("http://127.0.0.1:8994/openapi.json")
    assert res_openapi.status_code == 200
    openapi_doc = res_openapi.json()
    assert "components" in openapi_doc
    assert "securitySchemes" in openapi_doc["components"]
    schemes = openapi_doc["components"]["securitySchemes"]
    assert "OAuth2PasswordBearer" in schemes or "HTTPBearer" in schemes

    # 2. Test HTTPException status code propagation (401 instead of 500)
    res_unauth = requests.get("http://127.0.0.1:8994/secure/me")
    assert res_unauth.status_code == 401
    assert "Not authenticated" in res_unauth.json()["detail"] or "Unauthorized" in res_unauth.json()["detail"]

