import base64
from typing import Any, Optional
from pydantic import BaseModel
from ..exceptions import HTTPException
from .base import SecurityBase
from .utils import get_authorization_scheme_param


class HTTPBasicCredentials(BaseModel):
    """Container for HTTP Basic authentication credentials."""

    username: str
    password: str


class HTTPAuthorizationCredentials(BaseModel):
    """Container for HTTP Authorization credentials (Bearer, Digest, etc.)."""

    scheme: str
    credentials: str


class HTTPBase(SecurityBase):
    """Base class for HTTP authentication schemes."""

    def __init__(
        self,
        *,
        scheme: str,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(scheme_name=scheme_name)
        self.scheme = scheme
        self.description = description
        self.auto_error = auto_error

    def make_authenticate_headers(self) -> dict[str, str]:
        return {"WWW-Authenticate": self.scheme.title()}

    def make_not_authenticated_error(self) -> HTTPException:
        return HTTPException(
            status_code=401,
            detail="Not authenticated",
            headers=self.make_authenticate_headers(),
        )

    def __call__(self, request: Any = None, req: Any = None) -> Optional[HTTPAuthorizationCredentials]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        authorization = r.headers.get("authorization") or r.headers.get("Authorization")
        scheme, credentials = get_authorization_scheme_param(authorization)
        if not (authorization and scheme and credentials):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        if scheme.lower() != self.scheme.lower():
            if self.auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Invalid authentication scheme",
                    headers=self.make_authenticate_headers(),
                )
            return None

        return HTTPAuthorizationCredentials(scheme=scheme, credentials=credentials)


class HTTPBasic(HTTPBase):
    """HTTP Basic authentication dependency helper."""

    def __init__(
        self,
        *,
        realm: Optional[str] = None,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(
            scheme="basic",
            scheme_name=scheme_name,
            description=description,
            auto_error=auto_error,
        )
        self.realm = realm

    def make_authenticate_headers(self) -> dict[str, str]:
        realm = f' realm="{self.realm}"' if self.realm else ""
        return {"WWW-Authenticate": f"Basic{realm}"}

    def __call__(self, request: Any = None, req: Any = None) -> Optional[HTTPBasicCredentials]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        authorization = r.headers.get("authorization") or r.headers.get("Authorization")
        scheme, param = get_authorization_scheme_param(authorization)
        if not (authorization and scheme and param):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        if scheme.lower() != "basic":
            if self.auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Invalid authentication scheme",
                    headers=self.make_authenticate_headers(),
                )
            return None

        try:
            decoded = base64.b64decode(param).decode("utf-8")
            username, _, password = decoded.partition(":")
        except Exception:
            if self.auto_error:
                raise HTTPException(
                    status_code=401,
                    detail="Invalid basic authentication credentials",
                    headers=self.make_authenticate_headers(),
                )
            return None

        return HTTPBasicCredentials(username=username, password=password)


class HTTPBearer(HTTPBase):
    """HTTP Bearer token authentication dependency helper."""

    def __init__(
        self,
        *,
        bearerFormat: Optional[str] = None,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(
            scheme="bearer",
            scheme_name=scheme_name,
            description=description,
            auto_error=auto_error,
        )
        self.bearerFormat = bearerFormat


class HTTPDigest(HTTPBase):
    """HTTP Digest authentication dependency helper."""

    def __init__(
        self,
        *,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(
            scheme="digest",
            scheme_name=scheme_name,
            description=description,
            auto_error=auto_error,
        )
