from rustapi import Engine, Response, HTTPException,Depends,BackgroundTasks,WebSocket
from pydantic import BaseModel
import asyncio
import uuid
from routers_test import router
import time

app = Engine()

app.include_router(router)  

@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    await websocket.accept()
    print("🔌 WebSocket client connected!")
    while True:
        try:
            data = await websocket.receive_text()
            print(f"📩 Received: {data}")
            await websocket.send_text(f"Echo from RustAPI: {data}")
        except Exception:
            break

def process_image(image_id: str, effect: str):
    print(f"\n🖼️  [Background Task Started] Processing image {image_id} with effect: {effect}...")
    time.sleep(3) # Simulating heavy CPU lifting (e.g., image resizing)
    print(f"✅  [Background Task Finished] Image {image_id} complete!\n")

# 2. This is the endpoint that triggers it
# @app.post("/upload")
# def upload_file(bg: BackgroundTasks):
#     print("\n📡 [Route Handler] Handling incoming upload request...")
    
#     # Push the task to Tokio's background thread pool natively in Rust
#     bg.add_task(process_image, "IMG_001.jpg", effect="sepia")
    
#     print("🚀 [Route Handler] Returning HTTP Response immediately!")
#     # Response returns instantly!
#     return {"message": "Upload successful! Processing in background."}

@app.post("/upload")
def handle_upload(req):
    description = req.form.get("description", "No description provided")
    documents = req.files.get("document", [])
    
    if not documents:
        return {"error": "No file uploaded"}
        
    file = documents[0]
    file_bytes = file.read()
    
    return {
        "message": "File successfully uploaded and parsed by Rust!",
        "filename": file.filename,
        "content_type": file.content_type,
        "size_bytes": len(file_bytes),
        "form_description": description
    }

@app.get("/")
def health_check():
    return {"status": "Master App is running normally."}

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
    host = "127.0.0.1"
    port = 8000
    print(f"Starting RustAPI server on http://{host}:{port} ...")
    app.run(host=host, port=port)