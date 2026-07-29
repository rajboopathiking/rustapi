"""
RustAPI Example Application: FastAPI-style Python surface, Rust core underneath.
Run: python examples/app.py
"""
import rustapi
from pydantic import BaseModel

app = rustapi.Engine()

# ---- 1. Embedded Rust-Native Database Engine ----
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob')")


# ---- 2. Pydantic Response Model Schemas ----
class UserOut(BaseModel):
    id: int
    username: str
    email: str

class UserIn(BaseModel):
    username: str
    password: str
    email: str


# ---- 3. Route Handlers ----
@app.get("/")
async def root():
    return {"message": "Hello from RustAPI!"}


@app.get("/hello")
def hello():
    return {"message": "Hello from a Python function, routed by Rust"}


@app.get("/users/{user_id}")
def get_user_by_id(request):
    return {"user_id": request.path_params["user_id"], "requested_via": request.path}


@app.post("/create_user")
async def create_user(user: UserIn):
        # Here you would typically insert the user into the database
    return {"message": f"User {user.username} created successfully!"}

@app.get("/search")
def search(request):
    return {"query_params": request.query_params}


@app.get("/users")
def get_users():
    # Executes SQL in Rust and streams JSON directly to HTTP response
    return db.query_json("SELECT * FROM users")


@app.get("/user", response_model=UserOut)
def get_single_user():
    return {
        "id": 1,
        "username": "boopathi",
        "password_hash": "secret_hash",  # Filtered out automatically by UserOut schema
        "email": "user@example.com",
    }


@app.get("/render")
async def render_html():
    template = "<h1>Hello {{ name }}! Welcome to RustAPI.</h1>"
    rendered_html = rustapi.render_template(template, {"name": "Boopathi"})
    return rustapi.HTMLResponse(rendered_html)


@app.post("/auth/hash")
def hash_endpoint(req):
    data = req.json()
    raw_pass = data.get("password") or req.form.get("password", "MyDefaultSecret123!")

    h = rustapi.hash_password(raw_pass)
    token = rustapi.encode_jwt({"sub": "user_1"}, "my_secret_key")
    return rustapi.JSONResponse({"hash": h, "token": token})


@app.get("/stream")
def stream_route():
    def generator():
        yield "Hello "
        yield "from "
        yield "RustAPI!"

    return rustapi.StreamingResponse(generator(), media_type="text/plain")


# ---- 4. Native MCP (Model Context Protocol) Tools ----
@app.tool()
def add_numbers(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000, reload=True)
