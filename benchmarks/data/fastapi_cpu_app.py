
import uvicorn
import math
import hashlib
import json
from fastapi import FastAPI
from fastapi.responses import JSONResponse

app = FastAPI()

def is_prime(n):
    if n <= 1:
        return False
    for i in range(2, int(math.isqrt(n)) + 1):
        if n % i == 0:
            return False
    return True

@app.get("/cpu/primes")
def cpu_primes():
    # Heavy CPU loop computing primes
    primes = [n for n in range(2, 1500) if is_prime(n)]
    return {"count": len(primes), "sample": primes[:5]}

@app.post("/cpu/hash")
def cpu_hash():
    # Cryptographic PBKDF2 password hashing (CPU Intensive)
    dk = hashlib.pbkdf2_hmac('sha256', b'SuperSecretPassword123!', b'salt_val_123', 1000)
    return {"hash": dk.hex()}

@app.get("/cpu/json")
def cpu_json():
    # Heavy JSON payload serialization (500 items)
    data = [{"id": i, "name": f"User_{i}", "active": i % 2 == 0, "metadata": {"role": "admin", "score": i * 1.5}} for i in range(500)]
    return data

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8095, log_level="error")
