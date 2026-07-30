import uvicorn
import sqlite3
import os
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

DB_PATH = "crud_benchmark/benchmark_db.sqlite"

app = FastAPI()

def get_db():
    conn = sqlite3.connect(DB_PATH, check_same_thread=False)
    conn.row_factory = sqlite3.Row
    return conn

class BookCreate(BaseModel):
    title: str
    author: str
    price: float

class BookUpdate(BaseModel):
    price: float

@app.get("/books")
def list_books():
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("SELECT id, title, author, price FROM books LIMIT 1000")
    rows = cursor.fetchall()
    return [{"id": r["id"], "title": r["title"], "author": r["author"], "price": r["price"]} for r in rows]

@app.get("/books/{book_id}")
def get_book(book_id: int):
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("SELECT id, title, author, price FROM books WHERE id = ?", (book_id,))
    row = cursor.fetchone()
    if not row:
        raise HTTPException(status_code=404, detail="Book not found")
    return {"id": row["id"], "title": row["title"], "author": row["author"], "price": row["price"]}

@app.post("/books")
def create_book(book: BookCreate):
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("INSERT INTO books (title, author, price) VALUES (?, ?, ?)", (book.title, book.author, book.price))
    conn.commit()
    return {"status": "created", "id": cursor.lastrowid}

@app.put("/books/{book_id}")
def update_book(book_id: int, book: BookUpdate):
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("UPDATE books SET price = ? WHERE id = ?", (book.price, book_id))
    conn.commit()
    return {"status": "updated", "id": book_id}

@app.delete("/books/{book_id}")
def delete_book(book_id: int):
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("DELETE FROM books WHERE id = ?", (book_id,))
    conn.commit()
    return {"status": "deleted", "id": book_id}

if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=8098, log_level="error")
