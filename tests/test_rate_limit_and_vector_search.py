import time
import subprocess
import signal
import sys
import pytest
import requests
import rustapi

HOST = "127.0.0.1"
PORT = 8063
BASE = f"http://{HOST}:{PORT}"

app = rustapi.Engine()

# Initialize SQLite database
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE IF NOT EXISTS rl_items (id INTEGER PRIMARY KEY, title TEXT)")
db.execute("DELETE FROM rl_items")
db.execute("INSERT INTO rl_items (title) VALUES ('Item Alpha'), ('Item Beta')")

# 1. Python route with rate limit (3 requests per second)
@app.get("/rate-limited/python", rate_limit="3/second")
def rl_python():
    return {"status": "ok", "type": "python"}

# 2. Native SQL route with rate limit (2 requests per second)
@app.get("/rate-limited/sql", sql="SELECT * FROM rl_items", rate_limit="2/second")
def rl_sql():
    pass

# 3. Native static route with rate limit (2 requests per second)
app.add_native_route("/rate-limited/static", '{"status": "native_ok"}', rate_limit="2/second")


@pytest.fixture(scope="module", autouse=True)
def run_rl_server():
    proc = subprocess.Popen(
        [
            sys.executable,
            "-c",
            f"from tests.test_rate_limit_and_vector_search import app, HOST, PORT; app.run(host='{HOST}', port={PORT}, reload=False)",
        ],
        start_new_session=(sys.platform != "win32"),
    )

    connected = False
    for _ in range(40):
        try:
            r = requests.get(f"{BASE}/rate-limited/python", timeout=1)
            if r.status_code == 200:
                connected = True
                break
        except Exception:
            time.sleep(0.1)

    if not connected:
        proc.kill()
        pytest.fail("Rate limit test server failed to start within 4 seconds")

    yield

    try:
        if sys.platform != "win32":
            import os
            os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        else:
            proc.terminate()
        proc.wait(timeout=3)
    except Exception:
        proc.kill()


# ==== AI / VECTOR OPERATIONS TESTS ====

def test_cosine_similarity():
    """Verify SIMD cosine similarity calculations in Rust."""
    # 1. Identical vectors -> 1.0
    v1 = [1.0, 2.0, 3.0]
    assert pytest.approx(rustapi.cosine_similarity(v1, v1), 1e-4) == 1.0

    # 2. Orthogonal vectors -> 0.0
    v_a = [1.0, 0.0, 0.0]
    v_b = [0.0, 1.0, 0.0]
    assert pytest.approx(rustapi.cosine_similarity(v_a, v_b), 1e-4) == 0.0

    # 3. Opposite vectors -> -1.0
    v_neg = [-1.0, -2.0, -3.0]
    assert pytest.approx(rustapi.cosine_similarity(v1, v_neg), 1e-4) == -1.0


def test_vector_search_top_k():
    """Verify fast native vector search across candidate embeddings."""
    query = [1.0, 0.0, 0.0]
    candidates = [
        [0.9, 0.1, 0.0],   # idx 0: very similar (~0.99)
        [0.0, 1.0, 0.0],   # idx 1: orthogonal (0.0)
        [0.99, 0.01, 0.0], # idx 2: highest similarity (~1.0)
        [-0.5, 0.0, 0.0],  # idx 3: negative (-1.0)
    ]

    results = rustapi.vector_search(query, candidates, top_k=2)
    assert len(results) == 2
    # Highest similarity should be index 2
    assert results[0][0] == 2
    assert results[0][1] > 0.99
    # Second highest similarity should be index 0
    assert results[1][0] == 0


def test_vector_search_high_dimensions_speed():
    """Verify vector search on 1536-dimensional vectors (OpenAI embedding size)."""
    dim = 1536
    query = [0.05] * dim
    candidates = [[0.05] * dim for _ in range(150)]
    candidates[42] = [0.05] * dim

    t0 = time.time()
    results = app.vector_search(query, candidates, top_k=5)
    duration = time.time() - t0

    assert len(results) == 5
    # Must complete comfortably under 0.35s even in unoptimized debug mode
    assert duration < 0.35


# ==== RATE LIMITING TESTS ====

def test_python_route_rate_limiting():
    """Verify that Python routes reject requests exceeding rate limit with HTTP 429."""
    # Reset/wait window
    time.sleep(1.0)

    # Allowed: 3/second
    r1 = requests.get(f"{BASE}/rate-limited/python", timeout=2)
    assert r1.status_code == 200

    r2 = requests.get(f"{BASE}/rate-limited/python", timeout=2)
    assert r2.status_code == 200

    r3 = requests.get(f"{BASE}/rate-limited/python", timeout=2)
    assert r3.status_code == 200

    # 4th request MUST be rejected with HTTP 429 Too Many Requests
    r4 = requests.get(f"{BASE}/rate-limited/python", timeout=2)
    assert r4.status_code == 429
    assert r4.headers.get("retry-after") is not None
    assert "Rate limit exceeded" in r4.json()["detail"]

    # Wait for window refill
    time.sleep(1.1)
    r5 = requests.get(f"{BASE}/rate-limited/python", timeout=2)
    assert r5.status_code == 200


def test_native_sql_route_rate_limiting():
    """Verify that native SQL routes enforce rate limiting directly in Rust."""
    time.sleep(1.0)

    # Allowed: 2/second
    r1 = requests.get(f"{BASE}/rate-limited/sql", timeout=2)
    assert r1.status_code == 200

    r2 = requests.get(f"{BASE}/rate-limited/sql", timeout=2)
    assert r2.status_code == 200

    # 3rd request rejected with 429
    r3 = requests.get(f"{BASE}/rate-limited/sql", timeout=2)
    assert r3.status_code == 429
    assert "Rate limit exceeded" in r3.json()["detail"]
