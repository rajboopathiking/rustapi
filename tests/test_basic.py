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
# NATIVE PYTEST TEST FUNCTIONS
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