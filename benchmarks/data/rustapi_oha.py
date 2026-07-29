
import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi

app = rustapi.Engine()

db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
values_str = ", ".join([f"('User {i}', 'user{i}@example.com')" for i in range(100)])
db.execute(f"INSERT INTO users (name, email) VALUES {values_str}")

# Tier 2: Hybrid Handlers
@app.get("/json")
def get_json():
    return {"status": "ok", "message": "hello"}

@app.get("/sql")
def get_sql():
    return db.query_json("SELECT id, name, email FROM users")

@app.get("/render")
def get_render():
    html = rustapi.render_template(
        "<h1>Welcome {{ name }}!</h1><p>Active items: {{ items | length }}</p>",
        {"name": "Boopathi", "items": ["A", "B", "C", "D"]}
    )
    return rustapi.HTMLResponse(html)

@app.post("/auth/jwt")
def auth_jwt():
    token = rustapi.encode_jwt({"sub": "user_42", "role": "admin"}, secret="secret_key")
    claims = rustapi.decode_jwt(token, secret="secret_key")
    return claims

# Tier 3: Pure Rust Native Route (0ms GIL / 0ms Bytecode)
app.add_native_route("/native/json", '{"status":"ok","engine":"pure_rust_tier3"}')
app.add_native_route("/native/render", '<h1>Welcome Boopathi!</h1><p>Active items: 4</p>', content_type="text/html")

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8092)
