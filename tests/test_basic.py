import time
import threading
import logging
import pytest
import requests
from pydantic import BaseModel
import asyncio
import rustapi
import json

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

# Pydantic Model for testing request body validation
class Item(BaseModel):
    name: str
    description: str
    price: float
    tax: float = 0.0

# ---------------------------------------------------------
# DEFINE ROUTES (Evaluated at module load time)
# ---------------------------------------------------------

@app.get("/")
def root():
    logger.info("--> Handling GET / (root)")
    return {"message": "Welcome to RustAPI production test suite!"}

@app.get("/sync")
def sync_route():
    logger.info("--> Handling GET /sync (synchronous task)")
    return {"type": "sync", "status": "completed", "timestamp": time.time()}

@app.get("/async")
async def async_route():
    logger.info("--> Handling GET /async (asynchronous event loop task)")
    await asyncio.sleep(0.1)
    return {"type": "async", "status": "completed"}

@app.get("/items/{item_id}")
def get_item(req):
    item_id = req.path_params.get("item_id")
    query_search = req.query_params.get("search", "none")
    logger.info(f"--> Handling GET /items/{item_id} with query search='{query_search}'")
    return {
        "item_id": item_id, 
        "query_params": req.query_params
    }

@app.post("/data")
def post_data(data: Item):
    logger.info(f"--> Handling POST /data with validated Pydantic payload: {data}")
    total = data.price + (data.tax if data.tax else 0.0)
    return {
        "status": "validated", 
        "item_name": data.name, 
        "total_price": total
    }

# ---------------------------------------------------------
# DEFINE MCP TOOLS / RESOURCES / PROMPTS
# ---------------------------------------------------------

@app.tool()
def add_numbers(a: int, b: int) -> int:
    """Add two numbers together."""
    logger.info(f"--> Handling MCP tool add_numbers(a={a}, b={b})")
    return a + b

@app.tool(name="greet", description="Greet a person by name")
def greet_tool(name: str) -> str:
    logger.info(f"--> Handling MCP tool greet(name={name})")
    return f"Hello, {name}!"

@app.resource("config://app-name")
def app_name_resource() -> str:
    logger.info("--> Handling MCP resource config://app-name")
    return "RustAPI Test Suite"

@app.prompt()
def summary_prompt(topic: str) -> str:
    """Generate a prompt asking for a summary of a topic."""
    logger.info(f"--> Handling MCP prompt summary_prompt(topic={topic})")
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
    """POST a single JSON-RPC 2.0 message to /mcp and return the raw response."""
    payload = {"jsonrpc": "2.0", "id": msg_id, "method": method}
    if params is not None:
        payload["params"] = params
    return requests.post(f"{BASE}/mcp", json=payload)

# ---------------------------------------------------------
# NATIVE PYTEST TEST FUNCTIONS - HTTP
# ---------------------------------------------------------

def test_root_endpoint():
    logger.info("[RUNNING] Test 1: Root GET endpoint")
    res = requests.get(f"{BASE}/")
    assert res.status_code == 200
    assert "Welcome" in res.json().get("message", "")
    logger.info("✅ [PASSED] Test 1: Root endpoint verified successfully.\n")

def test_sync_endpoint():
    logger.info("[RUNNING] Test 2: Synchronous endpoint handler")
    res = requests.get(f"{BASE}/sync")
    assert res.status_code == 200
    assert res.json().get("type") == "sync"
    logger.info("✅ [PASSED] Test 2: Sync execution verified successfully.\n")

def test_async_endpoint():
    logger.info("[RUNNING] Test 3: Asynchronous coroutine handler")
    res = requests.get(f"{BASE}/async")
    assert res.status_code == 200
    assert res.json().get("type") == "async"
    logger.info("✅ [PASSED] Test 3: Async event-loop bridge verified successfully.\n")

