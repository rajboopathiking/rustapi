import pytest
from httpx import ASGITransport, AsyncClient
from rustapi import FastAPI, Depends, HTTPException


@pytest.mark.asyncio
async def test_sync_yield_dependency_teardown_order():
    """Verify synchronous generator dependency runs setup, handler, and teardown exactly once."""
    app = FastAPI()
    events = []

    def get_sync_resource():
        events.append("enter_sync")
        try:
            yield "sync_val"
        finally:
            events.append("exit_sync")

    @app.get("/sync-dep")
    def sync_route(val: str = Depends(get_sync_resource)):
        events.append(f"handler_{val}")
        return {"result": val}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/sync-dep")
        assert resp.status_code == 200
        assert resp.json() == {"result": "sync_val"}

    assert events == ["enter_sync", "handler_sync_val", "exit_sync"]


@pytest.mark.asyncio
async def test_async_yield_dependency_teardown_order():
    """Verify asynchronous generator dependency runs setup, handler, and teardown exactly once."""
    app = FastAPI()
    events = []

    async def get_async_resource():
        events.append("enter_async")
        try:
            yield "async_val"
        finally:
            events.append("exit_async")

    @app.get("/async-dep")
    async def async_route(val: str = Depends(get_async_resource)):
        events.append(f"handler_{val}")
        return {"result": val}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/async-dep")
        assert resp.status_code == 200
        assert resp.json() == {"result": "async_val"}

    assert events == ["enter_async", "handler_async_val", "exit_async"]


@pytest.mark.asyncio
async def test_nested_yield_dependencies_teardown_order():
    """Verify nested dependencies teardown in reverse LIFO order."""
    app = FastAPI()
    events = []

    async def outer_dep():
        events.append("enter_outer")
        try:
            yield "outer"
        finally:
            events.append("exit_outer")

    async def inner_dep(outer: str = Depends(outer_dep)):
        events.append(f"enter_inner_with_{outer}")
        try:
            yield f"inner_using_{outer}"
        finally:
            events.append("exit_inner")

    @app.get("/nested")
    async def nested_route(res: str = Depends(inner_dep)):
        events.append(f"handler_{res}")
        return {"res": res}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/nested")
        assert resp.status_code == 200

    assert events == [
        "enter_outer",
        "enter_inner_with_outer",
        "handler_inner_using_outer",
        "exit_inner",
        "exit_outer",
    ]


@pytest.mark.asyncio
async def test_yield_dependency_teardown_on_handler_exception():
    """Verify generator dependencies tear down cleanly even when route handler raises an error."""
    app = FastAPI()
    events = []

    async def managed_db():
        events.append("enter_db")
        try:
            yield "db_session"
        finally:
            events.append("exit_db")

    @app.get("/failing-route")
    async def failing_route(db: str = Depends(managed_db)):
        events.append("handler_raises")
        raise HTTPException(status_code=400, detail="Custom bad request")

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/failing-route")
        assert resp.status_code == 400
        assert resp.json() == {"detail": "Custom bad request"}

    # Teardown MUST have executed despite the exception
    assert events == ["enter_db", "handler_raises", "exit_db"]


@pytest.mark.asyncio
async def test_yield_dependency_teardown_on_subsequent_dependency_error():
    """Verify that if dep1 succeeds with yield but dep2 raises, dep1 is still torn down cleanly."""
    app = FastAPI()
    events = []

    async def dep1():
        events.append("enter_dep1")
        try:
            yield "val1"
        finally:
            events.append("exit_dep1")

    async def dep2():
        events.append("dep2_fails")
        raise HTTPException(status_code=403, detail="Forbidden dependency")

    @app.get("/dep-error")
    async def dep_error_route(d1: str = Depends(dep1), d2: None = Depends(dep2)):
        return {"ok": True}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/dep-error")
        assert resp.status_code == 403

    assert "enter_dep1" in events
    assert "dep2_fails" in events
    assert "exit_dep1" in events
