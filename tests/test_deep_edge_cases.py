import time
import io
import threading
import pytest
import requests
from pydantic import BaseModel, field_validator
import rustapi
from rustapi import (
    FastAPI,
    HTTPException,
    JSONResponse,
    HTMLResponse,
    PlainTextResponse,
    RedirectResponse,
    EventSourceResponse,
    ServerSentEvent,
    BackgroundTasks,
    encode_jwt,
    decode_jwt,
    hash_password,
    verify_password,
    render_template,
)

HOST = "127.0.0.1"
PORT = 8055
BASE = f"http://{HOST}:{PORT}"

app = FastAPI(title="Deep Edge Cases Test API")
bg_executed = []


class CustomError(Exception):
    def __init__(self, message: str):
        self.message = message


@app.exception_handler(CustomError)
def custom_exception_handler(req, exc: CustomError):
    return JSONResponse({"error_type": "CustomError", "detail": exc.message}, status_code=418)


class StrictUserInput(BaseModel):
    username: str
    age: int


def background_worker(task_id: str):
    time.sleep(0.05)
    bg_executed.append(task_id)


@app.post("/validate-user")
def validate_user(user: StrictUserInput):
    return {"status": "valid", "username": user.username, "age": user.age}


@app.get("/trigger-http-exception")
def trigger_http_exception():
    raise HTTPException(status_code=418, detail="Teapot error triggered")


@app.post("/trigger-bg-task")
def trigger_bg_task(task_id: str, bg: BackgroundTasks):
    bg.add_task(background_worker, task_id)
    return {"message": "task scheduled"}


@app.get("/custom-response")
def custom_response():
    return JSONResponse(
        content={"key": "value"},
        status_code=202,
        headers={"X-Custom-Header": "RustAPI-FastPath"},
    )


@app.get("/sse-stream")
def sse_stream():
    def event_generator():
        yield ServerSentEvent(data="message 1", event="update", id="1")
        yield ServerSentEvent(data="message 2", event="update", id="2")

    return EventSourceResponse(event_generator())


@pytest.fixture(scope="module", autouse=True)
def run_edge_case_server():
    server_thread = threading.Thread(
        target=lambda: app.run(host=HOST, port=PORT),
        daemon=True,
    )
    server_thread.start()

    connected = False
    for _ in range(25):
        try:
            r = requests.get(f"{BASE}/custom-response", timeout=1)
            if r.status_code == 202:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Deep edge case server failed to launch"


def test_pydantic_validation_success():
    r = requests.post(f"{BASE}/validate-user", json={"username": "dev", "age": 25})
    assert r.status_code == 200
    data = r.json()
    assert data == {"status": "valid", "username": "dev", "age": 25}


def test_custom_exception_handler_registration():
    assert CustomError in app.exception_handlers


def test_http_exception_interception():
    r = requests.get(f"{BASE}/trigger-http-exception")
    assert r.status_code == 418
    data = r.json()
    assert data["detail"] == "Teapot error triggered"


def test_custom_json_response_headers_and_status():
    r = requests.get(f"{BASE}/custom-response")
    assert r.status_code == 202
    assert r.headers.get("X-Custom-Header") == "RustAPI-FastPath"
    assert r.json() == {"key": "value"}


def test_background_task_execution():
    r = requests.post(f"{BASE}/trigger-bg-task?task_id=bg_test_1001")
    assert r.status_code == 200
    assert r.json()["message"] == "task_scheduled" or r.json()["message"] == "task scheduled"

    time.sleep(0.2)
    assert "bg_test_1001" in bg_executed


def test_argon2_password_hashing_security():
    password = "SuperSecretPassword123!"
    hashed = hash_password(password)
    assert hashed != password
    assert verify_password(password, hashed) is True
    assert verify_password("WrongPassword", hashed) is False


def test_jwt_encode_and_decode_claims():
    payload = {"sub": "user_987", "role": "admin", "exp": 9999999999}
    secret = "TopSecretJWTKey456"
    token = encode_jwt(payload, secret)
    assert isinstance(token, str)

    decoded = decode_jwt(token, secret)
    assert decoded["sub"] == "user_987"
    assert decoded["role"] == "admin"


def test_minijinja_template_rendering_engine():
    template = "Hello {{ name }}! Items: {% for item in items %}{{ item }}{% if not loop.last %}, {% endif %}{% endfor %}"
    context = {"name": "RustAPI Developer", "items": ["Fast", "Native", "Safe"]}

    rendered = render_template(template, context)
    assert rendered == "Hello RustAPI Developer! Items: Fast, Native, Safe"