def test_path_and_query_params():
    logger.info("[RUNNING] Test 4: Path parameters and query parameters parser")
    res = requests.get(f"{BASE}/items/500?search=laptop&sort=asc")
    assert res.status_code == 200
    data = res.json()
    assert data.get("item_id") == "500"
    assert data.get("query_params", {}).get("search") == "laptop"
    assert data.get("query_params", {}).get("sort") == "asc"
    logger.info("✅ [PASSED] Test 4: Path & query parameters extracted correctly.\n")

def test_pydantic_validation():
    logger.info("[RUNNING] Test 5: Pydantic request body schema validation & serialization")
    payload = {
        "name": "Mechanical Keyboard",
        "description": "RGB Linear Switches",
        "price": 99.99,
        "tax": 10.0
    }
    res = requests.post(f"{BASE}/data", json=payload)
    assert res.status_code == 200, f"Got error response: {res.text}"
    data = res.json()
    assert data.get("status") == "validated"
    assert data.get("total_price") == 109.99
    logger.info("✅ [PASSED] Test 5: Pydantic parsing and type validation working seamlessly.\n")

def test_openapi_generation():
    logger.info("[RUNNING] Test 6: OpenAPI JSON schema generation")
    res = requests.get(f"{BASE}/openapi.json")
    assert res.status_code == 200
    spec = res.json()
    assert spec.get("openapi") == "3.0.0"
    assert "/items/{item_id}" in spec.get("paths", {})
    assert "/data" in spec.get("paths", {})
    logger.info("✅ [PASSED] Test 6: OpenAPI route documentation compiled correctly.\n")

def test_not_found_error():
    logger.info("[RUNNING] Test 7: 404 Not Found error handling")
    res = requests.get(f"{BASE}/invalid-route-path")
    assert res.status_code == 404
    logger.info("✅ [PASSED] Test 7: 404 error returned as expected.\n")

# ---------------------------------------------------------
# NATIVE PYTEST TEST FUNCTIONS - MCP (JSON-RPC over POST /mcp)
# ---------------------------------------------------------

def test_mcp_initialize():
    logger.info("[RUNNING] Test 8: MCP initialize handshake")
    res = _mcp_call("initialize", {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "pytest", "version": "1.0"},
    })
    assert res.status_code == 200, f"Got error response: {res.text}"
    data = res.json()
    assert data.get("jsonrpc") == "2.0"
    result = data.get("result", {})
    assert "protocolVersion" in result
    assert "capabilities" in result
    assert "serverInfo" in result
    logger.info("✅ [PASSED] Test 8: MCP initialize handshake verified.\n")

def test_mcp_tools_list():
    logger.info("[RUNNING] Test 9: MCP tools/list")
    res = _mcp_call("tools/list")
    assert res.status_code == 200, f"Got error response: {res.text}"
    tools = res.json().get("result", {}).get("tools", [])
    tool_names = [t["name"] for t in tools]
    assert "add_numbers" in tool_names
    assert "greet" in tool_names

    add_tool = next(t for t in tools if t["name"] == "add_numbers")
    assert add_tool["inputSchema"]["type"] == "object"
    assert "a" in add_tool["inputSchema"]["properties"]
    assert "b" in add_tool["inputSchema"]["properties"]
    assert add_tool["description"] == "Add two numbers together."
    logger.info("✅ [PASSED] Test 9: MCP tools/list returned registered tools with auto-generated schemas.\n")

def test_mcp_tools_call():
    logger.info("[RUNNING] Test 10: MCP tools/call (add_numbers)")
    res = _mcp_call("tools/call", {"name": "add_numbers", "arguments": {"a": 4, "b": 5}})
    assert res.status_code == 200, f"Got error response: {res.text}"
    result = res.json().get("result", {})
    assert result.get("isError") is False
    content_text = result["content"][0]["text"]
    assert json.loads(content_text) == 9
    logger.info("✅ [PASSED] Test 10: MCP tool executed and returned the expected result.\n")

