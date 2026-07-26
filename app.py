from rustapi import Engine, Response, HTTPException,Depends
from pydantic import BaseModel
import asyncio
import uuid

app = Engine()

class User(BaseModel):
    name: str

def get_db():
    # Simulate a database connection
    return {"db": "connected"}

# 1. Automatic 422 generation if JSON payload doesn't match User schema
# @app.post("/users")
# def create_user(user: User):
#     # 2. Custom Response with 201 Created and custom Headers
#     return Response({"message": f"Created {user.name}"}, status_code=201, headers={"X-App": "RustAPI"})

@app.get("/")
async def root():
    return {"message": "hello"}

@app.get("/users")
def get_users(db = Depends(get_db)):
    return {"data": db}

# 1. Sync Dependency with Caching (Request Scoping)
def get_request_id():
    req_id = str(uuid.uuid4())[:8]
    print(f"   [Dependency] Generated new request ID: {req_id}")
    return req_id

# 2. Async Dependency
async def get_user_token():
    print("   [Dependency] Fetching async token...")
    await asyncio.sleep(0.1) # Simulate async I/O
    return "async_token_999"

# 3. Generator Dependency (Setup / Teardown)
def db_session():
    print("   [Dependency] ---> DB Session OPENED")
    yield "db_conn_active"
    print("   [Dependency] <--- DB Session CLOSED (Teardown completed)")

@app.get("/test-depends")
def test_depends_route(
    req_id_1=Depends(get_request_id),
    req_id_2=Depends(get_request_id),  # Should hit cache! No second print.
    token=Depends(get_user_token),
    db=Depends(db_session)
):
    print("   [Route] Executing main handler...")
    return {
        "message": "Depends system fully operational!",
        "req_id_1": req_id_1,
        "req_id_2": req_id_2,
        "caching_working": req_id_1 == req_id_2,
        "token": token,
        "db_status": db
    }

# 3. Accessing Headers & Cookies
@app.get("/auth")
def auth(req):
    token = req.headers.get("authorization")
    if not token:
        # 4. Structured HTTPException that translates directly to a JSON HTTP response
        raise HTTPException(status_code=401, detail="Unauthorized request")
    
    return {"status": "success", "session": req.cookies.get("session_id")}


if __name__ == "__main__":
    app.run(host="127.0.0.1",port=8000)