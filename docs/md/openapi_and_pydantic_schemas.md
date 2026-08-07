# 🛠 OpenAPI 3.0 & Pydantic Schema Generation Guide

**RustAPI** provides automatic, 100% FastAPI-compatible OpenAPI 3.0.0 specification generation served at `/openapi.json` and rendered in Swagger UI at `/docs`.

---

## 🚀 Highlights & Features

- **Pydantic `$ref` Schemas**: Automatic JSON schema extraction into `components["schemas"]` for `response_model` and request payload parameters.
- **Nested Model `$defs` Hoisting**: Hoists `$defs` definitions from Pydantic v2 schemas into top-level `components["schemas"]` and rewrites `$ref` paths.
- **Validation Error Schemas**: Includes `HTTPValidationError` and `ValidationError` schemas under `components["schemas"]` with `422 Unprocessable Entity` response references.
- **Interactive File Uploads**: Single (`UploadFile = File(...)`) and multi-file (`List[UploadFile] = File(...)`) parameters render interactive **Choose File** / **Choose Files** buttons in Swagger UI.
- **Dependency Isolation**: Dependency objects (`token: str = Depends(bearer)`) are excluded from query parameter generation and do not block JSON serialization.
- **Security Schemes**: Automatically detects `HTTPBearer`, `OAuth2PasswordBearer`, `APIKeyHeader`, `APIKeyQuery`, `APIKeyCookie`, `HTTPBasic`, `HTTPDigest`, and `OpenIdConnect` dependencies and generates top-level `securitySchemes` and operation locks.

---

## 📖 Usage Examples

### 1. Pydantic Request & Response Models

```python
from pydantic import BaseModel
from rustapi import FastAPI

app = FastAPI(title="Product Catalog API")

class ProductIn(BaseModel):
    name: str
    price: float

class ProductOut(BaseModel):
    id: int
    name: str
    price: float

@app.post("/products", response_model=ProductOut, status_code=201)
def create_product(product: ProductIn):
    return ProductOut(id=1, name=product.name, price=product.price)
```

**Generated OpenAPI Output (`/openapi.json`)**:

```json
{
  "openapi": "3.0.0",
  "info": { "title": "Product Catalog API", "version": "0.1.0" },
  "paths": {
    "/products": {
      "post": {
        "summary": "Create Product",
        "operationId": "create_product_products_post",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": { "$ref": "#/components/schemas/ProductIn" }
            }
          }
        },
        "responses": {
          "200": {
            "description": "Successful Response",
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/ProductOut" }
              }
            }
          },
          "422": {
            "description": "Validation Error",
            "content": {
              "application/json": {
                "schema": { "$ref": "#/components/schemas/HTTPValidationError" }
              }
            }
          }
        }
      }
    }
  },
  "components": {
    "schemas": {
      "ProductIn": {
        "title": "ProductIn",
        "type": "object",
        "properties": {
          "name": { "title": "Name", "type": "string" },
          "price": { "title": "Price", "type": "number" }
        },
        "required": ["name", "price"]
      },
      "ProductOut": {
        "title": "ProductOut",
        "type": "object",
        "properties": {
          "id": { "title": "Id", "type": "integer" },
          "name": { "title": "Name", "type": "string" },
          "price": { "title": "Price", "type": "number" }
        },
        "required": ["id", "name", "price"]
      },
      "ValidationError": { ... },
      "HTTPValidationError": { ... }
    }
  }
}
```

---

### 2. Single & Multi-File Upload Schemas

```python
from typing import List
from rustapi import FastAPI, File, Form, UploadFile

app = FastAPI(title="Upload Portal")

@app.post("/upload")
def upload_single(file: UploadFile = File(...), tag: str = Form("general")):
    return {"filename": file.filename, "tag": tag}

@app.post("/upload-batch")
def upload_multi(files: List[UploadFile] = File(...)):
    return {"count": len(files)}
```

---

### 3. Security Schemes & Authorize Button

```python
from rustapi import FastAPI, Depends
from rustapi.security import HTTPBearer, HTTPAuthorizationCredentials

app = FastAPI(title="Secure API")
bearer = HTTPBearer()

@app.get("/secret")
def get_secret(credentials: HTTPAuthorizationCredentials = Depends(bearer)):
    return {"secret": 42}
```

Swagger UI automatically displays the 🔓 **Authorize** button at top-right allowing interactive token entry during API testing.

---

## 🛠 Standalone OpenAPI Utility

For non-FastAPI apps or custom schema generation, use `rustapi.openapi.utils.get_openapi()`:

```python
from rustapi.openapi.utils import get_openapi

spec = get_openapi(
    title="Custom Service",
    version="1.0.0",
    description="Standalone OpenAPI spec generator",
)
```
