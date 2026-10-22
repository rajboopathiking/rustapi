import os
import signal
import sys
import time
import subprocess
import requests
import pytest

HOST = "127.0.0.1"


def test_native_run_startup_completes_before_serving():
    """Verify native app.run() executes startup handler before accepting requests."""
    port = 8111
    script = f"""import sys, time
from rustapi import FastAPI

app = FastAPI()
state = {{"ready": False}}

@app.on_event("startup")
def startup():
    time.sleep(0.5)
    state["ready"] = True

@app.get("/status")
def get_status():
    return {{"ready": state["ready"]}}

if __name__ == "__main__":
    app.run(host="{HOST}", port={port})
"""
    scratch_file = "scratch/tmp_test_native_startup.py"
    os.makedirs("scratch", exist_ok=True)
    with open(scratch_file, "w") as f:
        f.write(script)

    proc = subprocess.Popen(
        [sys.executable, scratch_file],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=(sys.platform != "win32"),
    )

    try:
        # Wait for server to respond
        res = None
        for _ in range(40):
            try:
                res = requests.get(f"http://{HOST}:{port}/status", timeout=1.0)
                if res.status_code == 200:
                    break
            except Exception:
                time.sleep(0.1)

        assert res is not None, "Server did not respond in time"
        assert res.status_code == 200
        # Startup handler must have completed before serving this request!
        assert res.json() == {"ready": True}

    finally:
        try:
            if sys.platform != "win32":
                os.killpg(proc.pid, signal.SIGINT)
            else:
                proc.terminate()
            stdout, stderr = proc.communicate(timeout=3)
        except Exception:
            try:
                if sys.platform != "win32":
                    os.killpg(proc.pid, signal.SIGKILL)
            except Exception:
                pass
            proc.kill()
            stdout, stderr = proc.communicate()

        if os.path.exists(scratch_file):
            os.remove(scratch_file)


def test_native_run_startup_exception_aborts_server():
    """Verify a startup exception prevents the server from serving requests and exits with failure."""
    port = 8112
    script = f"""import sys
from rustapi import FastAPI

app = FastAPI()

@app.on_event("startup")
def bad_startup():
    raise RuntimeError("Intentional Database Connection Failure on Startup")

@app.get("/should_not_reach")
def unreachable():
    return {{"reached": True}}

if __name__ == "__main__":
    app.run(host="{HOST}", port={port})
"""
    scratch_file = "scratch/tmp_test_native_bad_startup.py"
    os.makedirs("scratch", exist_ok=True)
    with open(scratch_file, "w") as f:
        f.write(script)

    proc = subprocess.Popen(
        [sys.executable, scratch_file],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=(sys.platform != "win32"),
    )

    stdout, stderr = "", ""
    try:
        stdout, stderr = proc.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, stderr = proc.communicate()

    if os.path.exists(scratch_file):
        os.remove(scratch_file)

    # Process must have exited with non-zero return code due to startup exception
    assert proc.returncode != 0, f"Expected non-zero exit code, got {proc.returncode}"
    combined = stdout + stderr
    assert "Application startup failed" in combined or "Intentional Database Connection Failure" in combined


def test_native_run_graceful_shutdown_executes_hooks():
    """Verify shutdown hooks execute and finish when SIGINT is sent to app.run()."""
    port = 8113
    marker_file = os.path.abspath("scratch/shutdown_hook_done.txt")
    if os.path.exists(marker_file):
        os.remove(marker_file)

    script = f"""import os, time
from rustapi import FastAPI

app = FastAPI()

@app.on_event("shutdown")
def on_shutdown():
    with open(r"{marker_file}", "w") as f:
        f.write("SHUTDOWN_COMPLETED")

@app.get("/ping")
def ping():
    return {{"status": "ok"}}

if __name__ == "__main__":
    app.run(host="{HOST}", port={port})
"""
    scratch_file = "scratch/tmp_test_native_shutdown.py"
    os.makedirs("scratch", exist_ok=True)
    with open(scratch_file, "w") as f:
        f.write(script)

    proc = subprocess.Popen(
        [sys.executable, scratch_file],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=(sys.platform != "win32"),
    )

    try:
        # Wait for server to be responsive
        connected = False
        for _ in range(40):
            try:
                r = requests.get(f"http://{HOST}:{port}/ping", timeout=1.0)
                if r.status_code == 200:
                    connected = True
                    break
            except Exception:
                time.sleep(0.1)

        assert connected, "Server failed to start in time"

        # Trigger graceful shutdown via SIGINT
        if sys.platform != "win32":
            os.killpg(proc.pid, signal.SIGINT)
        else:
            proc.terminate()

        stdout, stderr = proc.communicate(timeout=5)

        # Verify shutdown hook executed
        assert os.path.exists(marker_file), "Shutdown hook marker file was not created"
        with open(marker_file) as f:
            content = f.read()
        assert content == "SHUTDOWN_COMPLETED"

    finally:
        try:
            proc.kill()
            proc.communicate()
        except Exception:
            pass
        if os.path.exists(scratch_file):
            os.remove(scratch_file)
        if os.path.exists(marker_file):
            os.remove(marker_file)
