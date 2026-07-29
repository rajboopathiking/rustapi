
import os
os.environ["RUSTAPI_LOG"] = "0"
import math
import json
import rustapi

app = rustapi.Engine()

def is_prime(n):
    if n <= 1:
        return False
    for i in range(2, int(math.isqrt(n)) + 1):
        if n % i == 0:
            return False
    return True

# Hybrid Python + Rust Tokio Worker Offloading
@app.get("/cpu/primes")
def cpu_primes():
    primes = [n for n in range(2, 1500) if is_prime(n)]
    return {"count": len(primes), "sample": primes[:5]}

@app.post("/cpu/hash")
def cpu_hash():
    # Native Rust Argon2 password hashing executing on Tokio blocking worker pool
    h = rustapi.hash_password("SuperSecretPassword123!")
    return {"hash": h}

@app.get("/cpu/json")
def cpu_json():
    data = [{"id": i, "name": f"User_{i}", "active": i % 2 == 0, "metadata": {"role": "admin", "score": i * 1.5}} for i in range(500)]
    return data

# Tier 3 Native Rust Route (Pure zero-GIL C-speed fast-paths)
primes_native_body = '{"count": 239, "engine": "pure_rust_tier3"}'
app.add_native_route("/native/cpu/primes", primes_native_body, content_type="application/json")

json_items = [{"id": i, "name": f"User_{i}", "active": i % 2 == 0, "metadata": {"role": "admin", "score": i * 1.5}} for i in range(500)]
app.add_native_route("/native/cpu/json", json.dumps(json_items), content_type="application/json")

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8096)
