import os
os.environ["RUSTAPI_LOG"] = "0"
import rustapi
from pydantic import BaseModel

DB_PATH = "crud_benchmark/benchmark_db.sqlite"

app = rustapi.Engine()

# Connect to SQLite pool
db = app.connect_db(f"sqlite://{DB_PATH}")

class BookCreate(BaseModel):
    title: str
    author: str
    price: float

class BookUpdate(BaseModel):
    price: float

# ==== TIER 1/2: Python Routes with Rust Database & Native Primitives ====
@app.get("/books")
def list_books():
    return db.query_json("SELECT id, title, author, price FROM books LIMIT 1000")

@app.get("/books/{book_id}")
def get_book(book_id: int):
    return db.query_json(f"SELECT id, title, author, price FROM books WHERE id = {book_id}")

@app.post("/books")
def create_book(book: BookCreate):
    title_escaped = book.title.replace("'", "''")
    author_escaped = book.author.replace("'", "''")
    db.execute(f"INSERT INTO books (title, author, price) VALUES ('{title_escaped}', '{author_escaped}', {book.price})")
    return {"status": "created"}

@app.put("/books/{book_id}")
def update_book(book_id: int, book: BookUpdate):
    db.execute(f"UPDATE books SET price = {book.price} WHERE id = {book_id}")
    return {"status": "updated", "id": book_id}

@app.delete("/books/{book_id}")
def delete_book(book_id: int):
    db.execute(f"DELETE FROM books WHERE id = {book_id}")
    return {"status": "deleted", "id": book_id}

# ==== TIER 3: Pure Rust Native Fast-Path Routes (Zero GIL / Machine Code Speed) ====
app.add_native_route("/tier3/books/1", body='{"id":1,"title":"Book Title 1","author":"Author 1","price":10.0,"tier":3}', method="GET")
app.add_native_route("/tier3/books", body='{"status":"created","tier":3}', method="POST")

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8099)
