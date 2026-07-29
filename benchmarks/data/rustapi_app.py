
import rustapi

app = rustapi.Engine()

db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
for i in range(50):
    db.execute(f"INSERT INTO users (name, email) VALUES ('User {i}', 'user{i}@example.com')")

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

if __name__ == "__main__":
    import os
    os.environ["RUSTAPI_LOG"] = "0"
    app.run(host="127.0.0.1", port=8092)
