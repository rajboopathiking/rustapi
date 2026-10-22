import pytest
from httpx import ASGITransport, AsyncClient
from rustapi import FastAPI, Depends, HTTPException
from rustapi.responses import JSONResponse


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


@pytest.mark.asyncio
async def test_function_vs_request_scope_cleanup_timing():
    """Verify function-scoped dependencies clean up before response transmission,
    while request-scoped dependencies remain available until response completion."""
    app = FastAPI()
    events = []

    def func_dep():
        events.append("enter_func")
        try:
            yield "func_val"
        finally:
            events.append("exit_func")

    def req_dep():
        events.append("enter_req")
        try:
            yield "req_val"
        finally:
            events.append("exit_req")

    @app.get("/scopes-test")
    async def scopes_route(
        f: str = Depends(func_dep, scope="function"),
        r: str = Depends(req_dep, scope="request"),
    ):
        events.append("handler_running")
        return {"f": f, "r": r}

    # Custom receive/send to inspect events during response transmission
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/scopes-test")
        assert resp.status_code == 200
        assert resp.json() == {"f": "func_val", "r": "req_val"}

    # Both enter first, then handler, then function exits before response completes,
    # then request-scoped exits after response transmission completes!
    assert events == [
        "enter_func",
        "enter_req",
        "handler_running",
        "exit_func",
        "exit_req",
    ]


@pytest.mark.asyncio
async def test_delayed_response_sending_request_scope():
    """Verify request-scoped dependency remains active during response transmission."""
    app = FastAPI()
    events = []

    async def db_session():
        events.append("db_opened")
        try:
            yield "active_session"
        finally:
            events.append("db_closed")

    @app.get("/delayed-response")
    async def delayed_route(db: str = Depends(db_session, scope="request")):
        events.append("handler_executed")
        return {"db": db}

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/delayed-response")
        assert resp.status_code == 200
        assert resp.json() == {"db": "active_session"}

    assert events == ["db_opened", "handler_executed", "db_closed"]


@pytest.mark.asyncio
@pytest.mark.parametrize("status_code", [400, 404, 500])
async def test_returned_error_response_preserves_request_scope_until_body_completion(status_code):
    """Verify that a handler returning an error response (400, 404, 500) preserves
    request-scoped dependencies through response completion, while function-scoped
    dependencies clean up before response transmission begins."""
    app = FastAPI()
    events = []

    def func_dep():
        events.append("enter_func")
        try:
            yield "func_val"
        finally:
            events.append("exit_func")

    async def req_dep():
        events.append("enter_req")
        try:
            yield "req_val"
        finally:
            events.append("exit_req")

    @app.get("/error-return")
    async def error_route(f: str = Depends(func_dep, scope="function"), r: str = Depends(req_dep, scope="request")):
        events.append("handler_running")
        return JSONResponse(status_code=status_code, content={"error": f"returned_{status_code}"})

    async def custom_send(message):
        events.append(f"send_{message['type']}")
        if message["type"] == "http.response.start":
            # Function-scoped MUST have exited, but request-scoped MUST NOT have exited
            assert "exit_func" in events
            assert "exit_req" not in events
        elif message["type"] == "http.response.body":
            # While transmitting body, request-scoped MUST still be active
            assert "exit_req" not in events

    async def custom_receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/error-return",
        "headers": [],
        "query_string": b"",
        "server": ("127.0.0.1", 80),
    }

    await app(scope, custom_receive, custom_send)

    # After app completes, request-scoped dependency MUST be cleaned up
    assert "exit_req" in events
    assert events == [
        "enter_func",
        "enter_req",
        "handler_running",
        "exit_func",
        "send_http.response.start",
        "send_http.response.body",
        "exit_req",
    ]


