import time
import threading
import logging
import pytest
import requests
from pydantic import BaseModel
import asyncio
import rustapi
import json
import websockets

# Configure logging for pytest terminal output (use -s flag to view)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler()]
)
logger = logging.getLogger("RustAPI-PyTest")

HOST = "127.0.0.1"
PORT = 8000
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()

# ---------------------------------------------------------
# MOCKS FOR ADVANCED FEATURES (To satisfy lib.rs expectations)
# ---------------------------------------------------------

class Depends:
    """Mock Depends class for Dependency Injection. 
    The Rust engine duck-types this by checking the class name."""
    def __init__(self, dependency):
        self.dependency = dependency

class APIRouter:
    """Mock APIRouter for route modularization testing.
    The Rust engine extracts the `.routes` list from this object."""
    def __init__(self):
        self.routes = []
    def get(self, path):
        def decorator(func):
            self.routes.append(("GET", path, func))
            return func
        return decorator

# Pydantic Model for testing request body validation
class Item(BaseModel):
    name: str
    description: str
    price: float
    tax: float = 0.0

# ---------------------------------------------------------
# DEFINE HTTP ROUTES
# ---------------------------------------------------------

@app.get("/")
def root():
    return {"message": "Welcome to RustAPI production test suite!"}

@app.get("/sync")
def sync_route():
    return {"type": "sync", "status": "completed", "timestamp": time.time()}

@app.get("/async")
async def async_route():
    await asyncio.sleep(0.1)
    return {"type": "async", "status": "completed"}

@app.get("/items/{item_id}")
def get_item(req):
    item_id = req.path_params.get("item_id")
    query_search = req.query_params.get("search", "none")
    return {"item_id": item_id, "query_params": req.query_params}

@app.post("/data")
def post_data(data: Item):
    total = data.price + (data.tax if data.tax else 0.0)
    return {"status": "validated", "item_name": data.name, "total_price": total}

# -- Phase 1 & 2: Dependency Injection & Generators --
dep_state = {"teardown_called": False}

def db_session_generator():
    yield "active_db_connection"
    # This runs after the response is sent (teardown)
    dep_state["teardown_called"] = True

@app.get("/users")
def get_users(db=Depends(db_session_generator)):
    return {"status": "success", "db": db}

# -- Phase 3: Modularization (APIRouter) --
router = APIRouter()

@router.get("/ping")
def router_ping():
    return {"module": "router", "status": "pong"}

app.include_router(router, prefix="/api/v1")

# -- Phase 4: File Uploads & Forms (Multipart) --
@app.post("/upload")
def upload_file(req):
    files = req.files
    form = req.form
    if "document" not in files:
        return {"error": "no file"}
    
    doc = files["document"][0]
    content = doc.read().decode("utf-8")
    return {
        "filename": doc.filename,
        "description": form.get("description"),
        "content": content
    }

# -- Phase 5: WebSockets --
@app.websocket("/ws")
async def ws_endpoint(ws):
    while True:
        try:
            data = ws.receive_text()
            ws.send_text(f"echo: {data}")
        except Exception:
            break

# ---------------------------------------------------------
# DEFINE MCP TOOLS / RESOURCES / PROMPTS
# ---------------------------------------------------------

@app.tool()
def add_numbers(a: int, b: int) -> int:
    return a + b

@app.tool(name="greet", description="Greet a person by name")
def greet_tool(name: str) -> str:
    return f"Hello, {name}!"

@app.resource("config://app-name")
def app_name_resource() -> str:
    return "RustAPI Test Suite"

@app.prompt()
def summary_prompt(topic: str) -> str:
    return f"Please summarize the topic: {topic}"


# ---------------------------------------------------------
# PYTEST SESSION FIXTURE
# ---------------------------------------------------------

@pytest.fixture(scope="session", autouse=True)
def server_lifecycle():
    logger.info("🚀 Launching embedded Rust server instance for pytest session...")
    def run_server():
        app.run(host=HOST, port=PORT, reload=False)

    server_thread = threading.Thread(target=run_server, daemon=True)
    server_thread.start()

    # Wait until the server is actively listening
    deadline = time.time() + 5.0
    while time.time() < deadline:
        try:
            requests.get(f"{BASE}/", timeout=0.2)
            logger.info("🚀 Test server successfully bound and listening.")
            break
        except requests.exceptions.ConnectionError:
            time.sleep(0.1)
    else:
        raise RuntimeError("Test server failed to start within the deadline.")

    yield
    logger.info("🛑 Shutting down test server cleanly.")

