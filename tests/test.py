import time
import threading
import logging
import requests
from pydantic import BaseModel
import asyncio
import rustapi
import json

# Configure detailed colorized/formatted logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler()]
)
logger = logging.getLogger("RustAPI-Production-Test")

app = rustapi.Engine()

# Pydantic Model for testing request body validation
class Item(BaseModel):
    name: str
    description: str
    price: float
    tax: float = 0.0

# ---------------------------------------------------------
# DEFINE ROUTES
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
# BACKGROUND SERVER RUNNER
# ---------------------------------------------------------

def run_server():
    logger.info("🚀 Launching embedded Rust server instance for testing...")
    app.run(host="127.0.0.1", port=8000, reload=False)

# ---------------------------------------------------------
# AUTOMATED UNIT TEST SUITE
# ---------------------------------------------------------

def run_unit_tests():
    # Wait briefly for the background server thread to bind to port 8000
    time.sleep(1.0)
    
    base_url = "http://127.0.0.1:8000"
    passed = 0
    total = 7

    logger.info("==================================================")
    logger.info("🧪 INITIALIZING RUSTAPI AUTOMATED TEST SUITE")
    logger.info("==================================================")

    # 1. Test Root Endpoint
    try:
        logger.info("[RUNNING] Test 1: Root GET endpoint")
        res = requests.get(f"{base_url}/")
        assert res.status_code == 200, f"Expected status 200, got {res.status_code}"
        assert "Welcome" in res.json().get("message", "")
        logger.info("✅ [PASSED] Test 1: Root endpoint verified successfully.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 1: {e}\n")

    # 2. Test Sync Endpoint
    try:
        logger.info("[RUNNING] Test 2: Synchronous endpoint handler")
        res = requests.get(f"{base_url}/sync")
        assert res.status_code == 200
        assert res.json().get("type") == "sync"
        logger.info("✅ [PASSED] Test 2: Sync execution verified successfully.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 2: {e}\n")

    # 3. Test Async Endpoint
    try:
        logger.info("[RUNNING] Test 3: Asynchronous coroutine handler")
        res = requests.get(f"{base_url}/async")
        assert res.status_code == 200
        assert res.json().get("type") == "async"
        logger.info("✅ [PASSED] Test 3: Async event-loop bridge verified successfully.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 3: {e}\n")

    # 4. Test Path & Query Parameters
    try:
        logger.info("[RUNNING] Test 4: Path parameters and query parameters parser")
        res = requests.get(f"{base_url}/items/500?search=laptop&sort=asc")
        assert res.status_code == 200
        data = res.json()
        assert data.get("item_id") == "500"
        assert data.get("query_params", {}).get("search") == "laptop"
        assert data.get("query_params", {}).get("sort") == "asc"
        logger.info("✅ [PASSED] Test 4: Path & query parameters extracted correctly.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 4: {e}\n")

    # 5. Test Pydantic Model Validation (POST Body)
    try:
        logger.info("[RUNNING] Test 5: Pydantic request body schema validation & serialization")
        payload = {
            "name": "Mechanical Keyboard",
            "description": "RGB Linear Switches",
            "price": 99.99,
            "tax": 10.0
        }
        res = requests.post(f"{base_url}/data", json=payload)
        assert res.status_code == 200, f"Got error response: {res.text}"
        data = res.json()
        assert data.get("status") == "validated"
        assert data.get("total_price") == 109.99
        logger.info("✅ [PASSED] Test 5: Pydantic parsing and type validation working seamlessly.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 5: {e}\n")

    # 6. Test OpenAPI Specification Generation
    try:
        logger.info("[RUNNING] Test 6: OpenAPI JSON schema generation")
        res = requests.get(f"{base_url}/openapi.json")
        assert res.status_code == 200
        spec = res.json()
        assert spec.get("openapi") == "3.0.0"
        assert "/items/{item_id}" in spec.get("paths", {})
        assert "/data" in spec.get("paths", {})
        logger.info("✅ [PASSED] Test 6: OpenAPI route documentation compiled correctly.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 6: {e}\n")

    # 7. Test 404 Error Handling
    try:
        logger.info("[RUNNING] Test 7: 404 Not Found error handling")
        res = requests.get(f"{base_url}/invalid-route-path")
        assert res.status_code == 404
        logger.info("✅ [PASSED] Test 7: 404 error returned as expected.\n")
        passed += 1
    except AssertionError as e:
        logger.error(f"❌ [FAILED] Test 7: {e}\n")

    logger.info("==================================================")
    logger.info(f"📊 FINAL TEST SUMMARY: {passed}/{total} TESTS PASSED")
    logger.info("==================================================")

if __name__ == "__main__":
    # Start the local server instance in a daemon background thread
    server_thread = threading.Thread(target=run_server, daemon=True)
    server_thread.start()

    # Run the comprehensive unit test suite with detailed logs
    run_unit_tests()

    # Keep the script alive so you can open http://127.0.0.1:8000/docs
    logger.info("🌐 Server remains active for manual browser inspection at http://127.0.0.1:8000/docs")
    logger.info("Press Ctrl+C to terminate.")
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        logger.info("🛑 Shutting down test server cleanly.")