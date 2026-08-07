from typing import List, Optional
from pydantic import BaseModel
import rustapi
from rustapi import FastAPI, APIRouter, File, Form, UploadFile, Depends
from rustapi.security import HTTPBearer, APIKeyHeader
from rustapi.openapi.utils import get_openapi


class UserIn(BaseModel):
    username: str
    email: str


class UserOut(BaseModel):
    id: int
    username: str
    email: str


class Address(BaseModel):
    street: str
    city: str


class Company(BaseModel):
    name: str
    location: Address


def test_openapi_pydantic_request_and_response_schemas():
    app = FastAPI(title="Pydantic Schema Test")

    @app.post("/users", response_model=UserOut, tags=["users"])
    def create_user(user: UserIn):
        return UserOut(id=1, username=user.username, email=user.email)

    spec = app.openapi()

    assert "UserIn" in spec["components"]["schemas"]
    assert "UserOut" in spec["components"]["schemas"]

    # Verify requestBody $ref
    req_body_schema = spec["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"]
    assert req_body_schema == {"$ref": "#/components/schemas/UserIn"}

    # Verify 200 response $ref
    resp_200_schema = spec["paths"]["/users"]["post"]["responses"]["200"]["content"]["application/json"]["schema"]
    assert resp_200_schema == {"$ref": "#/components/schemas/UserOut"}


def test_openapi_nested_pydantic_models():
    app = FastAPI(title="Nested Models Test")

    @app.post("/company", response_model=Company)
    def create_company(company: Company):
        return company

    spec = app.openapi()

    assert "Company" in spec["components"]["schemas"]
    assert "Address" in spec["components"]["schemas"]


def test_openapi_http_validation_error_schemas():
    app = FastAPI(title="Validation Error Test")

    @app.get("/items/{item_id}")
    def get_item(item_id: int):
        return {"item_id": item_id}

    spec = app.openapi()

    assert "ValidationError" in spec["components"]["schemas"]
    assert "HTTPValidationError" in spec["components"]["schemas"]

    resp_422_schema = spec["paths"]["/items/{item_id}"]["get"]["responses"]["422"]["content"]["application/json"]["schema"]
    assert resp_422_schema == {"$ref": "#/components/schemas/HTTPValidationError"}


def test_openapi_single_and_multi_upload_file_schemas():
    app = FastAPI(title="File Upload OpenAPI Test")

    @app.post("/upload")
    def upload_single(file: UploadFile = File(...), note: str = Form("")):
        return {"filename": file.filename, "note": note}

    @app.post("/upload-multi")
    def upload_multi(files: List[UploadFile] = File(...)):
        return {"count": len(files)}

    spec = app.openapi()

    # Single Upload Schema
    single_schema = spec["paths"]["/upload"]["post"]["requestBody"]["content"]["multipart/form-data"]["schema"]
    assert single_schema["properties"]["file"] == {"type": "string", "format": "binary"}
    assert single_schema["properties"]["note"] == {"type": "string"}
    assert "file" in single_schema["required"]

    # Multi Upload Schema
    multi_schema = spec["paths"]["/upload-multi"]["post"]["requestBody"]["content"]["multipart/form-data"]["schema"]
    assert multi_schema["properties"]["files"] == {
        "type": "array",
        "items": {"type": "string", "format": "binary"},
    }
    assert "files" in multi_schema["required"]


def test_openapi_parameter_defaults_and_security_filtering():
    app = FastAPI(title="Security & Parameter Test")
    bearer = HTTPBearer()

    @app.get("/search")
    def search(q: str = "default_query", limit: int = 10, token: str = Depends(bearer)):
        return {"q": q, "limit": limit}

    spec = app.openapi()

    params = spec["paths"]["/search"]["get"]["parameters"]
    param_map = {p["name"]: p for p in params}

    assert "q" in param_map
    assert param_map["q"]["schema"]["default"] == "default_query"
    assert param_map["limit"]["schema"]["default"] == 10

    # Ensure Depends parameter is NOT put in query parameters
    assert "token" not in param_map

    # Ensure HTTPBearer security scheme is generated
    assert "securitySchemes" in spec["components"]


