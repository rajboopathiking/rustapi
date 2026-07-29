import os
import signal
import time
import subprocess
import requests
import sys

HOST = "127.0.0.1"
PORT = 8017
BASE = f"http://{HOST}:{PORT}"
FILE_PATH = "scratch/tmp_reload_app.py"


def test_reload_mode_spawns_server_and_outputs_logs():
    os.makedirs("scratch", exist_ok=True)
    code = f"""import rustapi

app = rustapi.Engine()

@app.get("/ping")
def ping():
    return {{"status": "pong"}}

if __name__ == "__main__":
    app.run(host="{HOST}", port={PORT}, reload=True)
"""
    with open(FILE_PATH, "w") as f:
        f.write(code)

    proc = subprocess.Popen(
        [sys.executable, FILE_PATH],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        connected = False
        for _ in range(30):
            try:
                r = requests.get(f"{BASE}/ping", timeout=1)
                if r.status_code == 200 and r.json() == {"status": "pong"}:
                    connected = True
                    break
            except Exception:
                time.sleep(0.1)

        assert connected, "Server failed to respond in reload=True mode"

        # Trigger request access log
        r2 = requests.get(f"{BASE}/ping")
        assert r2.status_code == 200

    finally:
        try:
            proc.send_signal(signal.SIGINT)
            stdout, stderr = proc.communicate(timeout=3)
        except Exception:
            proc.kill()
            stdout, stderr = proc.communicate()

        combined_logs = stdout + stderr

        assert "Will watch for file changes in" in combined_logs or "Started server process" in combined_logs
        assert f"http://{HOST}:{PORT}" in combined_logs
        assert "GET /ping HTTP/1.1" in combined_logs

        if os.path.exists(FILE_PATH):
            os.remove(FILE_PATH)
