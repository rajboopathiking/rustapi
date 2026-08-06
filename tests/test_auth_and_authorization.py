import pytest
import requests
import threading
import time
import asyncio
import jwt
from rustapi import FastAPI, Request, Depends, HTTPException, status
from rustapi.security import HTTPBearer, HTTPAuthorizationCredentials, OAuth2PasswordBearer, APIKeyHeader
from rustapi.responses import JSONResponse

SECRET_KEY = "supersecretkey123"
ALGORITHM = "HS256"

bearer_scheme = HTTPBearer()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="/auth/token")
api_key_scheme = APIKeyHeader(name="X-API-Key")


def create_token(username: str, role: str) -> str:
    return jwt.encode({"sub": username, "role": role}, SECRET_KEY, algorithm=ALGORITHM)


async def get_current_user(token: str = Depends(oauth2_scheme)) -> dict:
    if not token:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Authorization header missing or invalid",
        )
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])
        return {"username": payload["sub"], "role": payload["role"]}
    except Exception:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid or expired token",
        )


async def get_admin_user(current_user: dict = Depends(get_current_user)) -> dict:
    if current_user["role"] not in ("admin", "sysadmin"):
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Admin privileges required",
        )
    return current_user


app = FastAPI(title="Auth & Authorization Test Suite App")


@app.post("/auth/token")
def login(req: Request):
    return {"access_token": create_token("alice", "admin"), "token_type": "bearer"}


@app.get("/users/me")
async def read_me(current_user: dict = Depends(get_current_user)):
    return {"user": current_user["username"], "role": current_user["role"]}


@app.get("/admin/dashboard")
async def read_admin_dashboard(admin_user: dict = Depends(get_admin_user)):
    return {"message": f"Welcome Admin {admin_user['username']}!"}


@app.get("/api-key-protected")
def read_api_key(key: str = Depends(api_key_scheme)):
    if key != "secret-api-key-999":
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid API Key")
    return {"status": "valid key"}


def test_authentication_and_authorization_full_suite():
    port = 9005
    t = threading.Thread(target=lambda: app.run(host="127.0.0.1", port=port), daemon=True)
    t.start()
    time.sleep(1.5)

    base_url = f"http://127.0.0.1:{port}"

    # 1. OpenAPI securitySchemes verification
    res_openapi = requests.get(f"{base_url}/openapi.json")
    assert res_openapi.status_code == 200
    doc = res_openapi.json()
    assert "components" in doc
    assert "securitySchemes" in doc["components"]
    assert "OAuth2PasswordBearer" in doc["components"]["securitySchemes"] or "HTTPBearer" in doc["components"]["securitySchemes"]

    # 2. Authentication failure: Missing token -> 401 Unauthorized (NOT 500)
    res_no_token = requests.get(f"{base_url}/users/me")
    assert res_no_token.status_code == 401
    assert "Authorization header missing" in res_no_token.json()["detail"] or "Not authenticated" in res_no_token.json()["detail"]

    # 3. Authentication failure: Invalid token -> 401 Unauthorized
    res_bad_token = requests.get(f"{base_url}/users/me", headers={"Authorization": "Bearer invalid_garbage_token"})
    assert res_bad_token.status_code == 401

    # 4. Authentication success: Valid user token
    user_token = create_token("bob", "user")
    res_user = requests.get(f"{base_url}/users/me", headers={"Authorization": f"Bearer {user_token}"})
    assert res_user.status_code == 200
    assert res_user.json() == {"user": "bob", "role": "user"}

    # 5. Authorization failure: User accessing Admin endpoint -> 403 Forbidden (NOT 500)
    res_admin_denied = requests.get(f"{base_url}/admin/dashboard", headers={"Authorization": f"Bearer {user_token}"})
    assert res_admin_denied.status_code == 403
    assert res_admin_denied.json()["detail"] == "Admin privileges required"

    # 6. Authorization success: Admin accessing Admin endpoint -> 200 OK
    admin_token = create_token("alice", "admin")
    res_admin_allowed = requests.get(f"{base_url}/admin/dashboard", headers={"Authorization": f"Bearer {admin_token}"})
    assert res_admin_allowed.status_code == 200
    assert res_admin_allowed.json() == {"message": "Welcome Admin alice!"}

    # 7. APIKeyHeader authentication: Invalid key -> 401
    res_key_invalid = requests.get(f"{base_url}/api-key-protected", headers={"X-API-Key": "wrong-key"})
    assert res_key_invalid.status_code == 401

    # 8. APIKeyHeader authentication: Valid key -> 200
    res_key_valid = requests.get(f"{base_url}/api-key-protected", headers={"X-API-Key": "secret-api-key-999"})
    assert res_key_valid.status_code == 200
    assert res_key_valid.json() == {"status": "valid key"}
