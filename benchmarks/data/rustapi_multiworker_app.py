
import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi

app = rustapi.Engine()

# Tier 3: Rust-Native Fast-Path Route (0ms Python GIL overhead)
app.add_native_route("/fast-json", '{"status":"ok","engine":"pure_rust_tier3"}')

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8096, workers=4)
