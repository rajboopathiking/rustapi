def test_security_imports():
    from rustapi.security import (
        SecurityBase,
        APIKeyBase,
        APIKeyQuery,
        APIKeyHeader,
        APIKeyCookie,
        HTTPBase,
        HTTPBasic,
        HTTPBearer,
        HTTPDigest,
        HTTPBasicCredentials,
        HTTPAuthorizationCredentials,
        OAuth2,
        OAuth2PasswordBearer,
        OAuth2AuthorizationCodeBearer,
        OAuth2PasswordRequestForm,
        OAuth2PasswordRequestFormStrict,
        SecurityScopes,
        OpenIdConnect,
        get_authorization_scheme_param,
    )

    assert SecurityBase is not None
    assert APIKeyQuery is not None
    assert HTTPBearer is not None
    assert OAuth2PasswordBearer is not None
    assert SecurityScopes is not None
    assert OpenIdConnect is not None

    scheme, param = get_authorization_scheme_param("Bearer token123")
    assert scheme == "Bearer"
    assert param == "token123"


def test_openapi_imports():
    from rustapi.openapi import (
        get_swagger_ui_html,
        get_redoc_html,
        get_swagger_ui_oauth2_redirect_html,
        swagger_ui_default_parameters,
        get_openapi,
        models,
        utils,
    )

    assert get_swagger_ui_html is not None
    assert models.OpenAPI is not None
    assert models.Info is not None

    schema = get_openapi(title="Test API", version="1.0.0")
    assert schema["info"]["title"] == "Test API"
    assert schema["openapi"] == "3.1.0"
