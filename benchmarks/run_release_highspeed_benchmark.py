import time
import asyncio
import aiohttp
import subprocess
import sys
import os

RUSTAPI_MULTIWORKER_CODE = """
import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi

app = rustapi.Engine()

# Tier 3: Rust-Native Fast-Path Route (0ms Python GIL overhead)
app.add_native_route("/fast-json", '{"status":"ok","engine":"pure_rust_tier3"}')

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8096, workers=4)
"""

async def run_worker_load(num_requests: int = 5000, concurrency: int = 50):
    url = "http://127.0.0.1:8096/fast-json"
    connector = aiohttp.TCPConnector(limit=concurrency)
    async with aiohttp.ClientSession(connector=connector) as session:
        # Warmup
        for _ in range(20):
            try:
                async with session.get(url) as resp:
                    await resp.read()
            except Exception:
                pass

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
    with open("benchmarks/data/rustapi_multiworker_app.py", "w") as f:
        f.write(RUSTAPI_MULTIWORKER_CODE)

    print("\n=========================================================================")
    print(" ⚡ RustAPI Multi-Worker Tier 3 High-Speed Benchmark")
    print("=========================================================================\n")

    proc = subprocess.Popen([sys.executable, "benchmarks/data/rustapi_multiworker_app.py"])
    await asyncio.sleep(2.5)

    # Run 2 parallel client sessions to generate high concurrency load
    t1 = asyncio.create_task(run_worker_load(5000, 50))
    t2 = asyncio.create_task(run_worker_load(5000, 50))
    
    r1, r2 = await asyncio.gather(t1, t2)
    
    total_rps = r1[0] + r2[0]
    avg_lat = (r1[1] + r2[1]) / 2.0

    print(f"   RustAPI Multi-Worker Tier 3 Throughput : {total_rps:8.2f} req/sec | {avg_lat:6.2f} ms avg latency")
    print("   Total Requests Served                    : 10,000 requests in parallel")
    print("=========================================================================\n")

    proc.terminate()
    proc.wait()

if __name__ == "__main__":
    asyncio.run(main())