# ---------------------------------------------------------
# MCP TEST HELPER
# ---------------------------------------------------------

def _mcp_call(method, params=None, msg_id=1):
    payload = {"jsonrpc": "2.0", "id": msg_id, "method": method}
    if params is not None:
        payload["params"] = params
    return requests.post(f"{BASE}/mcp", json=payload)

# ---------------------------------------------------------
# NATIVE PYTEST TEST FUNCTIONS - HTTP Core
# ---------------------------------------------------------

def test_root_endpoint():
    res = requests.get(f"{BASE}/")
    assert res.status_code == 200
    assert "Welcome" in res.json().get("message", "")

def test_sync_endpoint():
    res = requests.get(f"{BASE}/sync")
    assert res.status_code == 200
    assert res.json().get("type") == "sync"

def test_async_endpoint():
    res = requests.get(f"{BASE}/async")
    assert res.status_code == 200
    assert res.json().get("type") == "async"

# ---------------------------------------------------------
# TEST PHASE 1 & 2: Routing & Dependencies
# ---------------------------------------------------------

def test_path_and_query_params():
    res = requests.get(f"{BASE}/items/500?search=laptop&sort=asc")
    assert res.status_code == 200
    data = res.json()
    assert data.get("item_id") == "500"
    assert data.get("query_params", {}).get("search") == "laptop"

def test_dependency_injection_and_teardown():
    logger.info("[RUNNING] Test: Dependency Injection & Generator Teardown")
    res = requests.get(f"{BASE}/users")
    assert res.status_code == 200
    assert res.json().get("db") == "active_db_connection"
    
    # Wait briefly for the background thread to call the generator's `next()` teardown
    time.sleep(0.1)
    assert dep_state["teardown_called"] is True, "Generator teardown was not executed!"
    logger.info("✅ [PASSED] Dependency caching, execution, and teardown successful.")

def test_pydantic_validation():
    payload = {"name": "Mechanical Keyboard", "description": "RGB", "price": 99.99, "tax": 10.0}
    res = requests.post(f"{BASE}/data", json=payload)
    assert res.status_code == 200
    assert res.json().get("total_price") == 109.99

# ---------------------------------------------------------
# TEST PHASE 3: APIRouter
# ---------------------------------------------------------

def test_apirouter_prefixing():
    logger.info("[RUNNING] Test: APIRouter Prefix Nesting")
    res = requests.get(f"{BASE}/api/v1/ping")
    assert res.status_code == 200
    assert res.json().get("module") == "router"
    logger.info("✅ [PASSED] APIRouter successfully routed and prefixed paths.")

# ---------------------------------------------------------
# TEST PHASE 4: Multipart Form / File Uploads
# ---------------------------------------------------------

def test_multipart_file_upload():
    logger.info("[RUNNING] Test: Multipart UploadFile & Form Data")
    files = {'document': ('test.txt', b'Hello RustAPI!', 'text/plain')}
    data = {'description': 'A sample test file'}
    
    res = requests.post(f"{BASE}/upload", files=files, data=data)
    assert res.status_code == 200
    resp_data = res.json()
    assert resp_data["filename"] == "test.txt"
    assert resp_data["description"] == "A sample test file"
    assert resp_data["content"] == "Hello RustAPI!"
    logger.info("✅ [PASSED] File uploads and multipart parsing functioning perfectly.")

# ---------------------------------------------------------
# TEST PHASE 5: WebSockets
# ---------------------------------------------------------

def test_websocket_connection_and_streaming():
    logger.info("[RUNNING] Test: Native WebSockets Bidirectional Streaming")
    
    async def run_ws_test():
        uri = f"ws://{HOST}:{PORT}/ws"
        async with websockets.connect(uri) as ws:
            await ws.send("Socket Test Payload")
            response = await ws.recv()
            assert response == "echo: Socket Test Payload"
            
    # Run the async websocket test synchronously for pytest
    asyncio.run(run_ws_test())
    logger.info("✅ [PASSED] WebSocket HTTP upgrade and streaming successful.")


