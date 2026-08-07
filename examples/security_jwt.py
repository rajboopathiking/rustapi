"""
RustAPI Complete Security & Authentication Example.

This example demonstrates all supported security schemes in RustAPI:
1. OAuth2 Password Flow & JWT (`OAuth2PasswordBearer`, `rustapi.encode_jwt`, `rustapi.decode_jwt`).
2. HTTP Bearer Token (`HTTPBearer`).
3. HTTP Basic Auth (`HTTPBasic`).
4. API Key Security (`APIKeyHeader`, `APIKeyQuery`, `APIKeyCookie`).
5. Role-based Authorization (Admin route guard).
6. Complete Swagger UI (`/docs`) Authorize lock button integration.

Run the application:
    python examples/security_jwt.py

Visit Swagger UI Docs in your browser:
    http://127.0.0.1:8000/docs
"""

import urllib.parse
from pydantic import BaseModel
import rustapi
from rustapi import Depends, HTTPException, PyRequest, status
from rustapi.security import (
    APIKeyCookie,
    APIKeyHeader,
    APIKeyQuery,
    HTTPAuthorizationCredentials,
    HTTPBasic,
    HTTPBasicCredentials,
    HTTPBearer,
    OAuth2PasswordBearer,
)

SECRET_KEY = "supersecret_jwt_key_change_in_production_32bytes"

app = rustapi.Engine()

# ---- Security Scheme Definitions ----
oauth2_scheme = OAuth2PasswordBearer(
    tokenUrl="/auth/token", scheme_name="OAuth2Password"
)
bearer_scheme = HTTPBearer(scheme_name="HTTPBearer")
basic_scheme = HTTPBasic(scheme_name="HTTPBasic")
api_key_header = APIKeyHeader(name="X-API-Key", scheme_name="APIKeyHeader")
api_key_query = APIKeyQuery(name="api_key", scheme_name="APIKeyQuery")
api_key_cookie = APIKeyCookie(name="session_id", scheme_name="APIKeyCookie")


# ---- Pydantic Models ----
class TokenResponse(BaseModel):
    access_token: str
    token_type: str


# ---- Security Dependency Guards ----
def get_current_user(token: str = Depends(oauth2_scheme)) -> dict:
    """Extract and decode JWT token supplied via OAuth2 / Bearer Authorization header."""
    try:
        payload = rustapi.decode_jwt(token, SECRET_KEY)
        return payload
    except Exception:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid or expired JWT token",
            headers={"WWW-Authenticate": "Bearer"},
        )


def get_admin_user(current_user: dict = Depends(get_current_user)) -> dict:
    """Authorization guard: ensure current user has 'admin' role."""
    if current_user.get("role") != "admin":
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Admin privileges required",
        )
    return current_user


# ---- Authentication Endpoint ----
@app.post(
    "/auth/token",
    response_model=TokenResponse,
    summary="OAuth2 Password Flow Login",
    tags=["Authentication"],
)
def login(req: PyRequest):
    """
    Authenticate user credentials and issue a signed JWT access token.
    Supports both JSON payload and x-www-form-urlencoded data (Swagger UI OAuth2 modal).
    """
    content_type = req.headers.get("content-type", "")
    if "application/x-www-form-urlencoded" in content_type:
        parsed = urllib.parse.parse_qs(req.body)
        username = parsed.get("username", [""])[0]
        password = parsed.get("password", [""])[0]
    else:
        try:
            body = req.json() if req.body else {}
            username = body.get("username", "")
            password = body.get("password", "")
        except Exception:
            username, password = "", ""

    if username == "alice" and password == "secretpassword":
        token = rustapi.encode_jwt(
            claims={"sub": username, "role": "admin"},
            secret=SECRET_KEY,
        )
        return {"access_token": token, "token_type": "bearer"}
    elif username == "bob" and password == "userpassword":
        token = rustapi.encode_jwt(
            claims={"sub": username, "role": "user"},
            secret=SECRET_KEY,
        )
        return {"access_token": token, "token_type": "bearer"}

    raise HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Invalid username or password",
    )


# ---- 1. OAuth2 / JWT Protected Endpoint ----
@app.get(
    "/protected/oauth2",
    summary="OAuth2 JWT Protected Endpoint",
    tags=["OAuth2 & JWT Security"],
)
def protected_oauth2(user: dict = Depends(get_current_user)):
    """Secured via OAuth2PasswordBearer dependency."""
    return {
        "message": f"Hello {user['sub']}! OAuth2 JWT authentication succeeded.",
        "user_claims": user,
    }


# ---- 2. Role-Based Admin Endpoint ----
@app.get(
    "/protected/admin",
    summary="Admin-Only Protected Endpoint",
    tags=["OAuth2 & JWT Security"],
)
def protected_admin(admin: dict = Depends(get_admin_user)):
    """Secured via OAuth2 JWT + Admin Role Guard."""
    return {
        "message": f"Welcome Admin {admin['sub']}! You have full administrative access.",
        "admin_claims": admin,
    }


# ---- 3. HTTP Bearer Scheme Endpoint ----
@app.get(
    "/protected/bearer",
    summary="HTTP Bearer Protected Endpoint",
    tags=["HTTP Security"],
)
def protected_bearer(auth=Depends(bearer_scheme)):
    """Secured via raw HTTPBearer scheme."""
    return {
        "scheme": auth.scheme,
        "token_preview": auth.credentials[:10] + "...",
    }


# ---- 4. HTTP Basic Auth Endpoint ----
@app.get(
    "/protected/basic",
    summary="HTTP Basic Auth Protected Endpoint",
    tags=["HTTP Security"],
)
def protected_basic(auth=Depends(basic_scheme)):
    """Secured via HTTPBasic scheme."""
    if auth.username == "admin" and auth.password == "secret":
        return {
            "message": f"Basic authentication successful for {auth.username}"
        }
    raise HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Invalid Basic Auth credentials",
        headers={"WWW-Authenticate": "Basic"},
    )


# ---- 5. API Key in Header Endpoint ----
@app.get(
    "/protected/api-key-header",
    summary="API Key Header Protected Endpoint",
    tags=["API Key Security"],
)
def protected_api_key_header(key: str = Depends(api_key_header)):
    """Secured via X-API-Key HTTP Header."""
    if key != "secret-header-key-999":
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid API Header Key",
        )
    return {"message": "Valid API Header Key provided", "api_key": key}


# ---- 6. API Key in Query Endpoint ----
@app.get(
    "/protected/api-key-query",
    summary="API Key Query Protected Endpoint",
    tags=["API Key Security"],
)
def protected_api_key_query(key: str = Depends(api_key_query)):
    """Secured via ?api_key= query parameter."""
    if key != "secret-query-key-888":
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid API Query Key",
        )
    return {"message": "Valid API Query Key provided", "api_key": key}


# ---- 7. API Key in Cookie Endpoint ----
@app.get(
    "/protected/api-key-cookie",
    summary="API Key Cookie Protected Endpoint",
    tags=["API Key Security"],
)
def protected_api_key_cookie(key: str = Depends(api_key_cookie)):
    """Secured via session_id cookie."""
    if key != "secret-session-cookie-777":
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid Session Cookie",
        )
    return {"message": "Valid Session Cookie provided", "cookie": key}


if __name__ == "__main__":
    print("Starting RustAPI Security Server on http://127.0.0.1:8000")
    print(
        "Interactive Swagger UI Docs available at http://127.0.0.1:8000/docs"
    )
    app.run(host="127.0.0.1", port=8000, reload=True)
