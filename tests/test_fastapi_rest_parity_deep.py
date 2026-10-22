import asyncio
import json
from typing import List, Optional
import pytest
from pydantic import BaseModel, Field
from rustapi import FastAPI, APIRouter, Depends, HTTPException, BackgroundTasks, status


class UserSchema(BaseModel):
    id: int
    username: str
    email: str
    is_active: bool = True
    hashed_password: str = "secret"


class UserResponse(BaseModel):
    id: int
    username: str
    email: str


@pytest.mark.asyncio
async def test_rest_nested_response_model_filtering():
    app = FastAPI()

    @app.get("/users", response_model=List[UserResponse])
    async def get_users():
        return [
            {"id": 1, "username": "alice", "email": "alice@example.com", "hashed_password": "p1"},
            {"id": 2, "username": "bob", "email": "bob@example.com", "hashed_password": "p2"},
        ]

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/users",
        "headers": [],
        "query_string": b"",
    }
    messages = []
    await app(scope, lambda: asyncio.sleep(0, result={"type": "http.request", "body": b""}), lambda m: messages.append(m) or asyncio.sleep(0))

    assert messages[0]["status"] == 200
    data = json.loads(messages[1]["body"].decode("utf-8"))
    assert len(data) == 2
    assert data[0] == {"id": 1, "username": "alice", "email": "alice@example.com"}
    assert "hashed_password" not in data[0]


@pytest.mark.asyncio
async def test_status_code_204_no_content():
    app = FastAPI()

    @app.delete("/items/{item_id}", status_code=204)
    def delete_item(item_id: int):
        return None

    scope = {
        "type": "http",
        "method": "DELETE",
        "path": "/items/42",
        "headers": [],
        "query_string": b"",
    }
    messages = []
    await app(scope, lambda: asyncio.sleep(0, result={"type": "http.request", "body": b""}), lambda m: messages.append(m) or asyncio.sleep(0))

    assert messages[0]["status"] == 204


@pytest.mark.asyncio
async def test_validation_error_structured_loc_and_type():
    app = FastAPI()

    class CreatePayload(BaseModel):
        title: str
        count: int = Field(gt=0)

    @app.post("/create")
    def create(payload: CreatePayload):
        return payload

    # Test invalid count <= 0
    scope = {
        "type": "http",
        "method": "POST",
        "path": "/create",
        "headers": [(b"content-type", b"application/json")],
        "query_string": b"",
    }
    invalid_payload = json.dumps({"title": "book", "count": -5}).encode("utf-8")
    messages = []
    await app(scope, lambda: asyncio.sleep(0, result={"type": "http.request", "body": invalid_payload}), lambda m: messages.append(m) or asyncio.sleep(0))

    assert messages[0]["status"] == 422
    body = json.loads(messages[1]["body"].decode("utf-8"))
    assert "detail" in body
    assert isinstance(body["detail"], list)
    assert any("greater_than" in err.get("type", "") or "count" in str(err) for err in body["detail"])


@pytest.mark.asyncio
async def test_custom_http_exception_with_dict_detail():
    app = FastAPI()

    @app.get("/error_custom")
    def trigger_error():
        raise HTTPException(
            status_code=400,
            detail={"code": "INVALID_STATE", "message": "State machine rejected transition"}
        )

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/error_custom",
        "headers": [],
        "query_string": b"",
    }
    messages = []
    await app(scope, lambda: asyncio.sleep(0, result={"type": "http.request", "body": b""}), lambda m: messages.append(m) or asyncio.sleep(0))

    assert messages[0]["status"] == 400
    body = json.loads(messages[1]["body"].decode("utf-8"))
    assert body["detail"] == {"code": "INVALID_STATE", "message": "State machine rejected transition"}