def test_openapi_multiple_security_schemes():
    app = FastAPI(title="Multi-Security Test")
    bearer = HTTPBearer()
    api_key = APIKeyHeader(name="X-API-Key")

    @app.get("/protected")
    def protected(auth: str = Depends(bearer), key: str = Depends(api_key)):
        return {"ok": True}

    spec = app.openapi()

    sec_schemes = spec["components"]["securitySchemes"]
    assert "HTTPBearer" in sec_schemes or any(v.get("scheme") == "bearer" for v in sec_schemes.values())
    assert "APIKeyHeader" in sec_schemes or any(v.get("in") == "header" for v in sec_schemes.values())

    op = spec["paths"]["/protected"]["get"]
    assert "security" in op


def test_openapi_path_parameter_types():
    app = FastAPI(title="Path Param Types Test")

    @app.get("/users/{user_id}/score/{score}/active/{active}")
    def get_user_score(user_id: int, score: float, active: bool, name: str):
        return {"user_id": user_id, "score": score, "active": active, "name": name}

    spec = app.openapi()

    params = spec["paths"]["/users/{user_id}/score/{score}/active/{active}"]["get"]["parameters"]
    param_map = {p["name"]: p for p in params}

    assert param_map["user_id"]["schema"]["type"] == "integer"
    assert param_map["score"]["schema"]["type"] == "number"
    assert param_map["active"]["schema"]["type"] == "boolean"
    assert param_map["name"]["schema"]["type"] == "string"
    assert param_map["name"]["in"] == "query"


def test_openapi_custom_responses_and_metadata():
    app = FastAPI(title="Custom Metadata Test")

    @app.get("/custom", summary="Custom Summary", description="Custom docstring", deprecated=True)
    def custom_endpoint():
        """Will be overridden by description kwarg"""
        return {}

    spec = app.openapi()

    op = spec["paths"]["/custom"]["get"]
    assert op["summary"] == "Custom Summary"
    assert op["description"] == "Custom docstring"
    assert op["deprecated"] is True


def test_openapi_nested_apirouters():
    app = FastAPI(title="Nested APIRouter Test")

    root_router = APIRouter(prefix="/api")
    sub_router = APIRouter(prefix="/v2", tags=["v2"])

    @sub_router.get("/items")
    def get_v2_items():
        return []

    root_router.include_router(sub_router)
    app.include_router(root_router)

    spec = app.openapi()

    assert "/api/v2/items" in spec["paths"]
    assert spec["paths"]["/api/v2/items"]["get"]["tags"] == ["v2"]


def test_openapi_app_metadata_customization():
    app = FastAPI(
        title="Custom App Title",
        version="2.5.0",
        description="Detailed App Description",
        servers=[{"url": "https://api.example.com", "description": "Production Server"}],
        terms_of_service="https://example.com/terms",
        contact={"name": "API Support", "email": "support@example.com"},
        license_info={"name": "MIT License", "url": "https://opensource.org/licenses/MIT"},
    )

    @app.get("/health")
    def health():
        return {"status": "healthy"}

    spec = app.openapi()

    assert spec["info"]["title"] == "Custom App Title"
    assert spec["info"]["version"] == "2.5.0"
    assert spec["info"]["description"] == "Detailed App Description"
    assert spec["info"]["termsOfService"] == "https://example.com/terms"
    assert spec["info"]["contact"]["email"] == "support@example.com"
    assert spec["info"]["license"]["name"] == "MIT License"
    assert spec["servers"][0]["url"] == "https://api.example.com"


def test_standalone_get_openapi_utility():
    spec = get_openapi(
        title="Standalone Spec Test",
        version="1.0.0",
        description="Testing standalone utility",
    )

    assert spec["info"]["title"] == "Standalone Spec Test"
    assert spec["info"]["version"] == "1.0.0"
    assert "ValidationError" in spec["components"]["schemas"]
    assert "HTTPValidationError" in spec["components"]["schemas"]
