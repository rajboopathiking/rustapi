from typing import Optional, Dict, Any
from ..exceptions import HTTPException


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


class APIKeyQuery:
    """API Key authentication query parameter dependency helper."""

    def __init__(self, *, name: str, auto_error: bool = True):
        self.name = name
        self.auto_error = auto_error


class APIKeyCookie:
    """API Key authentication cookie dependency helper."""

    def __init__(self, *, name: str, auto_error: bool = True):
        self.name = name
        self.auto_error = auto_error
