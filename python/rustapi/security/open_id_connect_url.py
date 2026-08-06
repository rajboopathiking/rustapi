from typing import Any, Optional
from ..exceptions import HTTPException
from .base import SecurityBase


class OpenIdConnect(SecurityBase):
    """OpenID Connect authentication dependency helper."""

    def __init__(
        self,
        *,
        openIdConnectUrl: str,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(scheme_name=scheme_name)
        self.openIdConnectUrl = openIdConnectUrl
        self.description = description
        self.auto_error = auto_error

    def make_not_authenticated_error(self) -> HTTPException:
        return HTTPException(
            status_code=401,
            detail="Not authenticated",
            headers={"WWW-Authenticate": "Bearer"},
        )

    def __call__(self, request: Any = None, req: Any = None) -> Optional[str]:
        r = request or req
        if not r or not hasattr(r, "headers"):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        authorization = r.headers.get("authorization") or r.headers.get("Authorization")
        if not authorization:
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        return authorization