# ---------------------------------------------------------
# TEST OpenAPI & Docs
# ---------------------------------------------------------

def test_openapi_generation():
    res = requests.get(f"{BASE}/openapi.json")
    assert res.status_code == 200
    assert res.json().get("openapi") == "3.0.0"

def test_docs_endpoint_serves_swagger_ui():
    res = requests.get(f"{BASE}/docs")
    assert res.status_code == 200
    assert "swagger" in res.text.lower()

def test_not_found_error():
    res = requests.get(f"{BASE}/invalid-route-path")
    assert res.status_code == 404


# ---------------------------------------------------------
# NATIVE PYTEST TEST FUNCTIONS - MCP
# ---------------------------------------------------------

def test_mcp_initialize():
    res = _mcp_call("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "pytest", "version": "1.0"}})
    assert res.status_code == 200
    assert "protocolVersion" in res.json().get("result", {})

def test_mcp_tools_list():
    res = _mcp_call("tools/list")
    assert res.status_code == 200
    tools = res.json().get("result", {}).get("tools", [])
    assert "add_numbers" in [t["name"] for t in tools]

def test_mcp_tools_call():
    res = _mcp_call("tools/call", {"name": "add_numbers", "arguments": {"a": 4, "b": 5}})
    assert res.status_code == 200
    assert res.json()["result"]["isError"] is False
    assert json.loads(res.json()["result"]["content"][0]["text"]) == 9

def test_mcp_tools_call_string_result():
    res = _mcp_call("tools/call", {"name": "greet", "arguments": {"name": "Boopathi"}})
    assert res.status_code == 200
    assert res.json()["result"]["content"][0]["text"] == "Hello, Boopathi!"

def test_mcp_resources_list_and_read():
    res_list = _mcp_call("resources/list")
    assert res_list.status_code == 200
    
    res_read = _mcp_call("resources/read", {"uri": "config://app-name"})
    assert res_read.status_code == 200
    assert res_read.json()["result"]["contents"][0]["text"] == "RustAPI Test Suite"

def test_mcp_prompts_list_and_get():
    res_list = _mcp_call("prompts/list")
    assert res_list.status_code == 200
    
    res_get = _mcp_call("prompts/get", {"name": "summary_prompt", "arguments": {"topic": "rust"}})
    assert res_get.status_code == 200
    assert "rust" in res_get.json()["result"]["messages"][0]["content"]["text"]

def test_mcp_ping():
    res = _mcp_call("ping")
    assert res.status_code == 200

def test_mcp_notification_no_response_body():
    payload = {"jsonrpc": "2.0", "method": "notifications/initialized"}
    res = requests.post(f"{BASE}/mcp", json=payload)
    assert res.status_code == 202

if __name__ == "__main__":
    app.run(host=HOST, port=PORT, reload=False)
# ---------------------------------------------------------
# TEST PHASE 5: Lifespan Hooks & Parameter Coercion
# ---------------------------------------------------------
lifespan_state = {"startup": False, "shutdown": False}

@app.on_event("startup")
def on_startup():
    logger.info("--> Executing Rust-Triggered Startup Hook")
    lifespan_state["startup"] = True

@app.on_event("shutdown")
async def on_shutdown():
    logger.info("--> Executing Rust-Triggered Shutdown Hook")
    lifespan_state["shutdown"] = True

@app.get("/calc/{a}")
def calculate(a: int, b: float, active: bool):
    return {"a": a, "b": b, "active": active}

def test_lifespan_startup():
    assert lifespan_state["startup"] is True, "Startup hook did not fire!"

def test_parameter_coercion_success():
    res = requests.get(f"{BASE}/calc/42?b=3.14&active=true")
    assert res.status_code == 200
    data = res.json()
    assert data["a"] == 42
    assert data["b"] == 3.14
    assert data["active"] is True

def test_parameter_coercion_failure_422():
    res = requests.get(f"{BASE}/calc/not-an-int?b=3.14&active=true")
    assert res.status_code == 422
    assert "must be an integer" in res.json().get("detail", "")
