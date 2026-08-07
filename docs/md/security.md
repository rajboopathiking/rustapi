# 🔒 Security & Authentication Guide in RustAPI (`pyrustapi`)

RustAPI provides high-performance C-extension security primitives powered by Rust alongside 100% FastAPI-compatible security schemes in `rustapi.security`.

---

## 📋 Overview of Security Features

| Feature | Primitive / Import | Description |
| :--- | :--- | :--- |
| **Native JWT Sign & Verify** | `rustapi.encode_jwt`, `rustapi.decode_jwt` | Rust-native `jsonwebtoken` engine with zero external Python dependencies (`PyJWT`/`jose`). |
| **Argon2 Password Hashing** | `rustapi.hash_password`, `rustapi.verify_password` | Native Rust Argon2 password hashing & verification. |
| **OAuth2 Password Flow** | `from rustapi.security import OAuth2PasswordBearer` | OAuth2 password bearer token extraction & Swagger UI login integration. |
| **HTTP Bearer Scheme** | `from rustapi.security import HTTPBearer` | Extract raw Bearer authorization credentials. |
| **HTTP Basic Auth** | `from rustapi.security import HTTPBasic` | Extract Base64-encoded `Basic username:password` credentials. |
| **API Key Authentication** | `APIKeyHeader`, `APIKeyQuery`, `APIKeyCookie` | Extract API Keys from headers (`X-API-Key`), query parameters (`?api_key=`), or cookies (`session_id`). |
| **OpenID Connect** | `from rustapi.security import OpenIdConnect` | OpenID Connect discovery URI security scheme. |
| **Swagger UI Integration** | `http://127.0.0.1:8000/docs` | Auto-registers `securitySchemes` in `/openapi.json` for interactive 🔓 **Authorize** button. |

---

## 1. Native Rust JWT Primitives

RustAPI embeds the Rust `jsonwebtoken` crate directly inside the core binary, providing zero-overhead token encoding and decoding.

```python
import rustapi

SECRET_KEY = "supersecret_jwt_key_change_in_production_32bytes"

# 1. Encode JWT Token
token = rustapi.encode_jwt(
    claims={"sub": "alice", "role": "admin"},
    secret=SECRET_KEY,
)

# 2. Decode JWT Token
try:
    payload = rustapi.decode_jwt(token, SECRET_KEY)
    print("Decoded claims:", payload)  # {'sub': 'alice', 'role': 'admin'}
except ValueError as e:
    print("Invalid or expired token:", e)
```

---

## 2. OAuth2 Password Flow & JWT Dependency

Combine `OAuth2PasswordBearer` with `rustapi.decode_jwt` to secure endpoints.

```python
import urllib.parse
from pydantic import BaseModel
import rustapi
from rustapi import Depends, HTTPException, PyRequest, status
from rustapi.security import OAuth2PasswordBearer

SECRET_KEY = "supersecret_jwt_key_change_in_production_32bytes"
app = rustapi.Engine()

oauth2_scheme = OAuth2PasswordBearer(
    tokenUrl="/auth/token", scheme_name="OAuth2Password"
)


class TokenResponse(BaseModel):
    access_token: str
    token_type: str


def get_current_user(token: str = Depends(oauth2_scheme)) -> dict:
    """Dependency that extracts and verifies the JWT token from the Authorization header."""
    try:
        payload = rustapi.decode_jwt(token, SECRET_KEY)
        return payload
    except Exception:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid or expired JWT token",
            headers={"WWW-Authenticate": "Bearer"},
        )


@app.post("/auth/token", response_model=TokenResponse)
def login(req: PyRequest):
    """
    Handles both JSON API logins and Swagger UI OAuth2 form logins.
    Swagger UI sends 'application/x-www-form-urlencoded' data.
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

    raise HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Invalid username or password",
    )


@app.get("/protected")
def protected_route(user: dict = Depends(get_current_user)):
    return {"message": f"Hello {user['sub']}!", "claims": user}
```

> [!TIP]
> **Swagger UI OAuth2 Modal Compatibility**: Swagger UI sends login credentials as `application/x-www-form-urlencoded`. Using `urllib.parse.parse_qs(req.body)` ensures your `/auth/token` endpoint works seamlessly in both Swagger UI `/docs` and JSON REST clients (`curl`, Postman).

---

## 3. Role-Based Authorization Guards

Create reusable dependency guards by chaining `Depends()`:

```python
def get_admin_user(current_user: dict = Depends(get_current_user)) -> dict:
    """Enforces admin role check on the current authenticated user."""
    if current_user.get("role") != "admin":
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Admin privileges required",
        )
    return current_user


@app.get("/admin/dashboard")
def admin_dashboard(admin: dict = Depends(get_admin_user)):
    return {"status": "success", "admin": admin["sub"]}
```

---

## 4. All Supported Security Schemes

RustAPI provides 100% FastAPI parity for all HTTP and API Key security dependencies:

### HTTP Bearer (`HTTPBearer`)
```python
from rustapi.security import HTTPBearer

bearer_scheme = HTTPBearer()

@app.get("/protected/bearer")
def protected_bearer(auth=Depends(bearer_scheme)):
    return {"scheme": auth.scheme, "credentials": auth.credentials}
```

### HTTP Basic Auth (`HTTPBasic`)
```python
from rustapi.security import HTTPBasic

basic_scheme = HTTPBasic()

@app.get("/protected/basic")
def protected_basic(auth=Depends(basic_scheme)):
    if auth.username == "admin" and auth.password == "secret":
        return {"user": auth.username}
    raise HTTPException(status_code=401, detail="Invalid Basic credentials")
```

### API Key in Header (`APIKeyHeader`)
```python
from rustapi.security import APIKeyHeader

api_key_header = APIKeyHeader(name="X-API-Key")

@app.get("/protected/header-key")
def protected_header(key: str = Depends(api_key_header)):
    if key != "secret-api-key":
        raise HTTPException(status_code=401, detail="Invalid API Key")
    return {"status": "valid"}
```

### API Key in Query (`APIKeyQuery`)
```python
from rustapi.security import APIKeyQuery

api_key_query = APIKeyQuery(name="api_key")

@app.get("/protected/query-key")
def protected_query(key: str = Depends(api_key_query)):
    return {"key": key}
```

### API Key in Cookie (`APIKeyCookie`)
```python
from rustapi.security import APIKeyCookie

api_key_cookie = APIKeyCookie(name="session_id")

@app.get("/protected/cookie-key")
def protected_cookie(key: str = Depends(api_key_cookie)):
    return {"session": key}
```

---

## 5. Interactive Swagger UI Integration (`/docs`)

When security schemes are registered via `Depends()`, RustAPI automatically generates the OpenAPI `securitySchemes` in `/openapi.json`.

This renders the interactive 🔓 **Authorize** button at the top of `/docs` as well as lock icons next to each protected endpoint.

### Reference Runnable Example:
See [`examples/security_jwt.py`](../../examples/security_jwt.py) for a complete working example covering all security schemes.
