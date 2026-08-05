from typing import Optional, Dict, Any
from ..exceptions import HTTPException


class HTTPAuthorizationCredentials:
    """FastAPI-compatible HTTP Authorization credentials container."""

    def __init__(self, scheme: str, credentials: str):
        self.scheme = scheme
        self.credentials = credentials


class HTTPBasic:
    """HTTP Basic authentication dependency helper."""

    def __init__(self, *, realm: Optional[str] = None, auto_error: bool = True):
        self.realm = realm
        self.auto_error = auto_error


class HTTPBearer:
    """HTTP Bearer authentication dependency helper."""

    def __init__(self, *, bearerFormat: Optional[str] = None, auto_error: bool = True):
        self.bearerFormat = bearerFormat
        self.auto_error = auto_error

    def __call__(self, request: Any = None, req: Any = None) -> Optional[HTTPAuthorizationCredentials]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        auth_header = r.headers.get("authorization") or r.headers.get("Authorization")
        if not auth_header:
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        scheme, _, credentials = auth_header.partition(" ")
        if scheme.lower() != "bearer" or not credentials:
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Invalid authentication scheme")
            return None

        return HTTPAuthorizationCredentials(scheme="Bearer", credentials=credentials)


class OAuth2PasswordBearer:
    """OAuth2 password flow with bearer token dependency helper."""

    def __init__(
        self,
        tokenUrl: str,
        scheme_name: Optional[str] = None,
        scopes: Optional[Dict[str, str]] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        self.tokenUrl = tokenUrl
        self.scheme_name = scheme_name
        self.scopes = scopes or {}
        self.description = description
        self.auto_error = auto_error

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        auth_header = r.headers.get("authorization") or r.headers.get("Authorization")
        if not auth_header:
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        scheme, _, token = auth_header.partition(" ")
        if scheme.lower() != "bearer" or not token:
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        return token


class OAuth2PasswordRequestForm:
    """OAuth2 password request form for token extraction."""

    def __init__(
        self,
        grant_type: str = "password",
        username: str = "",
        password: str = "",
        scope: str = "",
        client_id: Optional[str] = None,
        client_secret: Optional[str] = None,
    ):
        self.grant_type = grant_type
        self.username = username
        self.password = password
        self.scopes = scope.split()
        self.client_id = client_id
        self.client_secret = client_secret


class APIKeyHeader:
    """API Key authentication header dependency helper."""

    def __init__(self, *, name: str, auto_error: bool = True):
        self.name = name
        self.auto_error = auto_error

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        key = r.headers.get(self.name) or r.headers.get(self.name.lower())
        if not key and self.auto_error:
            raise HTTPException(status_code=401, detail="Not authenticated")
        return key


class APIKeyQuery:
    """API Key authentication query parameter dependency helper."""

    def __init__(self, *, name: str, auto_error: bool = True):
        self.name = name
        self.auto_error = auto_error

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "query_params"):
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        key = r.query_params.get(self.name)
        if not key and self.auto_error:
            raise HTTPException(status_code=401, detail="Not authenticated")
        return key


class APIKeyCookie:
    """API Key authentication cookie dependency helper."""

    def __init__(self, *, name: str, auto_error: bool = True):
        self.name = name
        self.auto_error = auto_error

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "cookies"):
            if self.auto_error:
                raise HTTPException(status_code=401, detail="Not authenticated")
            return None

        key = r.cookies.get(self.name)
        if not key and self.auto_error:
            raise HTTPException(status_code=401, detail="Not authenticated")
        return key


__all__ = [
    "HTTPAuthorizationCredentials",
    "HTTPBasic",
    "HTTPBearer",
    "OAuth2PasswordBearer",
    "OAuth2PasswordRequestForm",
    "APIKeyHeader",
    "APIKeyQuery",
    "APIKeyCookie",
]
