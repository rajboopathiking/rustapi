# from rustapi import Engine
# from pydantic import BaseModel
# import asyncio

# app = Engine()

# class Item(BaseModel):
#     name: str
#     price: float
#     quantity: int

# # 1. ASYNC I/O TEST 
# @app.get("/db-fetch/{item_id}")
# async def simulate_db(req):
#     item_id = req.path_params.get("item_id")
#     await asyncio.sleep(0.05)
#     return {"item_id": item_id, "status": "fetched_from_db"}

# # 2. JSON VALIDATION TEST 
# @app.post("/orders")
# def create_order(item: Item):
#     total = item.price * item.quantity
#     return {"name": item.name, "total_price": total, "status": "processed"}

# # 3. CPU BOUND TEST
# @app.get("/compute")
# def compute_heavy():
#     total = sum(i * i for i in range(10_000))
#     return {"total": total}

# if __name__ == "__main__":
#     app.run(host="127.0.0.1", port=8001)


from rustapi import Engine
from pydantic import BaseModel
import asyncio



import json

# 1. Generate large.json (for /bulk)
products = {"items": [{"id": i, "name": f"P{i}", "price": 1.5} for i in range(20000)]}
with open("large.json", "w") as f: json.dump(products, f)

# 2. Generate validate.json (for /validate)
users = {"users": [{"id": i, "name": "User", "age": 30, "addresses": [{"street": "Main", "city": "NYC", "state": "NY", "zip": "10001"}]} for i in range(5000)]}
with open("validate.json", "w") as f: json.dump(users, f)

# 3. Generate mixed.json (for /mixed)
with open("mixed.json", "w") as f: json.dump({"name": "Test", "price": 99.9, "quantity": 5}, f)

app = Engine()

# --- MODELS ---
class Product(BaseModel): id: int; name: str; price: float
class Products(BaseModel): items: list[Product]

class Address(BaseModel): street: str; city: str; state: str; zip: str
class User(BaseModel): id: int; name: str; age: int; addresses: list[Address]
class Payload(BaseModel): users: list[User]

class Item(BaseModel): name: str; price: float; quantity: int

# 1. MASSIVE JSON SERIALIZATION
@app.get("/large-json")
def large_json():
    return {
        "items": [
            {"id": i, "name": f"Item {i}", "price": i * 0.5, "active": True, "tags": ["a", "b", "c"]}
            for i in range(10000)
        ]
    }

# 2. LARGE REQUEST BODY PARSING
@app.post("/bulk")
def bulk(products: Products):
    return {"count": len(products.items)}

# 3. CPU-INTENSIVE COMPUTATION
@app.get("/prime")
def prime():
    count = 0
    for n in range(2, 50000):
        is_prime = True
        for i in range(2, int(n**0.5)+1):
            if n % i == 0:
                is_prime = False
                break
        if is_prime: count += 1
    return {"count": count}

# 4. CONCURRENT ASYNC WORKLOAD
@app.get("/parallel")
async def parallel():
    await asyncio.gather(*(asyncio.sleep(0.05) for _ in range(5)))
    return {"done": True}

# 5. HEAVY VALIDATION
@app.post("/validate")
def validate(payload: Payload):
    return {"users": len(payload.users)}

# 6. ROUTE PARAMS (Routing Overhead)
@app.get("/users/{uid}/orders/{oid}/items/{iid}")
def route(req):
    # Extracting from your framework's req object
    return {
        "uid": int(req.path_params.get("uid", 0)),
        "oid": int(req.path_params.get("oid", 0)),
        "iid": int(req.path_params.get("iid", 0))
    }

# 7. MIXED WORKLOAD
@app.post("/mixed")
async def mixed(item: Item):
    await asyncio.sleep(0.02)
    total = sum(i * i for i in range(50000))
    return {"name": item.name, "total": total, "price": item.price * item.quantity}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8001)