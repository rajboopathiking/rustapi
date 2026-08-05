import io
import sqlite3
import threading
import time
import requests
import jwt
import httpx
import numpy as np
from PIL import Image
from pydantic import BaseModel, Field
from rustapi import FastAPI, Request, Depends, HTTPException, status
from rustapi.uploads import UploadFile


# 1. Pydantic Model
class ImageAnalysisRequest(BaseModel):
    project_name: str = Field(..., min_length=2)
    threshold: float = Field(0.5, ge=0.0, le=1.0)


# 2. Database Setup (SQLite stdlib)
def init_db():
    conn = sqlite3.connect(":memory:", check_same_thread=False)
    conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT, token TEXT)")
    conn.execute("INSERT INTO users (username, token) VALUES ('alice', 'secret_jwt_123')")
    conn.commit()
    return conn

db_conn = init_db()

# DB Dependency
def get_db():
    return db_conn


# 3. Application
app = FastAPI(title="Ecosystem Compatibility Test App")


@app.post("/analyze-image")
async def analyze_image(req: Request, db=Depends(get_db)):
    # A. Parse form fields or JSON body with Pydantic
    try:
        json_body = await req.json()
    except Exception:
        json_body = {}

    project_name = req.form.get("project_name") or json_body.get("project_name") or "default_project"
    threshold = float(req.form.get("threshold") or json_body.get("threshold") or 0.5)
    model = ImageAnalysisRequest(project_name=project_name, threshold=threshold)

    # B. Query SQLite database
    cursor = db.cursor()
    cursor.execute("SELECT username FROM users WHERE token = 'secret_jwt_123'")
    row = cursor.fetchone()
    user = row[0] if row else "unknown"

    # C. Read uploaded file & process image with PIL (Pillow) & NumPy
    if "photo" not in req.files or not req.files["photo"]:
        raise HTTPException(status_code=400, detail="No photo uploaded")

    photo_file: UploadFile = req.files["photo"][0]
    img_bytes = await photo_file.read()

    image = Image.open(io.BytesIO(img_bytes)).convert("L")
    img_array = np.array(image)

    # Perform NumPy calculation (mean brightness)
    mean_brightness = float(np.mean(img_array))

    # D. JWT Encoding
    payload = {"sub": user, "project": model.project_name, "brightness": mean_brightness}
    encoded_token = jwt.encode(payload, "secret_key_999", algorithm="HS256")

    return {
        "status": "success",
        "user": user,
        "project": model.project_name,
        "mean_brightness": mean_brightness,
        "jwt_token": encoded_token,
    }


def test_python_ecosystem_compatibility():
    port = 8996
    thread = threading.Thread(target=lambda: app.run(host="127.0.0.1", port=port), daemon=True)
    thread.start()
    time.sleep(0.4)

    # Generate sample 100x100 grayscale image in memory using PIL
    img = Image.new("L", (100, 100), color=128)
    img_byte_arr = io.BytesIO()
    img.save(img_byte_arr, format="PNG")
    png_bytes = img_byte_arr.getvalue()

    # Call endpoint using HTTPX
    with httpx.Client(base_url=f"http://127.0.0.1:{port}") as client:
        files = {"photo": ("test_image.png", png_bytes, "image/png")}
        data = {"project_name": "SkinAnalysis", "threshold": "0.75"}

        response = client.post("/analyze-image", data=data, files=files)
        assert response.status_code == 200

        res_json = response.json()
        assert res_json["status"] == "success"
        assert res_json["user"] == "alice"
        assert res_json["project"] == "SkinAnalysis"
        assert res_json["mean_brightness"] == 128.0

        # Decode JWT token to verify PyJWT compatibility
        decoded = jwt.decode(res_json["jwt_token"], "secret_key_999", algorithms=["HS256"])
        assert decoded["sub"] == "alice"
        assert decoded["project"] == "SkinAnalysis"
        assert decoded["brightness"] == 128.0
