import pytest
import requests
import threading
import time
from rustapi import FastAPI, Request, Depends, HTTPException, status
from rustapi.security import (
    HTTPBearer,
    OAuth2PasswordBearer,
    OAuth2AuthorizationCodeBearer,
    APIKeyHeader,
    APIKeyQuery,
    APIKeyCookie,
    HTTPBasic,
    HTTPDigest,
    OpenIdConnect,
)
from rustapi.openapi import get_openapi, get_swagger_ui_html

bearer_scheme = HTTPBearer(scheme_name="CustomBearer")
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="/auth/token", scheme_name="CustomOAuth2")
oauth2_code_scheme = OAuth2AuthorizationCodeBearer(
    authorizationUrl="/auth/authorize", tokenUrl="/auth/token", scheme_name="CustomOAuth2Code"
)
api_header_scheme = APIKeyHeader(name="X-API-Token", scheme_name="CustomHeaderKey")
api_query_scheme = APIKeyQuery(name="api_secret", scheme_name="CustomQueryKey")
api_cookie_scheme = APIKeyCookie(name="session_id", scheme_name="CustomCookieKey")
basic_scheme = HTTPBasic(scheme_name="CustomBasic")
digest_scheme = HTTPDigest(scheme_name="CustomDigest")
openid_scheme = OpenIdConnect(openIdConnectUrl="https://example.com/.well-known/openid-configuration", scheme_name="CustomOpenID")

app = FastAPI(title="Swagger UI Security Test Suite API", version="0.7.86")


@app.get("/sec/bearer")
def get_bearer(auth=Depends(bearer_scheme)):
    return {"scheme": "bearer"}


@app.get("/sec/oauth2")
def get_oauth2(auth=Depends(oauth2_scheme)):
    return {"scheme": "oauth2"}


@app.get("/sec/oauth2-code")
def get_oauth2_code(auth=Depends(oauth2_code_scheme)):
    return {"scheme": "oauth2-code"}


@app.get("/sec/header-key")
def get_header_key(auth=Depends(api_header_scheme)):
    return {"scheme": "header-key"}


@app.get("/sec/query-key")
def get_query_key(auth=Depends(api_query_scheme)):
    return {"scheme": "query-key"}


@app.get("/sec/cookie-key")
def get_cookie_key(auth=Depends(api_cookie_scheme)):
    return {"scheme": "cookie-key"}


@app.get("/sec/basic")
def get_basic(auth=Depends(basic_scheme)):
    return {"scheme": "basic"}


@app.get("/sec/digest")
def get_digest(auth=Depends(digest_scheme)):
    return {"scheme": "digest"}


@app.get("/sec/openid")
def get_openid_route(auth=Depends(openid_scheme)):
    return {"scheme": "openid"}


def test_swagger_ui_and_openapi_security_top_lock_button():
    port = 9015
    t = threading.Thread(target=lambda: app.run(host="127.0.0.1", port=port), daemon=True)
    t.start()
    time.sleep(1.5)

    base_url = f"http://127.0.0.1:{port}"

    # 1. Test Swagger UI (/docs) returns HTML with StandaloneLayout & Authorize lock button presets
    res_docs = requests.get(f"{base_url}/docs")
    assert res_docs.status_code == 200
    assert "SwaggerUIBundle" in res_docs.text
    assert "StandaloneLayout" in res_docs.text or "SwaggerUIStandalonePreset" in res_docs.text

    # 2. Test /openapi.json contains all securitySchemes for top Authorize lock button
    res_spec = requests.get(f"{base_url}/openapi.json")
    assert res_spec.status_code == 200
    spec = res_spec.json()

    assert "components" in spec
    assert "securitySchemes" in spec["components"]
    schemes = spec["components"]["securitySchemes"]

    assert "CustomBearer" in schemes
    assert schemes["CustomBearer"]["type"] == "http"
    assert schemes["CustomBearer"]["scheme"] == "bearer"

    assert "CustomOAuth2" in schemes
    assert schemes["CustomOAuth2"]["type"] == "oauth2"
    assert "password" in schemes["CustomOAuth2"]["flows"]

    assert "CustomOAuth2Code" in schemes
    assert schemes["CustomOAuth2Code"]["type"] == "oauth2"
    assert "authorizationCode" in schemes["CustomOAuth2Code"]["flows"]

    assert "CustomHeaderKey" in schemes
    assert schemes["CustomHeaderKey"]["type"] == "apiKey"
    assert schemes["CustomHeaderKey"]["in"] == "header"

    assert "CustomQueryKey" in schemes
    assert schemes["CustomQueryKey"]["type"] == "apiKey"
    assert schemes["CustomQueryKey"]["in"] == "query"

    assert "CustomCookieKey" in schemes
    assert schemes["CustomCookieKey"]["type"] == "apiKey"
    assert schemes["CustomCookieKey"]["in"] == "cookie"

    assert "CustomBasic" in schemes
    assert schemes["CustomBasic"]["type"] == "http"
    assert schemes["CustomBasic"]["scheme"] == "basic"

    assert "CustomDigest" in schemes
    assert schemes["CustomDigest"]["type"] == "http"
    assert schemes["CustomDigest"]["scheme"] == "digest"

    assert "CustomOpenID" in schemes
    assert schemes["CustomOpenID"]["type"] == "openIdConnect"

    # 3. Test individual path operations contain 'security' requirements
    paths = spec["paths"]
    assert "security" in paths["/sec/bearer"]["get"]
    assert paths["/sec/bearer"]["get"]["security"] == [{"CustomBearer": []}]

    assert "security" in paths["/sec/header-key"]["get"]
    assert paths["/sec/header-key"]["get"]["security"] == [{"CustomHeaderKey": []}]


def test_python_get_openapi_security_generation():
    py_spec = get_openapi(
        title="Python OpenAPI Security Test",
        version="0.7.86",
        routes=app.routes,
    )
    assert "components" in py_spec
    assert "securitySchemes" in py_spec["components"]
    schemes = py_spec["components"]["securitySchemes"]
    assert "CustomBearer" in schemes
    assert "CustomOAuth2" in schemes
    assert "CustomHeaderKey" in schemes

    html_resp = get_swagger_ui_html(openapi_url="/openapi.json", title="Docs")
    content = html_resp.content if isinstance(html_resp.content, str) else str(html_resp.content)
    assert "SwaggerUIBundle" in content
