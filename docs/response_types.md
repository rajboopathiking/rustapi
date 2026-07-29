# 📤 Response Types & Request Ergonomics

RustAPI provides specialized response wrappers and clean request body parsing.

---

## 🎯 Response Classes

```python
import rustapi

# 1. HTML Response (Content-Type: text/html; charset=utf-8)
@app.get("/page")
def html_page():
    return rustapi.HTMLResponse("<h1>Hello World</h1>")

# 2. JSON Response (Content-Type: application/json)
@app.get("/api/data")
def json_data():
    return rustapi.JSONResponse({"status": "success"})

# 3. Plain Text Response (Content-Type: text/plain; charset=utf-8)
@app.get("/text")
def plain_text():
    return rustapi.PlainTextResponse("Hello plain text")

# 4. Redirect Response (Status 307)
@app.get("/old-path")
def redirect():
    return rustapi.RedirectResponse("/page")

# 5. Chunked Streaming Response
@app.get("/stream")
def stream_data():
    def generator():
        yield "Chunk 1\n"
        yield "Chunk 2\n"
    return rustapi.StreamingResponse(generator(), media_type="text/plain")
```

---

## 📥 Request Body & Form Parsing

Inspect incoming requests via `req.json()`, `req.form`, `req.body`, `req.headers`, and `req.cookies`:

```python
@app.post("/login")
def login_handler(req):
    # Parse JSON body payload
    json_data = req.json()
    
    # Access Form or Query parameter fallback
    username = json_data.get("username") or req.form.get("username")
    
    return {"user": username, "user_agent": req.headers.get("user-agent")}
```
