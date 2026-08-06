from typing import Any, Dict, List, Optional
from ..exceptions import HTTPException
from .base import SecurityBase
from .utils import get_authorization_scheme_param


class SecurityScopes:
    """Class to manage and validate required security scopes for OAuth2 dependencies."""

    def __init__(self, scopes: Optional[List[str]] = None, scope_str: str = ""):
        self.scopes = scopes or []
        self.scope_str = scope_str or " ".join(self.scopes)


class OAuth2PasswordRequestForm:
    """OAuth2 password flow request form parser."""

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
        self.scopes = scope.split() if scope else []
        self.client_id = client_id
        self.client_secret = client_secret


class OAuth2PasswordRequestFormStrict(OAuth2PasswordRequestForm):
    """Strict OAuth2 password form requiring grant_type='password'."""

    def __init__(
        self,
        grant_type: str = "password",
        username: str = "",
        password: str = "",
        scope: str = "",
        client_id: Optional[str] = None,
        client_secret: Optional[str] = None,
    ):
        if grant_type != "password":
            raise HTTPException(
                status_code=400,
                detail="Incorrect grant_type, expected 'password'",
            )
        super().__init__(
            grant_type=grant_type,
            username=username,
            password=password,
            scope=scope,
            client_id=client_id,
            client_secret=client_secret,
        )


class OAuth2(SecurityBase):
    """OAuth2 base dependency class."""

    def __init__(
        self,
        *,
        flows: Optional[Any] = None,
        scheme_name: Optional[str] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(scheme_name=scheme_name)
        self.flows = flows
        self.description = description
        self.auto_error = auto_error


class OAuth2PasswordBearer(OAuth2):
    """OAuth2 password flow with bearer token dependency helper."""

    def __init__(
        self,
        tokenUrl: str,
        scheme_name: Optional[str] = None,
        scopes: Optional[Dict[str, str]] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(
            scheme_name=scheme_name,
            description=description,
            auto_error=auto_error,
        )
        self.tokenUrl = tokenUrl
        self.scopes = scopes or {}

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
        scheme, param = get_authorization_scheme_param(authorization)
        if not (authorization and scheme and param):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        if scheme.lower() != "bearer":
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        return param


class OAuth2AuthorizationCodeBearer(OAuth2):
    """OAuth2 authorization code flow with bearer token dependency helper."""

    def __init__(
        self,
        authorizationUrl: str,
        tokenUrl: str,
        scheme_name: Optional[str] = None,
        scopes: Optional[Dict[str, str]] = None,
        description: Optional[str] = None,
        auto_error: bool = True,
    ):
        super().__init__(
            scheme_name=scheme_name,
            description=description,
            auto_error=auto_error,
        )
        self.authorizationUrl = authorizationUrl
        self.tokenUrl = tokenUrl
        self.scopes = scopes or {}

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
        scheme, param = get_authorization_scheme_param(authorization)
        if not (authorization and scheme and param):
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        if scheme.lower() != "bearer":
            if self.auto_error:
                raise self.make_not_authenticated_error()
            return None

        return param