def test_mcp_tools_call_string_result():
    logger.info("[RUNNING] Test 11: MCP tools/call (greet, string return)")
    res = _mcp_call("tools/call", {"name": "greet", "arguments": {"name": "Boopathi"}})
    assert res.status_code == 200, f"Got error response: {res.text}"
    result = res.json().get("result", {})
    assert result.get("isError") is False
    assert result["content"][0]["text"] == "Hello, Boopathi!"
    logger.info("✅ [PASSED] Test 11: MCP tool with string return handled without double-encoding.\n")

def test_mcp_tools_call_unknown():
    logger.info("[RUNNING] Test 12: MCP tools/call with an unregistered tool name")
    res = _mcp_call("tools/call", {"name": "does_not_exist", "arguments": {}})
    assert res.status_code == 200, f"Got error response: {res.text}"
    data = res.json()
    assert data["error"]["code"] == -32602
    logger.info("✅ [PASSED] Test 12: Unknown tool call correctly returned a JSON-RPC error.\n")

def test_mcp_resources_list_and_read():
    logger.info("[RUNNING] Test 13: MCP resources/list and resources/read")
    res_list = _mcp_call("resources/list")
    assert res_list.status_code == 200, f"Got error response: {res_list.text}"
    resources = res_list.json().get("result", {}).get("resources", [])
    uris = [r["uri"] for r in resources]
    assert "config://app-name" in uris

    res_read = _mcp_call("resources/read", {"uri": "config://app-name"})
    assert res_read.status_code == 200, f"Got error response: {res_read.text}"
    contents = res_read.json().get("result", {}).get("contents", [])
    assert contents[0]["uri"] == "config://app-name"
    assert contents[0]["text"] == "RustAPI Test Suite"
    logger.info("✅ [PASSED] Test 13: MCP resource listed and read successfully.\n")

def test_mcp_resources_read_unknown():
    logger.info("[RUNNING] Test 14: MCP resources/read with an unregistered URI")
    res = _mcp_call("resources/read", {"uri": "config://does-not-exist"})
    assert res.status_code == 200, f"Got error response: {res.text}"
    data = res.json()
    assert data["error"]["code"] == -32602
    logger.info("✅ [PASSED] Test 14: Unknown resource read correctly returned a JSON-RPC error.\n")

def test_mcp_prompts_list_and_get():
    logger.info("[RUNNING] Test 15: MCP prompts/list and prompts/get")
    res_list = _mcp_call("prompts/list")
    assert res_list.status_code == 200, f"Got error response: {res_list.text}"
    prompts = res_list.json().get("result", {}).get("prompts", [])
    names = [p["name"] for p in prompts]
    assert "summary_prompt" in names

    res_get = _mcp_call("prompts/get", {"name": "summary_prompt", "arguments": {"topic": "rust"}})
    assert res_get.status_code == 200, f"Got error response: {res_get.text}"
    messages = res_get.json().get("result", {}).get("messages", [])
    assert messages[0]["role"] == "user"
    assert "rust" in messages[0]["content"]["text"]
    logger.info("✅ [PASSED] Test 15: MCP prompt listed and retrieved successfully.\n")

def test_mcp_ping():
    logger.info("[RUNNING] Test 16: MCP ping")
    res = _mcp_call("ping")
    assert res.status_code == 200, f"Got error response: {res.text}"
    assert res.json().get("result") == {}
    logger.info("✅ [PASSED] Test 16: MCP ping responded correctly.\n")

def test_mcp_unknown_method():
    logger.info("[RUNNING] Test 17: MCP unrecognized method")
    res = _mcp_call("totally/bogus/method")
    assert res.status_code == 200, f"Got error response: {res.text}"
    data = res.json()
    assert data["error"]["code"] == -32601
    logger.info("✅ [PASSED] Test 17: Unrecognized MCP method correctly rejected.\n")

def test_mcp_notification_no_response_body():
    logger.info("[RUNNING] Test 18: MCP notification (no id) returns 202 with an empty body")
    payload = {"jsonrpc": "2.0", "method": "notifications/initialized"}
    res = requests.post(f"{BASE}/mcp", json=payload)
    assert res.status_code == 202
    assert res.text == ""
    logger.info("✅ [PASSED] Test 18: MCP notification correctly returned no response body.\n")