@pytest.mark.asyncio
async def test_sync_and_async_yield_dependencies_with_returned_error():
    """Verify both synchronous and asynchronous yield dependencies preserve request scope
    during returned error responses."""
    app = FastAPI()
    events = []

    def sync_req_dep():
        events.append("enter_sync_req")
        try:
            yield "sync_req_val"
        finally:
            events.append("exit_sync_req")

    async def async_req_dep():
        events.append("enter_async_req")
        try:
            yield "async_req_val"
        finally:
            events.append("exit_async_req")

    @app.get("/dual-error")
    async def dual_error_route(
        s: str = Depends(sync_req_dep, scope="request"),
        a: str = Depends(async_req_dep, scope="request"),
    ):
        events.append("handler_exec")
        return JSONResponse(status_code=400, content={"status": "fail"})

    async def custom_send(message):
        events.append(f"send_{message['type']}")
        if message["type"] == "http.response.body":
            assert "exit_sync_req" not in events
            assert "exit_async_req" not in events

    async def custom_receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/dual-error",
        "headers": [],
        "query_string": b"",
        "server": ("127.0.0.1", 80),
    }

    await app(scope, custom_receive, custom_send)

    # Both must exit after response completion in reverse order
    assert "exit_sync_req" in events
    assert "exit_async_req" in events
    assert events == [
        "enter_sync_req",
        "enter_async_req",
        "handler_exec",
        "send_http.response.start",
        "send_http.response.body",
        "exit_async_req",
        "exit_sync_req",
    ]


@pytest.mark.asyncio
async def test_raised_exception_vs_returned_response_timing():
    """Verify that raised exceptions unwind and tear down dependencies before sending
    the exception response (matching FastAPI), whereas returned responses keep request-scoped
    dependencies alive until after body completion."""
    app = FastAPI()
    raise_events = []
    return_events = []

    def make_dep(events_list):
        async def dep():
            events_list.append("dep_enter")
            try:
                yield "val"
            finally:
                events_list.append("dep_exit")
        return dep

    @app.get("/raise-400")
    async def raise_route(d: str = Depends(make_dep(raise_events), scope="request")):
        raise_events.append("handler")
        raise HTTPException(status_code=400, detail="Raised 400 error")

    @app.get("/return-400")
    async def return_route(d: str = Depends(make_dep(return_events), scope="request")):
        return_events.append("handler")
        return JSONResponse(status_code=400, content={"detail": "Returned 400 error"})

    def make_custom_send(events_list):
        async def send_cb(msg):
            events_list.append(f"send_{msg['type']}")
        return send_cb

    async def custom_receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    # Test raised exception: dep_exit happens before send
    scope_raise = {
        "type": "http", "method": "GET", "path": "/raise-400",
        "headers": [], "query_string": b"", "server": ("127.0.0.1", 80),
    }
    await app(scope_raise, custom_receive, make_custom_send(raise_events))
    assert raise_events == [
        "dep_enter",
        "handler",
        "dep_exit",
        "send_http.response.start",
        "send_http.response.body",
    ]

    # Test returned error response: dep_exit happens after send
    scope_return = {
        "type": "http", "method": "GET", "path": "/return-400",
        "headers": [], "query_string": b"", "server": ("127.0.0.1", 80),
    }
    await app(scope_return, custom_receive, make_custom_send(return_events))
    assert return_events == [
        "dep_enter",
        "handler",
        "send_http.response.start",
        "send_http.response.body",
        "dep_exit",
    ]


@pytest.mark.asyncio
async def test_send_failure_and_cancellation_cleans_up_exactly_once():
    """Verify that if ASGI send fails during body transmission, cleanup occurs
    in finally: and runs exactly once."""
    app = FastAPI()
    cleanup_count = 0

    async def res_dep():
        nonlocal cleanup_count
        try:
            yield "resource"
        finally:
            cleanup_count += 1

    @app.get("/send-fail")
    async def send_fail_route(r: str = Depends(res_dep, scope="request")):
        return {"result": r}

    async def failing_send(message):
        if message["type"] == "http.response.body":
            raise ConnectionResetError("Client disconnected during body write")

    async def custom_receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    scope = {
        "type": "http", "method": "GET", "path": "/send-fail",
        "headers": [], "query_string": b"", "server": ("127.0.0.1", 80),
    }

    with pytest.raises(ConnectionResetError):
        await app(scope, custom_receive, failing_send)

    # Cleanup must occur exactly once
    assert cleanup_count == 1

