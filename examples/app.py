"""
RustAPI Example Application: FastAPI-style Python surface, Rust core underneath.
Run: python examples/app.py
"""
import json
import rustapi
from pydantic import BaseModel

app = rustapi.Engine()

# ---- 1. Embedded Rust-Native Database Engine ----
db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob')")
db.execute(
    "CREATE TABLE recent_uploads ("
    "id INTEGER PRIMARY KEY AUTOINCREMENT, "
    "filename TEXT, "
    "content_type TEXT, "
    "size INTEGER, "
    "description TEXT, "
    "uploaded_at TEXT DEFAULT CURRENT_TIMESTAMP"
    ")"
)


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
recent_uploads = []


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


# ---- 4. File Uploads & Recent Uploads ----
@app.post("/upload")
def upload_file(req):
    """
    Upload a file: Parsed natively in Rust Tokio worker threads into req.files.
    Stores metadata into SQLite database and maintains a recent uploads buffer.
    """
    files = req.files.get("file", [])
    if not files:
        return rustapi.JSONResponse(
            {"error": "No file uploaded. Send a multipart form field named 'file'."},
            status_code=400,
        )

    doc = files[0]
    content = doc.read()
    filename = doc.filename or "uploaded_file.bin"
    content_type = doc.content_type or "application/octet-stream"
    size = len(content)
    description = req.form.get("description", "No description provided")

    # Sanitize inputs for single-string SQL execution
    safe_fn = filename.replace("'", "''")
    safe_ct = content_type.replace("'", "''")
    safe_desc = description.replace("'", "''")

    # Persist file metadata in embedded Rust-native SQLite database
    db.execute(
        f"INSERT INTO recent_uploads (filename, content_type, size, description) "
        f"VALUES ('{safe_fn}', '{safe_ct}', {size}, '{safe_desc}')"
    )

    record = {
        "id": len(recent_uploads) + 1,
        "filename": filename,
        "content_type": content_type,
        "size": size,
        "description": description,
        "preview": content.decode("utf-8", errors="ignore")[:100] if size > 0 else "",
    }
    recent_uploads.append(record)
    if len(recent_uploads) > 20:
        recent_uploads.pop(0)

    return {"message": "File uploaded successfully", "uploaded_file": record}


@app.get("/uploads/recent")
def get_recent_uploads():
    """
    Get recent file uploads: Returns recent upload metadata from memory
    and queries records from the embedded SQLite database.
    """
    db_res = db.query_json("SELECT * FROM recent_uploads ORDER BY id DESC LIMIT 10")
    db_records = json.loads(db_res.content) if hasattr(db_res, "content") else []
    return {
        "recent_in_memory": list(reversed(recent_uploads[-10:])),
        "recent_from_db": db_records,
    }


@app.get("/uploads/recent/db")
def get_recent_uploads_db():
    """
    Streams JSON directly from the SQLite database to the HTTP response.
    """
    return db.query_json("SELECT * FROM recent_uploads ORDER BY id DESC LIMIT 10")



# ---- 5. Native MCP (Model Context Protocol) Tools ----
@app.tool()
def add_numbers(a: int, b: int) -> int:
    """Add two numbers."""
    return a + b


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000, reload=True)

