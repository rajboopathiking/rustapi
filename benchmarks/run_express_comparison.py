import subprocess
import sys
import time
import json
import statistics

RUSTAPI_URL = "http://127.0.0.1:8092"
EXPRESS_URL = "http://127.0.0.1:8094"

POST_PAYLOAD = json.dumps({"name": "Laptop", "price": 999.99, "quantity": 5})

def run_oha(url, method="GET", body=None, concurrency=32, duration="4s"):
    cmd = [
        "oha",
        "-c", str(concurrency),
        "-z", duration,
        "--output-format", "json",
    ]
    if method == "POST":
        cmd.extend(["-m", "POST", "-H", "Content-Type: application/json"])
        if body:
            cmd.extend(["-d", body])
    cmd.append(url)

    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        return 0.0, 0.0, 0.0

    try:
        data = json.loads(res.stdout)
        summary = data.get("summary", {})
        rps = summary.get("requestsPerSec", 0.0)
        avg_lat = summary.get("average", 0.0) * 1000.0
        p95_lat = data.get("latencyPercentiles", {}).get("p95", 0.0) * 1000.0
        return rps, avg_lat, p95_lat
    except Exception:
        return 0.0, 0.0, 0.0

def run_scenario(name, path, method="GET", body=None, concurrency=32, trials=3):
    print(f"\n--- Testing: {name} ({method} {path}) [c={concurrency}] ---")
    results = {}
    for label, base in [("Express.js", EXPRESS_URL), ("RustAPI", RUSTAPI_URL)]:
        url = f"{base}{path}"
        rps_list, lat_list, p95_list = [], [], []
        for _ in range(trials):
            time.sleep(0.5)
            rps, avg, p95 = run_oha(url, method=method, body=body, concurrency=concurrency, duration="4s")
            rps_list.append(rps)
            lat_list.append(avg)
            p95_list.append(p95)
        med_rps = statistics.median(rps_list)
        med_lat = statistics.median(lat_list)
        med_p95 = statistics.median(p95_list)
        results[label] = (med_rps, med_lat, med_p95)
        print(f"  {label:12s}: {med_rps:8.1f} req/s | {med_lat:5.2f} ms avg | {med_p95:5.2f} ms p95")

    exp_rps = results["Express.js"][0]
    ra_rps = results["RustAPI"][0]
    speedup = ra_rps / exp_rps if exp_rps > 0 else 1.0
    status = "🏆 RustAPI BEATS Express.js" if speedup > 1.0 else "Express.js leads"
    print(f"  ==> Result: {status} ({speedup:.2f}x)")
    return results

def main():
    print("Starting RustAPI and Express servers...")
    p_ra = subprocess.Popen([sys.executable, "benchmarks/rustapi_bench.py"])
    p_exp = subprocess.Popen(["node", "benchmarks/node_app/express_server.js"])
    time.sleep(2.5)

    try:
        scenarios = [
            ("Sync JSON", "/json", "GET", None, 32),
            ("Async JSON", "/async_json", "GET", None, 32),
            ("Validated POST", "/items", "POST", POST_PAYLOAD, 32),
            ("50-Item Serialization", "/users", "GET", None, 32),
            ("High Concurrency Async JSON", "/async_json", "GET", None, 100),
        ]

        table = []
        for name, path, method, body, conc in scenarios:
            res = run_scenario(name, path, method=method, body=body, concurrency=conc)
            exp_rps, exp_avg, exp_p95 = res["Express.js"]
            ra_rps, ra_avg, ra_p95 = res["RustAPI"]
            diff = (ra_rps / exp_rps) if exp_rps > 0 else 0
            table.append((name, conc, exp_rps, exp_p95, ra_rps, ra_p95, diff))

        print("\n" + "="*80)
        print("📊 FINAL COMPARISON: RustAPI vs Express.js")
        print("="*80)
        print(f"{'Scenario':<28} | {'Conc':<5} | {'Express.js':<18} | {'RustAPI (v1.8.9)':<18} | {'Ratio':<10}")
        print("-" * 85)
        for name, conc, exp_rps, exp_p95, ra_rps, ra_p95, diff in table:
            winner = f"{diff:.2f}x (RustAPI)" if diff >= 1.0 else f"{1/diff:.2f}x (Express)"
            print(f"{name:<28} | {conc:<5} | {exp_rps:7.0f} r/s ({exp_p95:4.1f}ms) | {ra_rps:7.0f} r/s ({ra_p95:4.1f}ms) | {winner}")
        print("="*80)

    finally:
        p_ra.terminate()
        p_exp.terminate()
        p_ra.wait()
        p_exp.wait()

if __name__ == "__main__":
    main()
