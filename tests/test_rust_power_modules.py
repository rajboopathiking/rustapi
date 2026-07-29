import time
import threading
import pytest
import requests
import rustapi

SECRET = "super_secret_jwt_key_123"


def test_jwt_encode_and_decode():
    payload = {"user_id": 42, "role": "admin", "exp": 9999999999}
    token = rustapi.encode_jwt(payload, SECRET)
    assert isinstance(token, str)
    assert len(token) > 20

    decoded = rustapi.decode_jwt(token, SECRET)
    assert decoded["user_id"] == 42
    assert decoded["role"] == "admin"


def test_jwt_decode_invalid_secret_raises_value_error():
    payload = {"user_id": 42}
    token = rustapi.encode_jwt(payload, SECRET)
    with pytest.raises(ValueError, match="JWT Decoding Error"):
        rustapi.decode_jwt(token, "wrong_secret")


def test_argon2_password_hashing_and_verification():
    password = "MySecurePassword123!"
    password_hash = rustapi.hash_password(password)

    assert isinstance(password_hash, str)
    assert password_hash.startswith("$argon2")

    # Verify matching password
    assert rustapi.verify_password(password, password_hash) is True

    # Verify wrong password
    assert rustapi.verify_password("WrongPassword!", password_hash) is False


def test_minijinja_template_rendering():
    template = "Hello {{ name }}! You have {{ unread }} unread messages."
    context = {"name": "Boopathi", "unread": 5}

    rendered = rustapi.render_template(template, context)
    assert rendered == "Hello Boopathi! You have 5 unread messages."


# ---- Integrated HTTP Route Test ----
HOST = "127.0.0.1"
PORT = 8014
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()


@app.post("/auth/hash")
def hash_endpoint(req):
    body = req.form
    raw_pass = body.get("password", "")
    h = rustapi.hash_password(raw_pass)
    token = rustapi.encode_jwt({"sub": "user_1"}, SECRET)
    return {"hash": h, "token": token}


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
            r = requests.get(f"{BASE}/docs", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)
    assert connected, "Server failed to start for Phase C tests"


def test_integrated_http_auth_route():
    r = requests.post(f"{BASE}/auth/hash", data={"password": "MySecretPassword"})
    assert r.status_code == 200
    data = r.json()
    assert data["hash"].startswith("$argon2")

    decoded = rustapi.decode_jwt(data["token"], SECRET)
    assert decoded["sub"] == "user_1"
