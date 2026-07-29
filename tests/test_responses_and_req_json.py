import time
import threading
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8016
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()


@app.get("/html")
def get_html():
    template = "<h1>Hello {{ name }}!</h1>"
    context = {"name": "Boopathi"}
    rendered = rustapi.render_template(template, context)
    return rustapi.HTMLResponse(rendered)


@app.get("/text")
def get_text():
    return rustapi.PlainTextResponse("Hello Plain Text")


@app.get("/json_resp")
def get_json():
    return rustapi.JSONResponse({"status": "ok", "code": 200})


@app.get("/redirect")
def get_redirect():
    return rustapi.RedirectResponse("/html", status_code=307)


@app.post("/auth/json")
def auth_json(req):
    data = req.json()
    password = data.get("password", "")
    h = rustapi.hash_password(password)
    return {"hash": h, "received_user": data.get("username")}


@pytest.fixture(scope="module", autouse=True)
def run_server():
    server_thread = threading.Thread(
        target=lambda: app.run(host=HOST, port=PORT),
        daemon=True,
    )
    server_thread.start()

    connected = False
    for _ in range(20):
        try:
            r = requests.get(f"{BASE}/text", timeout=1)
            if r.status_code == 200:
                connected = True
                break
            else:
                import sys
                print(f"DEBUG: Status code was {r.status_code}", file=sys.stderr)
        except Exception as e:
            import sys
            print(f"DEBUG: Exception during connect: {type(e).__name__}: {e}", file=sys.stderr)
            time.sleep(0.1)
    assert connected, "Server failed to start for test_responses_and_req_json"


def test_html_response():
    r = requests.get(f"{BASE}/html")
    assert r.status_code == 200
    assert "text/html" in r.headers.get("content-type", "")
    assert r.text == "<h1>Hello Boopathi!</h1>"


def test_plain_text_response():
    r = requests.get(f"{BASE}/text")
    assert r.status_code == 200
    assert "text/plain" in r.headers.get("content-type", "")
    assert r.text == "Hello Plain Text"


def test_json_response():
    r = requests.get(f"{BASE}/json_resp")
    assert r.status_code == 200
    assert "application/json" in r.headers.get("content-type", "")
    assert r.json() == {"status": "ok", "code": 200}


def test_redirect_response():
    r = requests.get(f"{BASE}/redirect", allow_redirects=False)
    assert r.status_code == 307
    assert r.headers.get("location") == "/html"


def test_req_json_parsing():
    r = requests.post(
        f"{BASE}/auth/json",
        json={"username": "boopathi", "password": "SuperSecretPassword123"},
    )
    assert r.status_code == 200
    data = r.json()
    assert data["received_user"] == "boopathi"
    assert data["hash"].startswith("$argon2")
