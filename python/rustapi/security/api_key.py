from typing import Any, Optional
from ..exceptions import HTTPException
from .base import SecurityBase


class APIKeyBase(SecurityBase):
    """Base class for API Key authentication schemes."""

    def __init__(
        self,
        *,
        name: str,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(scheme_name=scheme_name)
        self.name = name
        self.description = description
        self.auto_error = auto_error

    def make_not_authenticated_error(self) -> HTTPException:
        return HTTPException(
            status_code=401,
            detail="Not authenticated",
            headers={"WWW-Authenticate": "APIKey"},
        )

    def check_api_key(self, api_key: Optional[str]) -> Optional[str]:
        if not api_key:
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None
        return api_key


class APIKeyQuery(APIKeyBase):
    """API Key authentication using a query parameter."""

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "query_params"):
            return self.check_api_key(None)

        key = r.query_params.get(self.name)
        return self.check_api_key(key)


class APIKeyHeader(APIKeyBase):
    """API Key authentication using an HTTP header."""

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            return self.check_api_key(None)

        key = r.headers.get(self.name) or r.headers.get(self.name.lower())
        return self.check_api_key(key)


class APIKeyCookie(APIKeyBase):
    """API Key authentication using a cookie."""

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "cookies"):
            return self.check_api_key(None)

        key = r.cookies.get(self.name)
        return self.check_api_key(key)
