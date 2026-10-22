import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi
from pydantic import BaseModel
from typing import List

app = rustapi.Engine()

class Item(BaseModel):
    name: str
    price: float
    quantity: int

class UserOut(BaseModel):
    id: int
    name: str
    email: str
    role: str

USERS_RAW = [
    {
        "id": i,
        "name": f"User_{i}",
        "email": f"user{i}@example.com",
        "role": "admin" if i % 2 == 0 else "user",
        "secret": "hidden_token",
    }
    for i in range(50)
]

@app.get("/json")
def get_json():
    return {"status": "ok", "message": "hello"}

@app.get("/async_json")
async def get_async_json():
    return {"status": "ok", "message": "async hello"}

@app.post("/items")
def create_item(item: Item):
    return {"name": item.name, "total": item.price * item.quantity}

@app.get("/users", response_model=List[UserOut])
def get_users():
    return USERS_RAW

# Pure Rust Native Tier 3 Fast-Path
app.add_native_route("/native/json", '{"status":"ok","message":"native hello"}')

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8092)
