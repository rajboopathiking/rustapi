
import uvicorn
import sqlite3
from fastapi import FastAPI
from fastapi.responses import HTMLResponse, JSONResponse
from jinja2 import Template
import jwt

app = FastAPI()

conn = sqlite3.connect(":memory:", check_same_thread=False)
conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
conn.executemany("INSERT INTO users (name, email) VALUES (?, ?)", [("User " + str(i), f"user{i}@example.com") for i in range(50)])
conn.commit()

@app.get("/json")
def get_json():
    return {"status": "ok", "message": "hello"}

@app.get("/sql")
def get_sql():
    cursor = conn.cursor()
    cursor.execute("SELECT id, name, email FROM users")
    rows = cursor.fetchall()
    users = [{"id": r[0], "name": r[1], "email": r[2]} for r in rows]
    return users

@app.get("/render")
def get_render():
    template = Template("<h1>Welcome {{ name }}!</h1><p>Active items: {{ items | length }}</p>")
    html = template.render(name="Boopathi", items=["A", "B", "C", "D"])
    return HTMLResponse(html)

@app.post("/auth/jwt")
def auth_jwt():
    token = jwt.encode({"sub": "user_42", "role": "admin"}, "secret_key", algorithm="HS256")
    claims = jwt.decode(token, "secret_key", algorithms=["HS256"])
    return claims

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8091, log_level="error")
