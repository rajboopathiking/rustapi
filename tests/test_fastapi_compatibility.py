import datetime
import uuid
from dataclasses import dataclass
from pydantic import BaseModel

import rustapi
from rustapi import (
    FastAPI,
    Request,
    Response,
    status,
    jsonable_encoder,
    EventSourceResponse,
    ServerSentEvent,
    format_sse_event,
    Body,
    Query,
    Path,
    Header,
    Cookie,
    Form,
    File,
    Security,
    WebSocketDisconnect,
    WebSocketException,
)
from rustapi.openapi import (
    get_swagger_ui_html,
    get_redoc_html,
    get_swagger_ui_oauth2_redirect_html,
)
from rustapi.security import OAuth2PasswordBearer, HTTPBearer, APIKeyHeader


def test_fastapi_and_request_aliases():
    assert FastAPI is rustapi.Engine
    assert Request is rustapi.PyRequest


def test_status_codes():
    assert status.HTTP_200_OK == 200
    assert status.HTTP_404_NOT_FOUND == 404
    assert status.HTTP_500_INTERNAL_SERVER_ERROR == 500


def test_openapi_docs_html_generation():
    swagger_res = get_swagger_ui_html(openapi_url="/openapi.json", title="Test API")
    assert swagger_res.status_code == 200
    assert "swagger-ui" in str(swagger_res.content)
    assert "/openapi.json" in str(swagger_res.content)

    redoc_res = get_redoc_html(openapi_url="/openapi.json", title="Test API")
    assert redoc_res.status_code == 200
    assert "<redoc spec-url=\"/openapi.json\"></redoc>" in str(redoc_res.content)

    oauth2_res = get_swagger_ui_oauth2_redirect_html()
    assert oauth2_res.status_code == 200
    assert "swaggerUIRedirectOauth2" in str(oauth2_res.content)


def test_jsonable_encoder():
    @dataclass
    class UserDC:
        name: str
        age: int

    class UserPy(BaseModel):
        name: str
        created_at: datetime.datetime

    now = datetime.datetime.now()
    u_id = uuid.uuid4()

    data = {
        "dc": UserDC(name="Alice", age=30),
        "py": UserPy(name="Bob", created_at=now),
        "id": u_id,
        "date": now.date(),
    }

    encoded = jsonable_encoder(data)
    assert encoded["dc"] == {"name": "Alice", "age": 30}
    assert encoded["py"]["name"] == "Bob"
    assert encoded["id"] == str(u_id)
    assert encoded["date"] == now.date().isoformat()


def test_sse_helpers():
    sse_event = ServerSentEvent(data={"message": "hello"}, event="update", id="evt_123", retry=5000)
    assert sse_event.data == {"message": "hello"}
    assert sse_event.event == "update"

    formatted_bytes = format_sse_event(
        data_str='{"status": "ok"}',
        event="status",
        id="1",
        retry=3000,
        comment="ping test",
    )
    wire_str = formatted_bytes.decode("utf-8")
    assert ": ping test\n" in wire_str
    assert "event: status\n" in wire_str
    assert 'data: {"status": "ok"}\n' in wire_str
    assert "id: 1\n" in wire_str
    assert "retry: 3000\n\n" in wire_str

    assert EventSourceResponse.media_type == "text/event-stream"
    sse_res = EventSourceResponse(content="data: test\n\n")
    assert sse_res.headers.get("Content-Type") == "text/event-stream"



def test_param_functions():
    q = Query(default=10, ge=1, le=100)
    p = Path(description="ID parameter")
    b = Body(default=None)

    assert q.default == 10
    assert q.ge == 1
    assert p.description == "ID parameter"
    assert b.default is None


def test_security_helpers():
    oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")
    bearer_scheme = HTTPBearer()
    api_key_scheme = APIKeyHeader(name="X-API-Key")

    assert oauth2_scheme.tokenUrl == "token"
    assert bearer_scheme.auto_error is True
    assert api_key_scheme.name == "X-API-Key"


def test_websocket_exceptions():
    err = WebSocketDisconnect(code=1001, reason="Going away")
    assert err.code == 1001
    assert err.reason == "Going away"

    ex = WebSocketException(code=1008, reason="Policy violation")
    assert ex.code == 1008


def test_frontend_app_serving(tmp_path):
    dist_dir = tmp_path / "dist"
    dist_dir.mkdir()
    (dist_dir / "index.html").write_text("<h1>My App</h1>")
    (dist_dir / "app.js").write_text("console.log('hi')")

    from rustapi.staticfiles import StaticFiles
    handler = StaticFiles(directory=str(dist_dir), html=True)

    res_index = handler("index.html")
    assert res_index.status_code == 200
    assert b"<h1>My App</h1>" in res_index.content
    assert res_index.headers.get("Content-Type") == "text/html; charset=utf-8"

    res_js = handler("app.js")
    assert res_js.status_code == 200
    assert b"console.log('hi')" in res_js.content
    assert res_js.headers.get("Content-Type") == "application/javascript"

    res_spa = handler("unknown/route")
    assert res_spa.status_code == 200
    assert b"<h1>My App</h1>" in res_spa.content


def test_app_and_router_advanced_fastapi_features(tmp_path):
    app = FastAPI()

    # 1. Route decorator with extra FastAPI kwargs
    @app.get("/items", status_code=201, tags=["items"], summary="Get items", description="Returns items")
    def get_items():
        return {"items": [1, 2, 3]}

    # 2. APIRouter with prefix & kwargs
    router = rustapi.APIRouter(prefix="/users", tags=["users"], dependencies=[])

    @router.get("/me", status_code=200, summary="Current user")
    def get_me():
        return {"user": "bob"}

    app.include_router(router, prefix="/api/v1", tags=["api"])

    # 3. app.frontend
    dist_dir = tmp_path / "app_dist"
    dist_dir.mkdir()
    (dist_dir / "index.html").write_text("<h1>Root App</h1>")
    app.frontend("/", directory=str(dist_dir))

    assert app is not None


