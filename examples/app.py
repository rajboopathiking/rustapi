"""
RustAPI example: FastAPI-style Python surface, Rust core underneath.
Run: python examples/app.py
"""
import rustapi

app = rustapi.Engine()


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


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
