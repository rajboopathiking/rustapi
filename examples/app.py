"""
RustAPI example: FastAPI-style Python surface, Rust core underneath.
Run: python examples/app.py
"""
import rustapi
from pydantic import BaseModel

app = rustapi.Engine()


db = app.connect_db("sqlite::memory:") # or "postgres://user:pass@localhost/db"
  
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob')")

class UserOut(BaseModel):                                                                                                                                                        
    id: int                                                                                                                                                                      
    username: str
    email: str

@app.get("/hello")
def hello():
    return {"message": "hello from a Python function, routed by Rust"}


@app.get("/users/{user_id}")
def get_user(request):
    return {"user_id": request.path_params["user_id"], "requested_via": request.path}


@app.get("/search")
def search(request):
    return {"query_params": request.query_params}


@app.post("/echo")
def echo(request):
    return {"you_sent": request.body}

@app.get("/stream-test")
def stream_route():
    def generator():
        yield "Hello "
        yield "from "
        yield "RustAPI!"
    return rustapi.StreamingResponse(generator(), media_type="text/plain")                                                                                                                                                                                  
  
@app.get("/users")
def get_users():
        # Executes SQL in Rust and streams JSON directly to HTTP response
    return db.query_json("SELECT * FROM users")

@app.get("/user", response_model=UserOut)
def get_user():
        return {
            "id": 1,
            "username": "boopathi",
            "password_hash": "secret_hash",  # Filtered out automatically
            "email": "user@example.com",
        }

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000,reload=True)
