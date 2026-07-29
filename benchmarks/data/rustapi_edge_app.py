
import rustapi

app = rustapi.Engine()

db = app.connect_db("sqlite::memory:")
db.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, title TEXT, content TEXT)")
values_str = ", ".join([f"('Item {i}', 'Long content description text Long content description text Long content description text')" for i in range(200)])
db.execute(f"INSERT INTO items (title, content) VALUES {values_str}")

@app.get("/items/category/{cat_id}/subcategory/{sub_id}/item/{item_id}")
def deep_route(cat_id: int, sub_id: int, item_id: int):
    return {"cat": cat_id, "sub": sub_id, "item": item_id}

@app.get("/large-query")
def large_query():
    return db.query_json("SELECT id, title, content FROM items")

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8094)
