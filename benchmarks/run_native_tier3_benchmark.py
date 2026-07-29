import time
import asyncio
import aiohttp
import subprocess
import sys
import os

RUSTAPI_TIER3_CODE = """
import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi

app = rustapi.Engine()

# Tier 3: Rust-Native Fast-Path Route (0ms Python GIL overhead)
app.add_native_route("/fast-json", '{"status":"ok","engine":"pure_rust_tier3"}')

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8095)
"""

async def measure_native(num_requests: int = 3000, concurrency: int = 50):
    url = "http://127.0.0.1:8095/fast-json"
    connector = aiohttp.TCPConnector(limit=concurrency)
    async with aiohttp.ClientSession(connector=connector) as session:
        # Warmup
        for _ in range(20):
            async with session.get(url) as resp:
                await resp.read()

        start = time.perf_counter()
        
        async def fetch():
            async with session.get(url) as resp:
                await resp.read()
                return resp.status

        tasks = [asyncio.create_task(fetch()) for _ in range(num_requests)]
        await asyncio.gather(*tasks)
        
        elapsed = time.perf_counter() - start
        rps = num_requests / elapsed
        avg_latency_ms = (elapsed / num_requests) * 1000 * concurrency
        return rps, avg_latency_ms, elapsed

async def main():
    os.makedirs("benchmarks/data", exist_ok=True)
    with open("benchmarks/data/rustapi_tier3_app.py", "w") as f:
        f.write(RUSTAPI_TIER3_CODE)

    print("\n=========================================================================")
    print(" 🚀 RustAPI Tier 3 (Rust-Native Business Logic) Benchmark")
    print("=========================================================================\n")

    proc = subprocess.Popen([sys.executable, "benchmarks/data/rustapi_tier3_app.py"])
    await asyncio.sleep(2.0)

    rps, lat, time_sec = await measure_native()

    print(f"   RustAPI Tier 3 (Rust-Native Fast-Path) : {rps:7.2f} req/sec | {lat:6.2f} ms avg latency")
    print(f"   Total Requests Served                   : 3,000 requests in {time_sec:.3f}s")
    print("=========================================================================\n")

    proc.terminate()
    proc.wait()

if __name__ == "__main__":
    asyncio.run(main())
