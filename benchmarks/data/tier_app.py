
import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi
import math

app = rustapi.Engine()
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO items (name) VALUES ('Item A'), ('Item B'), ('Item C')")

# ==== TIER 1: Pure Python Handler (Standard FastAPI Style) ====
@app.get("/tier1/python-math")
def tier1_handler():
    # Math calculation performed in Python bytecode
    total = sum([math.sqrt(i * 1.5) for i in range(100)])
    return {"tier": 1, "result": total}

# ==== TIER 2: Hybrid Surface (Python Route + Embedded Rust Primitives) ====
@app.get("/tier2/rust-db")
def tier2_db():
    # SQL query executed in Rust sqlx, zero-copy JSON stream to socket
    return db.query_json("SELECT * FROM items")

@app.get("/tier2/rust-template")
def tier2_template():
    # MiniJinja template rendered natively in Rust memory
    html = rustapi.render_template("<h1>Hello {{ name }}</h1>", {"name": "Boopathi"})
    return rustapi.HTMLResponse(html)

@app.post("/tier2/rust-jwt")
def tier2_jwt():
    # Native jsonwebtoken crate in Rust
    token = rustapi.encode_jwt({"sub": "user_42"}, secret="secret")
    claims = rustapi.decode_jwt(token, secret="secret")
    return claims

# ==== TIER 3: Rust-Native Business Logic (Pre-compiled C-Speed Route) ====
app.add_native_route("/tier3/rust-native", '{"tier": 3, "performance": "pure_rust_c_speed"}')

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8097)
