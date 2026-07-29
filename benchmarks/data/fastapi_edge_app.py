
import uvicorn
import sqlite3
from fastapi import FastAPI
from fastapi.responses import JSONResponse
import passlib.hash

app = FastAPI()

conn = sqlite3.connect(":memory:", check_same_thread=False)
conn.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, title TEXT, content TEXT)")
values = [("Item " + str(i), "Long content description text " * 5) for i in range(200)]
conn.executemany("INSERT INTO items (title, content) VALUES (?, ?)", values)
conn.commit()

@app.get("/items/category/{cat_id}/subcategory/{sub_id}/item/{item_id}")
def deep_route(cat_id: int, sub_id: int, item_id: int):
    return {"cat": cat_id, "sub": sub_id, "item": item_id}

@app.get("/large-query")
def large_query():
    cursor = conn.cursor()
    cursor.execute("SELECT id, title, content FROM items")
    rows = cursor.fetchall()
    return [{"id": r[0], "title": r[1], "content": r[2]} for r in rows]

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8093, log_level="error")
